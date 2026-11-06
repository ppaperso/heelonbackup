# Configuration Reference

This document provides a complete reference for all HeelonBackup configuration options.

## Table of Contents

1. [Configuration File Location](#configuration-file-location)
2. [SMB Configuration](#smb-configuration)
3. [Backup Configuration](#backup-configuration)
4. [Storage Configuration](#storage-configuration)
5. [Complete Example](#complete-example)
6. [Environment Variables](#environment-variables)
7. [Configuration Commands](#configuration-commands)
8. [Sealed Sources](#sealed-sources)

## Configuration File Location

### Default Location

By default, HeelonBackup looks for its configuration file at:

```
~/.config/heelonbackup/config.json
```

### Custom Location

You can specify a custom configuration file path using:

- Command-line option: `--config PATH` or `-c PATH`
- Environment variable: Not currently supported

### Example

```bash
# Use a custom configuration file
heelonbackup --config /etc/heelonbackup/config.json backup
```

---

## SMB Configuration

The `smb` section configures the connection to your SMB server (Synology NAS).

### Options

| Option | Type | Required | Default | Description |
|--------|------|----------|---------|-------------|
| `url` | string | **Yes** | - | SMB share URL in the format `smb://host[:port]/share[/folder]` |
| `username` | string | **Yes** | - | Username for SMB authentication |
| `password` | string | No | null | Password for SMB authentication (optional, non-interactive runs only; interactively a masked prompt is used) |
| `workgroup` | string | No | null | Workgroup or domain name |
| `timeout` | integer | No | 30 | Connection timeout in seconds (0 = no timeout) |
| `encrypt` | boolean | No | true | Require SMB3 encryption for the connection |

### URL Format

The SMB URL should be in one of these formats:

```
smb://server/share
smb://server/share/folder
smb://server:port/share
```

**Examples:**
```json
"smb": {
  "url": "smb://192.168.1.100/backup"
}

"smb": {
  "url": "smb://nas.local/backup/heelon"
}

"smb": {
  "url": "smb://192.168.1.100:445/public"
}
```

### Authentication

**Recommended (interactive):** in a terminal, the password is asked with a masked prompt when the NAS connection starts (the interactive console asks it once per session and keeps it in memory only) — nothing is stored in the shell history, the environment or on disk.

**Non-interactive runs only** (scripts, cron, `sudo` without a tty): use the `HEELONBACKUP_SMB_PASSWORD` environment variable (set and not empty):

```bash
# Set password for a non-interactive run
export HEELONBACKUP_SMB_PASSWORD="your_password"
heelonbackup backup
```

**Alternative:** Store password in configuration file:
```json
"smb": {
  "url": "smb://192.168.1.100/backup",
  "username": "backup_user",
  "password": "your_password"
}
```

### Encryption

When `encrypt: true` (default), HeelonBackup will require SMB3 encryption. This ensures that all data transmitted between your computer and the NAS is encrypted.

```json
"smb": {
  "encrypt": true  // Recommended for security
}
```

If your NAS doesn't support SMB3 encryption, you can disable it, but this is not recommended:
```json
"smb": {
  "encrypt": false  // Not recommended - data may be unencrypted
}
```

### Timeout

The timeout is applied to each SMB operation. If an operation takes longer than the timeout, it will fail and be retried.

```json
"smb": {
  "timeout": 60  // 60 seconds timeout
}
```

### Complete SMB Example

```json
"smb": {
  "url": "smb://192.168.1.100/backup",
  "username": "backup_user",
  "workgroup": "WORKGROUP",
  "timeout": 60,
  "encrypt": true
}
```

---

## Backup Configuration

The `backup` section configures what to back up and how.

### Options

| Option | Type | Required | Default | Description |
|--------|------|----------|---------|-------------|
| `sources` | array | No | ["/home"] | Directories to back up. Paths should be absolute. |
| `excludes` | array | No | [] | Patterns to exclude (glob syntax). See [Exclusion Patterns](#exclusion-patterns). |
| `max_file_size` | integer | No | 0 | Maximum file size in bytes to back up. 0 = unlimited. |
| `workers` | integer | No | CPU count | Number of parallel workers for file transfers. |

### Sources

The `sources` array specifies which directories to back up. Paths must be absolute.

```json
"backup": {
  "sources": ["/home/user"]
}
```

Multiple sources:
```json
"backup": {
  "sources": [
    "/home/user/Documents",
    "/home/user/Pictures",
    "/home/user/Projects"
  ]
}
```

You can also specify sources on the command line:
```bash
heelonbackup backup /home/user/Documents /home/user/Pictures
```

### Exclusion Patterns

The `excludes` array uses glob patterns to specify which files and directories should be excluded from the backup.

#### Pattern Types

1. **Name patterns (without `/`)**: Matched against every file or directory name
   - `"*.tmp"` - Excludes all files ending with .tmp
   - `"node_modules"` - Excludes all directories named node_modules
   - `"*.cache*"` - Excludes anything with .cache in the name

2. **Path patterns (with `/`)**: Matched against the end of the path
   - `".local/share/Trash"` - Excludes .local/share/Trash directories
   - `"Downloads/temp"` - Excludes Downloads/temp directories

3. **Absolute path patterns (starting with `/`)**: Matched against the absolute path
   - `"/home/me/Videos/**"` - Excludes everything under /home/me/Videos

#### Examples

```json
"backup": {
  "excludes": [
    // Common cache and temporary directories
    "*.cache*",
    "*.thumbnails*",
    ".Trash*",
    ".local/share/Trash*",
    
    // Development files
    "node_modules",
    ".git",
    "target",
    ".vscode",
    ".idea",
    
    // Large or unnecessary files
    "*.log",
    "*.tmp",
    ".DS_Store"
  ]
}
```

You can also specify temporary exclusions on the command line:
```bash
heelonbackup backup --exclude "*.tmp" --exclude "temp/"
```

### Maximum File Size

Use `max_file_size` to exclude files larger than a certain size. The value is in bytes.

```json
"backup": {
  "max_file_size": 1073741824  // 1 GB in bytes
}
```

Common values:
- 1 MB: `1048576`
- 100 MB: `104857600`
- 1 GB: `1073741824`
- 10 GB: `10737418240`

Set to 0 (default) to back up files of any size.

### Workers

The `workers` option controls how many parallel smbclient sessions are used for file transfers. More workers can improve transfer speed, but may also increase resource usage.

```json
"backup": {
  "workers": 4
}
```

**Recommended values:**
- Default (CPU count): Good for most use cases
- 2-4: Conservative, lower resource usage
- 4-8: Balanced performance and resource usage
- 8+: Aggressive, maximum performance (use with caution)

### Complete Backup Example

```json
"backup": {
  "sources": ["/home/user/Documents", "/home/user/Pictures"],
  "excludes": [
    "*.cache*",
    "*.thumbnails*",
    ".Trash*",
    ".local/share/Trash*",
    "node_modules",
    ".git",
    "target"
  ],
  "max_file_size": 1073741824,
  "workers": 4
}
```

---

## Storage Configuration

The `storage` section configures how backups are stored on the NAS.

### Options

| Option | Type | Required | Default | Description |
|--------|------|----------|---------|-------------|
| `base_dir` | string | No | "heelonbackup" | Base directory on the NAS where backups are stored |
| `retention` | integer | No | 5 | Number of backups to keep before deleting the oldest ones |

### Base Directory

All backups are stored in subdirectories of the `base_dir` on the NAS. Each backup gets its own directory named with a timestamp (YYYYMMDD_HHMMSS).

```json
"storage": {
  "base_dir": "my_backups"
}
```

This would create backups in directories like:
```
smb://nas/backup/my_backups/20261007_143022/
smb://nas/backup/my_backups/20261006_180000/
```

### Retention

The `retention` option specifies how many backups to keep. When a new backup is created and the number of backups exceeds this value, the oldest backups are automatically deleted.

```json
"storage": {
  "retention": 10  // Keep the last 10 backups
}
```

Set to 0 to keep all backups (not recommended as it may fill up your NAS).

### Complete Storage Example

```json
"storage": {
  "base_dir": "heelonbackup",
  "retention": 5
}
```

---

## Complete Example

Here's a complete configuration file with all options:

```json
{
  "smb": {
    "url": "smb://192.168.1.100/backup",
    "username": "backup_user",
    "workgroup": "WORKGROUP",
    "timeout": 60,
    "encrypt": true
  },
  "backup": {
    "sources": ["/home/user/Documents", "/home/user/Pictures"],
    "excludes": [
      "*.cache*",
      "*.thumbnails*",
      ".Trash*",
      ".local/share/Trash*",
      "node_modules",
      ".git",
      "target",
      ".vscode",
      ".idea",
      "*.log",
      "*.tmp"
    ],
    "max_file_size": 0,
    "workers": 4
  },
  "storage": {
    "base_dir": "heelonbackup",
    "retention": 5
  }
}
```

---

## Sealed Sources

Optional: omit `sealed` or set it to `null` to preserve ordinary backups.
To encrypt sensitive sources such as SSH keys **before** sending them to the NAS,
add this top-level section:

```json
"sealed": {
  "staging_dir": "~/.local/share/heelonbackup/sealed",
  "secret": {
    "tool": "secret-tool",
    "attribute": "heelonbackup",
    "key": "seal"
  },
  "items": [
    { "path": "~/.ssh", "archive": "ssh.tar.gz.age" }
  ]
}
```

| Field | Requirement |
|-------|-------------|
| `staging_dir` | Absolute path or `~/path`; private directory (0700), created automatically |
| `secret` | Optional; defaults to the lookup above. Only `secret-tool` is supported |
| `items` | Non-empty list of existing files/directories |
| `items[].path` | Absolute path or `~/path`, no `..`, not a filesystem root |
| `items[].archive` | Unique filename ending in `.tar.gz.age` (case-insensitive uniqueness) |

Staging and sensitive sources must not overlap. Staging itself cannot be a symlink.
Remove `.ssh` from `backup.sources`; if a parent such as `/home/user` remains a
source, the sensitive subtree is automatically excluded. Protection also applies
to explicit CLI paths, regardless of exclusion globs. Other copies or hard links
outside the configured sensitive paths are **not** protected: seal those paths too.

Full backups automatically include the configured encrypted archives; adding
staging to `backup.sources` is optional. Only listed archives are transferred from
staging, never stray files, lock files or temporary archives. Partial backups
include archives only if their selected paths overlap staging or sensitive sources.
Selecting one sensitive file includes all configured archives.

Every real backup refreshes all sealed items before scanning or connecting to the NAS.
The tar → gzip → age pipeline streams in memory: no plaintext archive or passphrase
is written to disk. Encryption is authenticated (age passphrase mode, scrypt +
ChaCha20-Poly1305), with fresh randomness each run. Archive files are 0600 and
atomically replaced after all archives have been generated; a staging lock remains
held throughout backup transfer. Failure, missing sources, keyring timeout, unreadable
archives or an archive exceeding `backup.max_file_size` abort the backup before NAS
access. Configured exclusion globs cannot silently omit required archives.
Symlinks inside an archive are stored as links, never followed.

`heelonbackup seal` refreshes archives without NAS access. Both `seal --dry-run`
and `backup --dry-run` validate and show the plan without keyring access or writes.
Planned archive sizes and contents are not part of dry-run scan counts.
See the [user guide](user_guide.md#sealing-sensitive-sources) for setup and recovery.

The console's **Seal sensitive sources (`e`)** menu configures one source at a
time, preserving existing items and ordinary sources. The staging directory is
shared by every item; changing it changes the destination for all archives.
Configuration is validated and previewed before explicit save confirmation.
The same guided setup is also available under **Configuration**.
Passphrase storage is a separate, explicitly confirmed keyring operation with
two masked prompts (`seal --set-passphrase` in the CLI), never a JSON property.
An empty/mismatched passphrase is rejected and keyring output is never displayed.
Replacing the keyring passphrase does not change old archives.

## Environment Variables

HeelonBackup supports the following environment variables:

### `HEELONBACKUP_SMB_PASSWORD`

Provides the SMB password for **non-interactive runs** (scripts, cron) without storing it in the configuration file. In an interactive terminal the password is asked with a masked prompt instead, and this variable is ignored.

```bash
# Set for current session
export HEELONBACKUP_SMB_PASSWORD="your_password"

# Or add to your shell profile
# ~/.bashrc or ~/.zshrc
echo 'export HEELONBACKUP_SMB_PASSWORD="your_password"' >> ~/.bashrc
source ~/.bashrc
```

**Note:** This is the recommended way to provide your SMB password.

### `HEELONBACKUP_SEAL_PASSPHRASE`

Optional seal-passphrase override, including for unattended runs. If absent,
`secret-tool lookup heelonbackup seal` is used (15-second timeout). An explicitly
empty or non-UTF-8 override is rejected. The passphrase is never logged, serialized
or passed in process arguments; seal and SMB passwords are removed from the
keyring subprocess environment. Prefer an unlocked session keyring or a trusted
secret provider rather than storing the override in shell profiles or scripts.
Keep an independent offline copy of the passphrase: loss of the keyring must not
make the backups unrecoverable.

### `RUST_LOG`

Controls the verbosity of log output.

```bash
# Debug level (most verbose)
export RUST_LOG=heelonbackup=debug

# Info level (default)
export RUST_LOG=heelonbackup=info

# Warning level
export RUST_LOG=heelonbackup=warn

# Error level (least verbose)
export RUST_LOG=heelonbackup=error
```

---

## Configuration Commands

HeelonBackup provides commands to manage your configuration:

### Initialize Configuration

```bash
heelonbackup config init
```

Creates a default configuration file if it doesn't exist.

### Edit Configuration

```bash
heelonbackup config edit
```

Opens the configuration file in your default editor.

### Validate Configuration

```bash
heelonbackup config check
```

Checks if your configuration file is valid and reports any errors.

### Show Configuration

```bash
heelonbackup config show
```

Displays the current configuration (with sensitive information like passwords redacted).

---

## Tips

1. **Start Simple**: Begin with a basic configuration and add complexity as needed.

2. **Test with Dry Run**: Always use `--dry-run` to test new configurations before actually backing up.

   ```bash
   heelonbackup backup --dry-run
   ```

3. **Exclude What You Don't Need**: Use exclusion patterns to skip files you don't need to back up.

4. **Use Environment Variables**: For sensitive information like passwords, prefer environment variables over configuration files.

5. **Monitor Your Backups**: Regularly check backup status and verify backups to ensure everything is working correctly.

6. **Keep It Updated**: Update HeelonBackup to the latest version for the best features and security.

7. **Review Retention**: Periodically review your retention policy to ensure you're keeping the right number of backups.
