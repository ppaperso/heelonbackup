# CLI Reference

This document provides a complete reference for all HeelonBackup command-line options and arguments.

## Table of Contents

1. [Global Options](#global-options)
2. [backup](#backup)
3. [status](#status)
4. [list](#list)
5. [verify](#verify)
6. [restore](#restore)
7. [config](#config)
8. [completions](#completions)

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
heelonbackup backup [OPTIONS] [SOURCES]...
```

### Arguments

| Argument | Type | Description |
|----------|------|-------------|
| `SOURCES` | PATH | Directories or files to back up. If not provided, uses configured sources. |

### Options

| Option | Short | Type | Description | Default |
|--------|-------|------|-------------|---------|
| `--exclude` | `-e` | PATTERN | Exclude pattern (glob syntax). Can be specified multiple times. | N/A |
| `--dry-run` | `-n` | flag | Show what would be backed up without actually transferring. | false |
| `--workers` | `-w` | NUMBER | Number of parallel workers. Overrides config. | CPU count |
| `--max-file-size` | | SIZE | Maximum file size in bytes. Overrides config. | 0 (unlimited) |

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

Restore files from a backup (use `--dry-run` to only check what would happen).

### Usage

```bash
heelonbackup restore [OPTIONS] BACKUP DESTINATION
```

### Arguments

| Argument | Type | Required | Description |
|----------|------|----------|-------------|
| `BACKUP` | STRING | Yes | Backup name (YYYYMMDD_HHMMSS) |
| `DESTINATION` | PATH | Yes | Local directory to restore files to |

### Options

| Option | Short | Type | Description | Default |
|--------|-------|------|-------------|---------|
| `--execute` | `-e` | flag | Actually restore files (without this, it's a dry run) | false |
| `--files` | `-f` | PATH | Specific files to restore (absolute paths). Can be specified multiple times. | N/A |
| `--pattern` | `-p` | PATTERN | Restore files matching this pattern (glob syntax) | N/A |
| `--dry-run` | `-n` | flag | Show what would be restored without actually doing it | false |

### Examples

```bash
# Dry run to see what would be restored
heelonbackup restore 20261007_143022 /home/user/restored

# Actually restore files
heelonbackup restore 20261007_143022 /home/user/restored --execute

# Restore specific files
heelonbackup restore 20261007_143022 /home/user/restored \
  --files "/home/user/Documents/important.pdf" \
  --files "/home/user/Pictures/vacation.jpg"

# Restore files matching a pattern
heelonbackup restore 20261007_143022 /home/user/restored \
  --pattern "*.pdf"

# Restore with dry run
heelonbackup restore 20261007_143022 /home/user/restored \
  --dry-run
```

### Restore Behavior

- By default, restore is a dry run (no files are actually restored)
- Use `--execute` to actually restore files
- Files are restored to their original paths relative to the destination
- Original file permissions are preserved when possible
- Existing files are not overwritten by default (this may change in future versions)

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
| `init` | Create default configuration if it doesn't exist |
| `edit` | Edit the configuration file with default editor |
| `check` | Validate the configuration file |
| `show` | Show the current configuration |

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
