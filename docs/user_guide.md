# HeelonBackup User Guide

> **Note**: This is the English version of the user guide. For the French version, see [user_guide_fr.md](user_guide_fr.md).

## Table of Contents

1. [Introduction](#-introduction)
2. [Installation](#-installation)
3. [Configuration](#-configuration)
4. [Basic Usage](#-basic-usage)
5. [Advanced Features](#-advanced-features)
6. [Troubleshooting](#-troubleshooting)
7. [Security Considerations](#-security-considerations)

## 📖 Introduction

HeelonBackup is a command-line backup tool designed for Linux users who want to reliably back up their data to a Synology NAS (or any SMB-compatible storage) via the SMB protocol.

### Key Features

- **Reliable**: SHA-256 checksums ensure data integrity
- **Fast**: Parallel transfers using multiple smbclient sessions
- **Flexible**: Customizable source directories and exclusion patterns
- **Transparent**: Clear status reporting and progress tracking
- **Restore-capable**: Can restore individual files or entire backups

### Use Cases

- Personal laptop backup to Synology NAS
- Home server backup
- Important documents backup
- Media files backup (photos, videos, music)
- Configuration files backup

## 🚀 Installation

### Prerequisites

Before installing HeelonBackup, ensure you have the following:

1. **Linux System**: Fedora 44+ or any recent distribution with glibc 2.34+
2. **Rust Compiler**: Latest stable version (Rust 2024 edition)
3. **SMB Client**: `smbclient` must be installed

### Install Prerequisites

#### On Fedora/RHEL/CentOS:
```bash
sudo dnf install samba-client rust cargo git
```

#### On Debian/Ubuntu:
```bash
sudo apt-get install smbclient rust cargo git
```

#### On Arch Linux:
```bash
sudo pacman -S smbclient rust git
```

### Install HeelonBackup

#### Method 1: From Source (Recommended)

```bash
# Clone the repository
git clone https://github.com/heelon/heelonbackup.git
cd heelonbackup

# Build in release mode (optimized for production)
cargo build --release

# Install the binary globally
sudo cp target/release/heelonbackup /usr/local/bin/

# Verify installation
heelonbackup --version
```

#### Method 2: Using Cargo

```bash
cargo install --git https://github.com/heelon/heelonbackup.git
```

#### Method 3: Manual Download

1. Download the latest release binary from GitHub
2. Make it executable: `chmod +x heelonbackup`
3. Place it in your PATH (e.g., `/usr/local/bin/`) 

### Verify Installation

```bash
heelonbackup --help
```

This should display the help message with all available commands.

## ⚙️ Configuration

### Quick Configuration

The easiest way to get started is to let HeelonBackup create a default configuration:

```bash
heelonbackup config init
```

This creates a configuration file at `~/.config/heelonbackup/config.json`.

### Configuration File Location

By default, HeelonBackup looks for its configuration file at:

```
~/.config/heelonbackup/config.json
```

You can specify a different configuration file using the `--config` or `-c` option:

```bash
heelonbackup --config /path/to/custom/config.json backup
```

### Configuration Options

Here's a detailed explanation of all configuration options:

#### SMB Configuration (`smb`)

| Option | Type | Required | Default | Description |
|--------|------|----------|---------|-------------|
| `url` | string | Yes | - | SMB share URL (e.g., `smb://192.168.1.100/backup`) |
| `username` | string | Yes | - | Username for SMB authentication |
| `password` | string | No | null | Password (optional, prefer environment variable) |
| `workgroup` | string | No | null | Workgroup or domain name |
| `timeout` | integer | No | 30 | Connection timeout in seconds |
| `encrypt` | boolean | No | true | Require SMB3 encryption |

**Example:**
```json
"smb": {
  "url": "smb://192.168.1.100/backup",
  "username": "backup_user",
  "workgroup": "WORKGROUP",
  "timeout": 60,
  "encrypt": true
}
```

#### Backup Configuration (`backup`)

| Option | Type | Required | Default | Description |
|--------|------|----------|---------|-------------|
| `sources` | array | No | ["/home"] | Directories to back up |
| `excludes` | array | No | [] | Patterns to exclude (glob syntax) |
| `max_file_size` | integer | No | 0 | Maximum file size in bytes (0 = unlimited) |
| `workers` | integer | No | CPU count | Number of parallel workers |

**Example:**
```json
"backup": {
  "sources": ["/home/user/Documents", "/home/user/Pictures"],
  "excludes": ["*.cache*", "*.thumbnails*", ".Trash*"],
  "max_file_size": 1073741824,
  "workers": 4
}
```

#### Storage Configuration (`storage`)

| Option | Type | Required | Default | Description |
|--------|------|----------|---------|-------------|
| `base_dir` | string | No | "heelonbackup" | Base directory on the NAS for backups |
| `retention` | integer | No | 5 | Number of backups to keep |

**Example:**
```json
"storage": {
  "base_dir": "my_backups",
  "retention": 10
}
```

### Environment Variables

HeelonBackup supports the following environment variables:

#### `HEELONBACKUP_SMB_PASSWORD`

The SMB password can be provided via this environment variable instead of storing it in the configuration file:

```bash
# Set password for the current session
export HEELONBACKUP_SMB_PASSWORD="your_secure_password"

# Run backup with password from environment
heelonbackup backup
```

This is the **recommended** way to provide the password, as it's more secure than storing it in a configuration file.

#### `RUST_LOG`

Controls the verbosity of log output:

```bash
# Debug level logging
export RUST_LOG=heelonbackup=debug

# Info level (default)
export RUST_LOG=heelonbackup=info

# Warning level only
export RUST_LOG=heelonbackup=warn
```

### Complete Configuration Example

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
    "sources": ["/home/user/Documents", "/home/user/Pictures", "/home/user/Projects"],
    "excludes": [
      "*.cache*",
      "*.thumbnails*",
      ".Trash*",
      ".local/share/Trash*",
      "node_modules",
      ".git",
      "target",
      ".vscode",
      ".idea"
    ],
    "max_file_size": 1073741824,
    "workers": 4
  },
  "storage": {
    "base_dir": "heelonbackup",
    "retention": 5
  }
}
```

### Configuration Validation

To check if your configuration is valid:

```bash
heelonbackup config check
```

This will validate the configuration file and report any errors.

## 🎯 Basic Usage

### Making Your First Backup

Once you have configured HeelonBackup, making a backup is simple:

```bash
heelonbackup backup
```

This will:
1. Scan all files in your configured source directories
2. Exclude files matching your exclusion patterns
3. Transfer files to your NAS via SMB
4. Verify the integrity of transferred files
5. Create a manifest with all backup metadata

### Monitoring Progress

During the backup, you'll see:

- A progress bar showing transfer progress
- File count and size information
- Estimated time remaining (ETA)
- Transfer speed

Example output:
```
Scanning /home/user... ✓
Found 12,487 files to back up (45.67 GB)
Transferring files... [==========>          ] 1248/12487 4.56 GB/s ETA 2m 30s
```

### Checking Backup Status

To see the status of your last backup:

```bash
heelonbackup status
```

This displays:
- Backup name (timestamp)
- Start and finish times
- Number of files backed up
- Total size
- Transfer rate
- Status (completed, completed with errors, failed)

For more details:

```bash
heelonbackup status --detailed
```

### Viewing All Backups

To see a list of all backups on your NAS:

```bash
heelonbackup list
```

This shows all backups with their status, size, and timestamp.

### Verifying a Backup

To ensure your backup is complete and intact:

```bash
heelonbackup verify
```

For a thorough verification that downloads and checks every file:

```bash
heelonbackup verify --deep
```

**Note**: Deep verification can take a long time and uses significant bandwidth.

### Restoring Files

To restore files from a backup:

```bash
# Dry run (shows what would be restored without actually doing it)
heelonbackup restore 20261007_143022 /home/user/restored

# Actual restore
heelonbackup restore 20261007_143022 /home/user/restored --execute

# Restore specific files only
heelonbackup restore 20261007_143022 /home/user/restored --files "/home/user/Documents/important.pdf"

# Restore files matching a pattern
heelonbackup restore 20261007_143022 /home/user/restored --pattern "*.pdf"
```

Replace `20261007_143022` with your backup name (timestamp).

## 🚀 Advanced Features

### Backup Specific Paths

You can back up specific directories or files that are not in your configuration:

```bash
heelonbackup backup /home/user/Documents /home/user/Pictures/Vacation
```

### Temporary Exclusions

Add temporary exclusions for a specific backup:

```bash
heelonbackup backup --exclude "*.tmp" --exclude "*.log" --exclude "temp/"
```

### Dry Run Mode

See what would be backed up without actually transferring any files:

```bash
heelonbackup backup --dry-run
```

This is useful for:
- Testing your configuration
- Checking what files will be included
- Estimating backup size and time

### Shell Completions

Generate shell completions for easier command-line usage:

#### Bash:
```bash
heelonbackup completions bash > ~/.local/share/bash-completion/completions/heelonbackup
```

#### Zsh:
```bash
heelonbackup completions zsh > ~/.local/share/zsh/site-functions/_heelonbackup
```

#### Fish:
```bash
heelonbackup completions fish > ~/.config/fish/completions/heelonbackup.fish
```

After adding the completions, restart your shell or run:

```bash
# For bash
source ~/.local/share/bash-completion/completions/heelonbackup

# For zsh
compinit
```

### Configuration Management

Manage your configuration interactively:

```bash
# Create default configuration
heelonbackup config init

# Edit configuration file
heelonbackup config edit

# Validate configuration
heelonbackup config check

# Show current configuration
heelonbackup config show
```

## 🔍 Troubleshooting

### Common Issues and Solutions

#### 1. smbclient not installed

**Error:**
```
error: smbclient is not installed
```

**Solution:**
```bash
# Fedora/RHEL
sudo dnf install samba-client

# Debian/Ubuntu
sudo apt-get install smbclient

# Arch Linux
sudo pacman -S smbclient
```

#### 2. Cannot reach the NAS

**Error:**
```
error: cannot reach the NAS: connection failed
```

**Solution:**
- Verify the NAS is powered on and connected to the network
- Check that the SMB service is running on the NAS
- Verify the IP address or hostname in your configuration
- Test connection manually:

```bash
smbclient //nas-ip/share -U your_username
```

#### 3. Authentication Failed

**Error:**
```
error: SMB authentication failed: session setup failed
```

**Solution:**
- Verify your username and password
- Check if the user has access to the share
- Try using the environment variable:

```bash
export HEELONBACKUP_SMB_PASSWORD="your_password"
heelonbackup backup
```

- Ensure the workgroup/domain is correct in your configuration

#### 4. Access Denied

**Error:**
```
error: SMB operation failed: NT_STATUS_ACCESS_DENIED
```

**Solution:**
- Check that the user has write permissions on the SMB share
- Verify the share permissions on your Synology NAS
- Ensure the user has the correct access rights

#### 5. Configuration File Not Found

**Error:**
```
error: configuration file not found at ~/.config/heelonbackup/config.json
```

**Solution:**
```bash
# Initialize configuration
heelonbackup config init

# Or specify a custom configuration file
heelonbackup --config /path/to/config.json backup
```

#### 6. File Already Exists

**Error:**
```
error: SMB operation failed: NT_STATUS_OBJECT_NAME_COLLISION
```

**Solution:**
- This is usually harmless and means the file already exists on the NAS
- HeelonBackup should handle this automatically
- If it persists, check for duplicate files in your backup sources

#### 7. Insufficient Space on NAS

**Error:**
```
error: SMB operation failed: NT_STATUS_DISK_FULL
```

**Solution:**
- Free up space on your NAS
- Check if the retention policy is working correctly
- Manually delete old backups if needed

### Debug Mode

For detailed troubleshooting, run HeelonBackup with debug logging:

```bash
export RUST_LOG=heelonbackup=debug
heelonbackup backup
```

This will output detailed information about:
- Configuration loading
- File scanning
- SMB connection
- Transfer operations
- Error details

### Checking SMB Connection

To manually test your SMB connection:

```bash
# Basic connection test
smbclient //nas-ip/share -U your_username

# With password from environment
PASSWD=your_password smbclient //nas-ip/share -U your_username

# List files in the share
smbclient //nas-ip/share -U your_username -c "ls"
```

## 🛡️ Security Considerations

### Password Security

**Best Practice**: Always use the environment variable for your SMB password instead of storing it in the configuration file:

```bash
export HEELONBACKUP_SMB_PASSWORD="your_password"
```

This ensures the password is not stored on disk and is only available for the current session.

### Encryption

HeelonBackup supports SMB3 encryption. Enable it in your configuration:

```json
{
  "smb": {
    "encrypt": true
  }
}
```

This encrypts all data transmitted between your computer and the NAS.

### Network Security

- Use a secure network for backups
- Consider using a VPN if backing up over the internet
- Keep your NAS firmware updated
- Use strong passwords for SMB access

### Data Integrity

HeelonBackup uses SHA-256 checksums to ensure data integrity:

- Each file is hashed before transfer
- Checksums are stored in the backup manifest
- Files can be verified after transfer
- Deep verification downloads and re-hashes files

### Permissions

- Files are backed up with their original Unix permissions
- The backup manifest stores permission information
- Restored files retain their original permissions

## 📈 Performance Tips

### Optimizing Backup Speed

1. **Increase Workers**: More parallel workers can improve transfer speed:
   ```json
   "backup": {
     "workers": 8
   }
   ```

2. **Use Wired Connection**: Wi-Fi can be slower and less reliable than Ethernet

3. **Schedule During Off-Peak**: Run backups when network usage is low

4. **Exclude Large Temporary Files**: Use `max_file_size` to skip very large files that change often

### Reducing Backup Size

1. **Add Exclusion Patterns**: Exclude directories you don't need:
   ```json
   "backup": {
     "excludes": ["*.cache*", "*.thumbnails*", "node_modules", ".git"]
   }
   ```

2. **Use max_file_size**: Skip files larger than a certain size:
   ```json
   "backup": {
     "max_file_size": 1073741824  // 1 GB
   }
   ```

### Monitoring Performance

Use the status command to see performance metrics:

```bash
heelonbackup status
```

Look for the "Rate" field, which shows the transfer speed in MB/s.

## 📊 Command Reference

For complete command documentation, see [CLI Reference](cli_reference.md).

## 🤝 Getting Help

If you encounter issues or have questions:

1. **Check the Troubleshooting Section**: Most common issues are covered above
2. **Review Logs**: Run with `RUST_LOG=heelonbackup=debug` for detailed output
3. **Consult Documentation**: See other files in the `docs/` directory
4. **Open an Issue**: If it's a bug or feature request, open an issue on GitHub

## 📚 Additional Resources

- [CLI Reference](cli_reference.md) - Complete command-line reference
- [Configuration Reference](configuration.md) - Detailed configuration options
- [Contributing Guide](../CONTRIBUTING.md) - How to contribute to the project
- [Security Policy](../SECURITY.md) - Security policies and vulnerability reporting
