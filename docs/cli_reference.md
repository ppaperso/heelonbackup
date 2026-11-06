# CLI Reference

This document provides a complete reference for all HeelonBackup command-line options and arguments.

## Table of Contents

1. [Global Options](#global-options)
2. [backup](#backup)
3. [status](#status)
4. [list](#list)
5. [ping](#ping)
6. [verify](#verify)
7. [restore](#restore)
8. [config](#config)
9. [completions](#completions)
10. [Interactive Mode (no subcommand)](#interactive-mode-no-subcommand)

## Global Options

These options are available for all commands:

| Option | Short | Type | Description | Default |
|--------|-------|------|-------------|---------|
| `--verbose` | `-v` | flag | Show debug messages | false |
| `--config` | `-c` | PATH | Configuration file path | `~/.config/heelonbackup/config.json` |
| `--help` | `-h` | flag | Show help message | N/A |
| `--version` | `-V` | flag | Show version | N/A |

### Examples

```bash
# Use a custom configuration file
heelonbackup --config /path/to/config.json backup

# Enable verbose logging
heelonbackup -v status

# Show version
heelonbackup --version
```

---

## backup

Backup the configured sources, or the given files/folders, to the NAS.

### Usage

```bash
heelonbackup backup [OPTIONS] [PATHS]...
```

### Arguments

| Argument | Type | Description |
|----------|------|-------------|
| `PATHS` | PATH | Files or folders to back up instead of the configured sources (repeatable). If omitted, uses `backup.sources` from the configuration. |

### Options

| Option | Short | Type | Description | Default |
|--------|-------|------|-------------|---------|
| `--dry-run` | `-n` | flag | Scan and check the connection, but transfer nothing | false |
| `--list` | `-l` | flag | With `--dry-run`: print every file that would be backed up | false |
| `--exclude` | `-e` | PATTERN | Extra exclude pattern (glob), **appended** to `backup.excludes` (repeatable) | N/A |
| `--workers` | `-w` | NUMBER | Parallel SMB sessions for this backup; overrides `backup.workers` (1-16) | CPU count (clamped 1-4) |
| `--max-file-size` | | SIZE | Maximum file size in bytes for this backup; overrides `backup.max_file_size` | 0 (unlimited) |

### Examples

```bash
# Backup configured sources
heelonbackup backup

# Backup specific directories
heelonbackup backup /home/user/Documents /home/user/Pictures

# Backup with temporary exclusions
heelonbackup backup --exclude "*.tmp" --exclude "*.log"

# Dry run to see what would be backed up
heelonbackup backup --dry-run

# Backup with 8 parallel workers
heelonbackup backup --workers 8

# Backup with maximum file size of 1GB
heelonbackup backup --max-file-size 1073741824
```

---

## status

Show the result of the last backups made from this computer.

### Usage

```bash
heelonbackup status [OPTIONS]
```

### Options

| Option | Short | Type | Description | Default |
|--------|-------|------|-------------|---------|
| `--all` | `-a` | flag | Show every recorded backup | false |
| `--detailed` | `-d` | flag | Show failed and skipped files | false |

### Examples

```bash
# Show status of last backup
heelonbackup status

# Show status of all backups
heelonbackup status --all

# Show detailed status with failed/skipped files
heelonbackup status --detailed

# Show all backups with details
heelonbackup status --all --detailed
```

### Output Format

The status command outputs information in the following format:

```
STATUS_ICON BackupName
   Started:   YYYY-MM-DD HH:MM:SS
   Finished:  YYYY-MM-DD HH:MM:SS (Duration)
   Files:     TotalFiles (NewFiles updated, FailedFiles failed)
   Size:      TotalSize
   Rate:      TransferRate
   Status:    status_label
   Sources:   source1, source2, ...
```

**Status Icons:**
- ✅ - Completed
- ⚠️ - Completed with errors
- ❌ - Failed
- ⏳ - In progress (or interrupted)

---

## list

List the backups stored on the NAS.

### Usage

```bash
heelonbackup list
```

### Options

This command has no specific options beyond the global ones.

### Examples

```bash
# List all backups on the NAS
heelonbackup list

# With verbose output
heelonbackup -v list
```

### Output Format

The list command outputs information in the following format:

```
BackupName  STATUS_ICON StatusLabel  Size  Files  Timestamp
```

Example:
```
heelonbackup/20261007_143022  ✅ completed     45.67 GB  12487 files  2026-10-07 14:30:22
heelonbackup/20261006_180000  ✅ completed     45.23 GB  12345 files  2026-10-06 18:00:00
heelonbackup/20261005_101530  ⚠️  completed w/errors  45.10 GB  12200 files  2026-10-05 10:15:30
```

---

## ping

Check that the NAS answers on its SMB port. It only opens a TCP connection (port 445, or the port given in `smb.url`) with a 3-second timeout: no SMB session, **no password needed**. The interactive console uses the same check for its NAS panel.

### Usage

```bash
heelonbackup ping
```

### Options

This command has no specific options beyond the global ones.

### Output and exit codes

```
🟢 NAS online: 192.168.1.100:445 (3 ms)                      # exit code 0
🔴 NAS offline: 192.168.1.100:445 (no answer within 3s)      # exit code 1
```

An invalid configuration exits with code 1 and the configuration error.

---

## verify

Check that a backup is complete on the NAS.

### Usage

```bash
heelonbackup verify [OPTIONS] [BACKUP]
```

### Arguments

| Argument | Type | Description |
|----------|------|-------------|
| `BACKUP` | STRING | Backup name (YYYYMMDD_HHMMSS). If not provided, verifies the latest backup. |

### Options

| Option | Short | Type | Description | Default |
|--------|-------|------|-------------|---------|
| `--deep` | | flag | Download every file and check its SHA-256 checksum (slow) | false |

### Examples

```bash
# Verify the latest backup
heelonbackup verify

# Verify a specific backup
heelonbackup verify 20261007_143022

# Deep verification (thorough but slow)
heelonbackup verify --deep

# Deep verification of a specific backup
heelonbackup verify 20261007_143022 --deep
```

### What Verification Does

**Standard verification:**
- Checks that the manifest file exists and is readable
- Verifies that all files listed in the manifest exist on the NAS
- Checks file sizes match the manifest

**Deep verification:**
- Downloads every file from the NAS
- Computes SHA-256 checksum of each downloaded file
- Compares checksums with those in the manifest
- Optionally deletes downloaded files after verification

---

## restore

Restore files from a backup stored on the NAS.

### Usage

```bash
heelonbackup restore [OPTIONS] [BACKUP]
```

### Arguments

| Argument | Type | Required | Description |
|----------|------|----------|-------------|
| `BACKUP` | STRING | No | Backup name (YYYYMMDD_HHMMSS). Default: the latest usable backup |

### Options

| Option | Short | Type | Description | Default |
|--------|-------|------|-------------|---------|
| `--target` | `-t` | DIR | Restore under this folder instead of the original locations | original locations |
| `--path` | `-p` | PATH | Only restore this file/folder (original absolute path; repeatable) | everything in the backup |
| `--dry-run` | `-n` | flag | Check everything (NAS content, conflicts, free space, permissions) and print the plan without writing anything | false |
| `--overwrite` | | flag | Replace existing files that differ from the backup (otherwise they are kept as conflicts) | false |
| `--list` | `-l` | flag | Print the action planned for every file | false |

### Semantics

- There is **no `--execute` flag**: without `--dry-run` the restore is **executed** (files are written). `--dry-run` only prints the plan and a verdict (exit 0 = would succeed, 2 = partial, 1 = would fail) and writes nothing.
- Without `--target`, files are restored to their **original absolute locations** — use `--target` for tests.
- The interactive console (`heelonbackup` without subcommand) always runs a dry run first and requires an explicit confirmation before any write.

### Examples

```bash
# Dry run: plan of restoring backup 20261007_143022 under /home/user/restored
heelonbackup restore 20261007_143022 --target /home/user/restored --dry-run

# Real restore under a target folder
heelonbackup restore 20261007_143022 --target /home/user/restored

# Only some original paths (repeatable), overwriting differing files, listing every action
heelonbackup restore 20261007_143022 --target /home/user/restored \
  --path /home/user/Documents --path /home/user/Pictures \
  --overwrite --list

# Dry run of the latest usable backup, restored to the original locations
heelonbackup restore --dry-run
```

### Restore Behavior

- SHA-256 checksums of downloaded files are verified before they are moved into place
- Original permissions and modification times are restored (setuid/setgid bits are never restored)
- Each file is restored atomically (temporary `.part` file, then rename)
- Existing differing files are kept unless `--overwrite` is given

---

## config

Manage the configuration (without action: create it if missing, otherwise show it).

### Usage

```bash
heelonbackup config [OPTIONS] [ACTION]
```

### Actions

| Action | Description |
|--------|-------------|
| `init` | Create default configuration if it doesn't exist (`--force` to replace an existing one) |
| `edit` | Edit the configuration file with default editor |
| `path` | Print the path of the configuration file |
| `check` | Validate the configuration file and resolve the configured sources (they must exist on disk); prints a summary and a final verdict. Exit 0 (`✅`) when valid, 1 with the error otherwise. No network access — use `list` to test the NAS connection |
| `show` | Show the current configuration |
| `sources` | List the configured sources — `✓` found on disk (with the resolved path), `✗` missing (skipped by the backup) — then the exclude patterns. Exit 0 when every source exists, 2 otherwise. No network access |

### Options

This command accepts the global options and has no specific options.

### Examples

```bash
# Create default configuration
heelonbackup config init

# Edit configuration
heelonbackup config edit

# Validate configuration
heelonbackup config check

# Show current configuration
heelonbackup config show

# List the sources and exclude patterns
heelonbackup config sources

# Create configuration at a specific path
heelonbackup --config /path/to/config.json config init
```

---

## completions

Generate shell completions for HeelonBackup.

### Usage

```bash
heelonbackup completions SHELL
```

### Arguments

| Argument | Type | Required | Description |
|----------|------|----------|-------------|
| `SHELL` | STRING | Yes | Shell type: bash, zsh, fish, powershell, elvish |

### Examples

```bash
# Generate bash completions
heelonbackup completions bash > ~/.local/share/bash-completion/completions/heelonbackup

# Generate zsh completions
heelonbackup completions zsh > ~/.local/share/zsh/site-functions/_heelonbackup

# Generate fish completions
heelonbackup completions fish > ~/.config/fish/completions/heelonbackup.fish

# Generate PowerShell completions
heelonbackup completions powershell > heelonbackup.ps1
```

### Installing Completions

**Bash:**
```bash
heelonbackup completions bash > ~/.local/share/bash-completion/completions/heelonbackup
source ~/.local/share/bash-completion/completions/heelonbackup
```

**Zsh:**
```bash
heelonbackup completions zsh > ~/.local/share/zsh/site-functions/_heelonbackup
compinit
```

**Fish:**
```bash
heelonbackup completions fish > ~/.config/fish/completions/heelonbackup.fish
```

---

## Exit Codes

HeelonBackup uses the following exit codes:

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | General error |
| 2 | Configuration error |
| 130 | Interrupted by user (Ctrl+C) |
| 141 | Broken pipe (e.g., `head` command) |

---

## Environment Variables

| Variable | Description | Example |
|----------|-------------|---------|
| `HEELONBACKUP_SMB_PASSWORD` | SMB password (overrides config file) | `export HEELONBACKUP_SMB_PASSWORD="secret"` |
| `RUST_LOG` | Log level | `export RUST_LOG=heelonbackup=debug` |

---

## Configuration File

For configuration file options, see [Configuration Reference](configuration.md).

## Interactive Mode (no subcommand)

Running `heelonbackup` with **no subcommand** opens the interactive console instead of printing this help: a full-screen dashboard (bilingual EN/FR) with the version, environment, live date/time, NAS state (same check as `ping`), last backup, and a terminal pane that streams the output and progress of the running operation. It offers guided flows for backup, status, backups on the NAS (`list`), verify, restore and configuration, using the same engine as the commands documented above. It needs a terminal (exit code 1 otherwise); quitting returns exit code 0, and `Ctrl-C` during an operation interrupts it without closing the dashboard. All subcommands remain unchanged and fully supported; see the user guides for the « Interactive Mode » section.
