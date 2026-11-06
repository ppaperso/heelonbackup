#!/usr/bin/env bash
# Read-only private-key marker search. Never prints file contents.
# Python's standard library provides chunked reads and time-based progress.
set -eu
if ! command -v python3.15 >/dev/null 2>&1; then
  printf 'Erreur : python3.15 (Python 3.15.0 minimum) est requis.\n' >&2
  exit 2
fi
exec python3.15 - "$@" <<'PYTHON'
import json
import os
import re
import stat
import sys
import tempfile
import threading
import time


def quoted(path):
    return json.dumps(os.fsdecode(path), ensure_ascii=True)


class Progress:
    def __init__(self):
        self.tty = sys.stderr.isatty() and os.environ.get("TERM") != "dumb"
        self.last = 0
        self.started = time.monotonic()
        self.active = False
        self.state = None
        self.lock = threading.RLock()
        self.stop = threading.Event()
        self.worker = threading.Thread(target=self.refresh_loop, daemon=True)
        self.worker.start()

    def refresh_loop(self):
        while not self.stop.wait(0.15):
            self.refresh()

    def close(self):
        self.stop.set()
        self.worker.join()
        self.clear()

    def clear(self):
        with self.lock:
            if self.active:
                sys.stderr.write("\r\033[2K")
                sys.stderr.flush()
                self.active = False

    def message(self, text):
        with self.lock:
            self.state = None
            self.clear()
            print(text, file=sys.stderr, flush=True)

    def finding(self, kind, path):
        with self.lock:
            self.clear()
            print(f"{kind}\t{quoted(path)}", flush=True)

    def draw(self, text, force=False):
        now = time.monotonic()
        if not self.tty or (not force and now - self.last < 0.15):
            return
        self.last = now
        try:
            columns = os.get_terminal_size(sys.stderr.fileno()).columns or 80
        except OSError:
            columns = 80
        sys.stderr.write("\r\033[2K" + text[:max(0, columns - 1)])
        sys.stderr.flush()
        self.active = True

    def inventory(self, count):
        with self.lock:
            self.state = ("inventory", count)
            self.refresh()

    def scan(self, count, total, processed, total_bytes, hits, force=False):
        with self.lock:
            self.state = ("scan", count, total, processed, total_bytes, hits)
            self.refresh(force)

    def refresh(self, force=False):
        with self.lock:
            if self.state is None:
                return
            if self.state[0] == "inventory":
                spinner = "|/-\\"[int(time.monotonic() * 8) % 4]
                self.draw(f"{spinner} Inventaire : {self.state[1]} fichiers", force)
                return
            _, count, total, processed, total_bytes, hits = self.state
            ratio = processed / total_bytes if total_bytes else (count / total if total else 1)
            filled = min(20, int(ratio * 20))
            elapsed = time.monotonic() - self.started
            eta = f"{elapsed * (1 - ratio) / ratio:.0f}s" if ratio else "--"
            self.draw(
                f"[{'#' * filled}{'-' * (20 - filled)}] {ratio * 100:5.1f}% "
                f"{count}/{total} fichiers | cles:{hits} | {elapsed:.0f}s ETA:{eta}",
                force,
            )


