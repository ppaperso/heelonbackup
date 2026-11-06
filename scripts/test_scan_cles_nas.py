"""Synthetic, local-only regression tests for the read-only NAS scanner."""

import errno
import json
import os
from pathlib import Path
import pty
import select
import subprocess
import sys
import tempfile
import unittest


SCRIPT = Path(__file__).with_name("scan-cles-nas.sh")
MARKER = b"-----BEGIN OPENSSH PRIVATE KEY-----"


class ScannerTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.env = {**os.environ, "MAX_BYTES": "0", "TERM": "dumb"}

    def run_scan(self, *args, limit="0"):
        return subprocess.run(
            ["bash", str(SCRIPT), str(self.root), *args],
            env={**self.env, "MAX_BYTES": limit},
            capture_output=True,
            text=True,
            timeout=20,
        )

    def test_clean_public_files_and_suspect_names(self):
        (self.root / "id_test.pub").write_text("ssh-ed25519 synthetic-public-key")
        (self.root / "id_test").write_text("not a private key")
        result = self.run_scan()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.count("NOM_SUSPECT"), 1)
        self.assertNotIn("CLE_PRIVEE", result.stdout)
        self.assertNotIn("\x1b", result.stdout + result.stderr)
        self.assertNotIn("\r", result.stderr)
        self.assertIn("analyses=2/2", result.stdout)

    def test_binary_embedded_markers_and_split_reads(self):
        (self.root / "binary").write_bytes(b"\x00prefix" + MARKER + b"\x00")
        (self.root / "split").write_bytes(b"x" * (256 * 1024 - 10) + MARKER)
        (self.root / "putty").write_bytes(b"PuTTY-User-Key-File-3: ssh-ed25519")
        (self.root / "pem").write_bytes(b"-----BEGIN ENCRYPTED PRIVATE KEY-----")
        result = self.run_scan()
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertEqual(result.stdout.count("CLE_PRIVEE"), 4)
        self.assertNotIn("PRIVATE KEY-----", result.stdout + result.stderr)
        self.assertNotIn("PuTTY-User-Key", result.stdout + result.stderr)

    def test_size_limit_is_inclusive_and_omissions_are_incomplete(self):
        (self.root / "exact").write_bytes(MARKER)
        result = self.run_scan(limit=str(len(MARKER)))
        self.assertEqual(result.returncode, 1, result.stderr)
        (self.root / "too-large").write_bytes(MARKER + b"x")
        result = self.run_scan(limit=str(len(MARKER)))
        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertIn("trop_gros=1", result.stdout)
        self.assertIn("INCOMPLETE", result.stderr)
        result = self.run_scan()
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn("marqueurs=2", result.stdout)

    def test_unusual_names_are_escaped_not_terminal_commands(self):
        name = "renamed\nkey\t\x1b[31m"
        (self.root / name).write_bytes(MARKER)
        result = self.run_scan()
        self.assertEqual(result.returncode, 1, result.stderr)
        findings = [line for line in result.stdout.splitlines() if line.startswith("CLE_PRIVEE\t")]
        self.assertEqual(len(findings), 1)
        self.assertEqual(json.loads(findings[0].split("\t", 1)[1]), str(self.root / name))
        self.assertNotIn("\x1b", result.stdout)

    def test_symlinks_and_fifo_are_not_read(self):
        (self.root / "link").symlink_to("/does-not-exist")
        os.mkfifo(self.root / "fifo")
        result = self.run_scan()
        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertIn("2 liens/fichiers speciaux/autres montages ignores", result.stderr)
        self.assertIn("analyses=0/0", result.stdout)
        self.assertIn("aucun fichier analysable", result.stderr)
        self.assertNotIn("Aucun marqueur trouve", result.stderr)

    def test_empty_directory_is_not_a_successful_nas_search(self):
        result = self.run_scan()
        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertIn("partage SMB est monte", result.stderr)

    def test_directory_symlink_is_reported_without_following_it(self):
        with tempfile.TemporaryDirectory() as target:
            Path(target, "key").write_bytes(MARKER)
            (self.root / "mounted-link").symlink_to(target, target_is_directory=True)
            result = self.run_scan()
        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertIn("lien symbolique", result.stderr)
        self.assertIn("mounted-link", result.stderr)
        self.assertNotIn("CLE_PRIVEE", result.stdout)

    def test_invalid_inputs(self):
        for limit in ["-1", "invalid", "", "9" * 19]:
            with self.subTest(limit=limit):
                result = self.run_scan(limit=limit)
                self.assertEqual(result.returncode, 2)
                self.assertIn("MAX_BYTES", result.stderr)
        result = self.run_scan("extra")
        self.assertEqual(result.returncode, 2)
        missing = subprocess.run(
            ["bash", str(SCRIPT), str(self.root / "missing")],
            env=self.env, capture_output=True, text=True, timeout=10,
        )
        self.assertEqual(missing.returncode, 2)
        self.assertIn("erreurs=1", missing.stdout)
        self.assertIn("INCOMPLETE", missing.stderr)

    def test_python_minimum_rejects_old_versions_and_prereleases(self):
        source = SCRIPT.read_text().split("<<'PYTHON'\n", 1)[1].rsplit("\nPYTHON", 1)[0]
        for version in [(3, 14, 9, "final"), (3, 15, 0, "alpha")]:
            with self.subTest(version=version):
                wrapper = (
                    "import sys\nfrom collections import namedtuple\n"
                    "Version = namedtuple('Version', 'major minor micro releaselevel')\n"
                    f"sys.version_info = Version{version!r}\n"
                    "sys.argv = ['-', '--help']\n"
                    f"exec(compile({source!r}, 'scan-cles-nas.sh', 'exec'))\n"
                )
                result = subprocess.run(
                    [sys.executable, "-c", wrapper], env=self.env,
                    capture_output=True, text=True, timeout=10,
                )
                self.assertEqual(result.returncode, 2, result.stderr)
                self.assertIn("Python 3.15.0 minimum", result.stderr)
                self.assertNotIn("Traceback", result.stderr)

    def command_with_open_hook(self, hook):
        source = SCRIPT.read_text().split("<<'PYTHON'\n", 1)[1].rsplit("\nPYTHON", 1)[0]
        wrapper = (
            "import os, errno, signal, sys, time\n"
            "real_open = os.open\n"
            "def hooked_open(path, *args, **kwargs):\n"
            + hook
            + "\n    return real_open(path, *args, **kwargs)\n"
            "os.open = hooked_open\n"
            f"sys.argv = ['-', {str(self.root)!r}]\n"
            f"exec(compile({source!r}, 'scan-cles-nas.sh', 'exec'))\n"
        )
        return [sys.executable, "-c", wrapper]

    def run_with_open_hook(self, hook):
        return subprocess.run(
            self.command_with_open_hook(hook), env=self.env,
            capture_output=True, text=True, timeout=10,
        )

    def test_read_errors_cannot_report_a_clean_search(self):
        (self.root / "blocked").write_bytes(MARKER)
        result = self.run_with_open_hook(
            "    if str(path).endswith('/blocked'):\n"
            "        raise PermissionError(errno.EACCES, 'synthetic permission failure')"
        )
        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertIn("erreurs=1", result.stdout)
        self.assertIn("synthetic permission failure", result.stderr)
        self.assertNotIn("Aucun marqueur trouve", result.stderr)

    def test_interrupt_returns_partial_report_and_status_130(self):
        (self.root / "interrupt").write_bytes(MARKER)
        result = self.run_with_open_hook(
            "    if str(path).endswith('/interrupt'):\n"
            "        os.kill(os.getpid(), signal.SIGINT)"
        )
        self.assertEqual(result.returncode, 130, result.stderr)
        self.assertIn("rapport partiel", result.stderr)
        self.assertIn("RESUME", result.stdout)
        self.assertNotIn("Traceback", result.stderr)

    def test_tty_progress_is_single_line_and_report_stays_clean(self):
        (self.root / "large").write_bytes(b"x" * (4 * 1024 * 1024))
        master, slave = pty.openpty()
        self.addCleanup(os.close, master)
        proc = subprocess.Popen(
            self.command_with_open_hook(
                "    if str(path).endswith('/large'):\n"
                "        time.sleep(0.7)"
            ),
            env={**self.env, "TERM": "xterm"},
            stdout=subprocess.PIPE, stderr=slave,
        )
        os.close(slave)
        output = bytearray()
        while True:
            ready, _, _ = select.select([master], [], [], 10)
            self.assertTrue(ready, "scanner stalled")
            try:
                chunk = os.read(master, 65536)
            except OSError as error:
                if error.errno != errno.EIO:
                    raise
                break
            if not chunk:
                break
            output.extend(chunk)
        stdout, _ = proc.communicate(timeout=10)
        self.assertEqual(proc.returncode, 0, output.decode())
        self.assertIn(b"\r\x1b[2K[", output)
        self.assertGreaterEqual(output.count(b"\r\x1b[2K["), 4, "redraw while a read is stalled")
        self.assertIn(b"100.0%", output)
        self.assertNotIn(b"\x1b", stdout)

    def test_search_preserves_files_and_permissions(self):
        path = self.root / "id_test"
        content = MARKER + b"\nsynthetic-data\n"
        path.write_bytes(content)
        path.chmod(0o600)
        before = path.stat()
        result = self.run_scan()
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertEqual(path.read_bytes(), content)
        after = path.stat()
        self.assertEqual((before.st_mode, before.st_mtime_ns), (after.st_mode, after.st_mtime_ns))
        self.assertEqual(list(self.root.iterdir()), [path])


if __name__ == "__main__":
    unittest.main()
