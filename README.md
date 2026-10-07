# HeelonBackup

[![Rust 2024](https://img.shields.io/badge/rust-2024_edition-orange.svg)](https://www.rust-lang.org/)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache_2.0-blue.svg)](LICENSE)
[![Code of Conduct](https://img.shields.io/badge/contributor-covenant-v2.1-blue.svg)](CODE_OF_CONDUCT.md)
[![GitHub release](https://img.shields.io/github/v/release/heelon/heelonbackup?display_name=tag)](https://github.com/heelon/heelonbackup/releases)
[![CI/CD](https://github.com/heelon/heelonbackup/actions/workflows/ci.yml/badge.svg)](https://github.com/heelon/heelonbackup/actions/workflows/ci.yml)
[![dependencies](https://img.shields.io/badge/dependencies-up%20to%20date-green)](https://github.com/heelon/heelonbackup/blob/main/Cargo.toml)
[![Maintenance](https://img.shields.io/badge/maintenance-actively%20developed-brightgreen)](https://github.com/heelon/heelonbackup)

**HeelonBackup** is an open-source, reliable, and optimized backup tool written in Rust for backing up Linux laptops (Fedora 44+) to Synology NAS (BeeStation) via SMB.

## ⭐ Features

- ✅ **Reliable Backups**: Every file is verified using SHA-256 checksums
- ✅ **Parallel Transfers**: Uses multiple smbclient sessions for maximum performance
- ✅ **Clear Status**: Displays date, time, size, and status of each backup
- ✅ **Restore Capability**: Can restore files from backups
- ✅ **Verification**: Full integrity verification with optional deep mode
- ✅ **Error Handling**: Robust error management with clear messages
- ✅ **Flexible Configuration**: JSON configuration with sources, exclusions, authentication
- ✅ **Progress Tracking**: Real-time progress bars with ETA
- ✅ **Local History**: Backup history stored on your machine

## 📋 Requirements

### System
- Linux Fedora 44+ (or any recent distribution with glibc 2.34+)
- Rust 2024 edition (latest stable version recommended)
- `smbclient` installed: `sudo dnf install samba-client`
- `cifs-utils` for CIFS mounts (optional)

### Synology NAS
- SMB share configured and accessible
- Write access for the backup user
- SMB3 recommended (encryption support)

## 🚀 Installation

### From Source

```bash
# Clone the repository
git clone https://github.com/heelon/heelonbackup.git
cd heelonbackup

# Build in release mode (optimized)
cargo build --release

# Install globally
sudo cp target/release/heelonbackup /usr/local/bin/
```

### With Cargo

```bash
cargo install --git https://github.com/heelon/heelonbackup.git
```

## 📖 Configuration

### Quick Setup

```bash
# Create default configuration
heelonbackup config init

# Or edit manually
heelonbackup config edit
```

### Manual Configuration

Create the file `~/.config/heelonbackup/config.json`:

```json
{
  "smb": {
    "url": "smb://192.168.1.100/backup",
    "username": "your_username",
    "workgroup": "WORKGROUP",
    "timeout": 60,
    "encrypt": true
  },
  "backup": {
    "sources": ["/home/user"],
    "excludes": [
      "*.cache*",
      "*.thumbnails*",
      ".Trash*",
      ".local/share/Trash*",
      "node_modules",
      ".git",
      "target",
      "__pycache__",
      ".vscode",
      ".idea"
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

### SMB Options
| Option | Description | Default |
|--------|-------------|--------|
| `url` | SMB share URL (e.g., `smb://nas-ip/share[/folder]`) | **Required** |
| `username` | Username for SMB connection | **Required** |
| `password` | Password (optional, prefer environment variable) | `null` |
| `workgroup` | Workgroup/domain | `null` |
| `timeout` | Timeout in seconds for each operation | `30` |
| `encrypt` | Require SMB3 encryption | `true` |

### Backup Options
| Option | Description | Default |
|--------|-------------|--------|
| `sources` | Source directories to backup | `["/home"]` |
| `excludes` | Exclusion patterns (glob patterns) | `[]` |
| `max_file_size` | Maximum file size (0 = unlimited) | `0` |
| `workers` | Number of parallel workers | CPU count |

### Environment Variables

```bash
# SMB password (preferred over configuration file)
export HEELONBACKUP_SMB_PASSWORD="your_password"

# Log level (debug, info, warn, error)
export RUST_LOG=heelonbackup=debug
```

## 🎯 Commands

### 💾 Backup

```bash
# Full backup of configured sources
heelonbackup backup

# Backup with specific sources
heelonbackup backup /home/user/Documents /home/user/Pictures

# Backup with temporary exclusions
heelonbackup backup --exclude "*.tmp" --exclude "*.log"

# Dry run - shows what would be backed up
heelonbackup backup --dry-run
```

### 📊 Status

```bash
# Status of the last backup
heelonbackup status

# Detailed status with failed/skipped files
heelonbackup status --detailed

# Show all backups
heelonbackup status --all
```

Example output:
```
✅ heelonbackup/20261007_143022
   Started:   2026-10-07 14:30:22
   Finished:  2026-10-07 14:35:12 (4m 50s)
   Files:     12487 (12487 new, 0 updated)
   Size:      45.67 GB
   Rate:      158.2 MB/s
   Status:    completed
   Sources:   /home/user
```

### 📋 List

```bash
# List all backups on the NAS
heelonbackup list
```

Example output:
```
heelonbackup/20261007_143022  ✅ completed     45.67 GB  12487 files  2026-10-07 14:30:22
heelonbackup/20261006_180000  ✅ completed     45.23 GB  12345 files  2026-10-06 18:00:00
heelonbackup/20261005_101530  ⚠️  completed w/errors  45.10 GB  12200 files  2026-10-05 10:15:30
```

### ✅ Verify

```bash
# Verify the last backup
heelonbackup verify

# Verify a specific backup
heelonbackup verify 20261007_143022

# Deep verification (downloads and checks every file)
heelonbackup verify --deep
```

### 🔄 Restore

```bash
# Restore files (dry run by default)
heelonbackup restore 20261007_143022 /home/user/restored

# Restore with actual execution
heelonbackup restore 20261007_143022 /home/user/restored --execute

# Restore specific files
heelonbackup restore 20261007_143022 /home/user/restored --files "/home/user/Documents/important.txt"

# Restore with filters
heelonbackup restore 20261007_143022 /home/user/restored --pattern "*.pdf"
```

### ⚙️ Configuration

```bash
# Initialize configuration
heelonbackup config init

# Edit configuration
heelonbackup config edit

# Validate configuration
heelonbackup config check

# Show current configuration
heelonbackup config show
```

### 🎯 Shell Completions

```bash
# Generate completions for bash
heelonbackup completions bash > ~/.local/share/bash-completion/completions/heelonbackup

# For zsh
heelonbackup completions zsh > ~/.local/share/zsh/site-functions/_heelonbackup

# For fish
heelonbackup completions fish > ~/.config/fish/completions/heelonbackup.fish
```

## 🛡️ Security

- **Encryption**: Support for SMB3 traffic encryption
- **Password Security**: No password storage in logs or command line
- **Environment Variables**: Password can be passed via `HEELONBACKUP_SMB_PASSWORD`
- **Verification**: SHA-256 checksums for every backed up file
- **Permissions**: Preserves original Unix permissions

## 📊 Technical Architecture

```
src/
├── main.rs              # Entry point with panic handling
├── lib.rs               # Main library exports
├── config.rs            # Configuration management
├── error.rs             # Error types with thiserror
├── ui.rs                # User interface (progress bars, formatting)
├── backup/              # Local side (discovery & hashing)
│   ├── mod.rs           # Exports: scan, hash_file, Excludes, ScanReport
│   ├── scanner.rs       # File scanning with exclusion handling
│   └── hasher.rs        # SHA-256 checksums (files and memory)
└── storage/             # Remote side (SMB, manifests, transfers)
    ├── mod.rs           # Exports: SmbClient, BackupManifest, Transfer, LocalHistory
    ├── smb_client.rs    # smbclient wrapper with session management
    ├── backup_manifest.rs # Backup manifest with local history
    └── transfer.rs      # Parallel transfer with batching
└── cli/                 # CLI commands
    ├── mod.rs           # Main CLI with routing
    ├── common.rs        # Common code (config loading, exit codes)
    ├── backup.rs        # Backup command
    ├── status.rs        # Status command
    ├── list.rs          # List command
    ├── verify.rs        # Verify command
    ├── restore.rs       # Restore command
    └── config_cmd.rs    # Configuration management
```

### Internal Workflow

1. **Scan**: `backup::scanner` discovers all files in source directories
2. **Hashing**: `backup::hasher` computes SHA-256 checksums
3. **Transfer**: `storage::transfer` handles parallel transfer via smbclient
4. **Manifest**: `storage::backup_manifest` stores metadata for each backup
5. **History**: Local history storage in `~/.local/share/heelonbackup/history/`

### Parallel Transfer
- Files are grouped in batches (200 files or 256MB per batch)
- Each batch is processed by a separate smbclient session
- Multiple workers run concurrently (configurable)
- smbclient confirms each transferred file on stderr
- Progress bar uses these confirmations

## 📈 Performance

- **Parallel Transfer**: Up to N concurrent smbclient sessions (N = worker count)
- **Optimized Batches**: 200 files or 256MB per session for maximum throughput
- **Efficient Hashing**: SHA-256 computed during transfer to minimize total time
- **Low Memory**: Minimal memory footprint through streaming

## 🔧 Troubleshooting

### smbclient not installed
```
error: smbclient is not installed
```
**Solution**: `sudo dnf install samba-client`

### SMB connection problem
```
error: cannot reach the NAS: connection failed
```
**Solution**:
```bash
# Test connection manually
smbclient //nas-ip/share -U your_username
```

### Authentication problem
```
error: SMB authentication failed: session setup failed
```
**Solution**:
- Verify username and password
- Try with environment variable: `export HEELONBACKUP_SMB_PASSWORD="your_password"`

### Missing configuration
```
error: configuration file not found at ~/.config/heelonbackup/config.json
```
**Solution**: `heelonbackup config init`

### Permission problem
```
error: SMB operation failed: NT_STATUS_ACCESS_DENIED
```
**Solution**: Check permissions on the SMB share

## 🧪 Development

### Prerequisites
- Rust 2024 edition
- Linux with glibc 2.34+
- smbclient installed

### Setup
```bash
git clone https://github.com/heelon/heelonbackup.git
cd heelonbackup
cargo build
```

### Tests
```bash
# Run all tests
cargo test

# Specific tests
cargo test scanner
cargo test hasher
cargo test transfer
```

### Checks
```bash
# Code formatting
cargo fmt --check

# Static analysis
cargo clippy

# Security audit
cargo audit
```

## 📚 Documentation

- [User Guide (English)](docs/user_guide.md)
- [Guide Utilisateur (Français)](docs/user_guide_fr.md)
- [Configuration Reference](docs/configuration.md)
- [CLI Reference](docs/cli_reference.md)

## 📜 License

Apache License 2.0 - see [LICENSE](LICENSE) for details.

## 🤝 Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for contribution guidelines.

## 👤 Code of Conduct

This project follows the [Contributor Covenant](CODE_OF_CONDUCT.md). By participating, you are expected to uphold this code.

## 🛡️ Security

See [SECURITY.md](SECURITY.md) for security policies and vulnerability reporting procedures.

## 🙏 Acknowledgments

- [Rust Language](https://www.rust-lang.org/) for the amazing language
- [Synology](https://www.synology.com/) for reliable NAS solutions
- [Fedora Project](https://fedoraproject.org/) for the operating system
- [clap-rs](https://github.com/clap-rs/clap) for excellent CLI parsing
- [tokio-rs](https://github.com/tokio-rs/tokio) for async runtime

---

**HeelonBackup** - Your reliable and performant backup solution to Synology NAS

*Made with ❤️ and Rust*

> **Note**: This project uses system `smbclient` for SMB operations. Ensure it is installed and available in your PATH.