def main():
    if sys.version_info < (3, 15, 0) or (
        sys.version_info[:3] == (3, 15, 0) and sys.version_info.releaselevel != "final"
    ):
        print("Erreur : Python 3.15.0 minimum (version finale) est requis.", file=sys.stderr)
        return 2
    if sys.argv[1:] == ["--help"]:
        print("Usage: bash scripts/scan-cles-nas.sh [MOUNT_POINT]\n"
              "Default: /mnt/beestation. Requires Python 3.15.0 minimum.\n"
              "MAX_BYTES=10485760 by default; 0 reads all file sizes.\n"
              "Read-only, same filesystem, no symlink traversal.\n"
              "Exit: 0=no markers, 1=markers, 2=incomplete/error, 130=interrupted.\n"
              "Archives are NOT decompressed; snapshots and other shares need separate scans.")
        return 0
    if len(sys.argv) > 2:
        print("Erreur : un seul point de montage est accepte (voir --help).", file=sys.stderr)
        return 2
    raw_limit = os.environ.get("MAX_BYTES", "10485760")
    if not re.fullmatch(r"[0-9]+", raw_limit) or len(raw_limit) > 18:
        print("Erreur : MAX_BYTES doit etre un entier positif ou nul (0 = sans limite).", file=sys.stderr)
        return 2
    limit = int(raw_limit)
    root = os.path.abspath(sys.argv[1] if len(sys.argv) == 2 else "/mnt/beestation")
    progress = Progress()
    errors = 0
    interrupted = False
    total = total_bytes = count = processed = hits = suspects = oversized = omitted = 0
    chunk_size = 256 * 1024
    # Overlap finds markers split between reads; arbitrary binary prefixes are allowed.
    marker = re.compile(
        rb"-----BEGIN (?:[A-Z0-9]+ ){0,8}PRIVATE KEY-----|PuTTY-User-Key-File-[0-9]+:"
    )

    def report_error(path, error):
        nonlocal errors
        errors += 1
        progress.message(f"Erreur sur {quoted(path)} : {error.strerror or str(error)}")

    def suspect(path, is_dir=False):
        nonlocal suspects
        name = os.path.basename(path)
        lower = name.lower()
        if (is_dir and name == ".ssh") or (not is_dir and (
            (name.startswith("id_") and not lower.endswith(".pub"))
            or lower.endswith((".pem", ".key", ".ppk"))
        )):
            suspects += 1
            progress.finding("NOM_SUSPECT", path)

    try:
        if os.path.islink(root):
            raise OSError("le point de montage ne doit pas etre un lien symbolique")
        root_stat = os.stat(root)
        if not stat.S_ISDIR(root_stat.st_mode):
            raise OSError("le point de montage doit etre un dossier")
        progress.message(f"Recherche en lecture seule dans {quoted(root)}.")
        progress.message("Inventaire (total encore inconnu)...")
        progress.inventory(0)
        suspect(root, True)
        # Only metadata goes to this private, automatically removed temporary file.
        with tempfile.TemporaryFile(mode="w+t", encoding="utf-8") as inventory:
            def walk_error(error):
                report_error(error.filename or root, error)

            for directory, dirs, files in os.walk(root, followlinks=False, onerror=walk_error):
                for name in dirs[:]:
                    path = os.path.join(directory, name)
                    try:
                        metadata = os.stat(path, follow_symlinks=False)
                        if stat.S_ISLNK(metadata.st_mode) or metadata.st_dev != root_stat.st_dev:
                            dirs.remove(name)
                            omitted += 1
                            reason = "lien symbolique" if stat.S_ISLNK(metadata.st_mode) else "autre montage"
                            progress.message(f"Ignore ({reason}) : {quoted(path)} ; analyser sa cible separement.")
                        else:
                            suspect(path, True)
                    except OSError as error:
                        dirs.remove(name)
                        report_error(path, error)
                for name in files:
                    path = os.path.join(directory, name)
                    try:
                        metadata = os.stat(path, follow_symlinks=False)
                        if not stat.S_ISREG(metadata.st_mode) or metadata.st_dev != root_stat.st_dev:
                            omitted += 1
                            reason = "autre montage" if metadata.st_dev != root_stat.st_dev else "lien ou fichier special"
                            progress.message(f"Ignore ({reason}) : {quoted(path)}")
                            continue
                        suspect(path)
                        if limit and metadata.st_size > limit:
                            oversized += 1
                            continue
                        inventory.write(json.dumps([
                            path, metadata.st_size, metadata.st_dev,
                            metadata.st_ino, metadata.st_mtime_ns,
                        ]) + "\n")
                        total += 1
                        total_bytes += metadata.st_size
                        progress.inventory(total)
                    except OSError as error:
                        report_error(path, error)

            progress.message(
                f"Inventaire : {total} fichiers a lire ({total_bytes} octets), "
                f"{oversized} au-dessus de MAX_BYTES, {omitted} liens/fichiers speciaux/autres montages ignores."
            )
            progress.started = time.monotonic()
            progress.scan(0, total, 0, total_bytes, 0, True)
            inventory.seek(0)
            for line in inventory:
                path, size, device, inode, mtime = json.loads(line)
                read_bytes = 0
                found = False
                try:
                    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
                    with os.fdopen(fd, "rb") as stream:
                        metadata = os.fstat(stream.fileno())
                        if not stat.S_ISREG(metadata.st_mode) or (
                            metadata.st_dev, metadata.st_ino, metadata.st_size, metadata.st_mtime_ns
                        ) != (device, inode, size, mtime):
                            raise OSError("fichier modifie depuis l'inventaire")
                        tail = b""
                        while read_bytes < size:
                            chunk = stream.read(min(chunk_size, size - read_bytes))
                            if not chunk:
                                raise OSError("fichier tronque pendant la lecture")
                            read_bytes += len(chunk)
                            processed += len(chunk)
                            found = found or marker.search(tail + chunk) is not None
                            tail = chunk[-256:]
                            progress.scan(count, total, processed, total_bytes, hits)
                        current = os.fstat(stream.fileno())
                        if (current.st_size, current.st_mtime_ns) != (size, mtime):
                            raise OSError("fichier modifie pendant la lecture")
                except OSError as error:
                    report_error(path, error)
                finally:
                    if found:
                        progress.finding("CLE_PRIVEE", path)
                        hits += 1
                count += 1
                progress.scan(count, total, processed, total_bytes, hits)
            progress.scan(count, total, processed, total_bytes, hits, True)
    except KeyboardInterrupt:
        interrupted = True
    except OSError as error:
        report_error(root, error)
    finally:
        progress.close()

    print(f"RESUME\tanalyses={count}/{total}\tmarqueurs={hits}\tnoms_suspects={suspects}"
          f"\ttrop_gros={oversized}\terreurs={errors}", flush=True)
    progress.message(
        "Limites : marqueurs generiques, pas d'identification du proprietaire ; "
        "archives non decompressees, fichiers chiffres non dechiffres. "
        "Verifier separement les autres partages, corbeilles et snapshots."
    )
    if interrupted:
        progress.message("Recherche INCOMPLETE : interrompue, rapport partiel.")
        return 130
    if total == 0:
        progress.message("Recherche INCOMPLETE : aucun fichier analysable. "
                         "Verifier que le partage SMB est monte et selectionner son dossier reel, "
                         "pas le dossier parent de montages ou de liens.")
        return 2
    if errors or oversized:
        progress.message("Recherche INCOMPLETE : erreurs ou limite de taille. "
                         "Corriger les erreurs ; MAX_BYTES=0 retire la limite.")
        return 2
    if hits:
        progress.message(f"ATTENTION : {hits} fichier(s) contenant des marqueurs de cles privees.")
        return 1
    progress.message("Aucun marqueur trouve dans les fichiers examines ; ce n'est pas une garantie d'absence.")
    return 0


try:
    sys.exit(main())
except BrokenPipeError:
    sys.exit(141)
PYTHON
