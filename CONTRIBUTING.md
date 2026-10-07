# Contributing to HeelonBackup

First off, thank you for considering contributing to HeelonBackup! 🎉 It's people like you that make open source such a great community.

This document provides guidelines for contributing to HeelonBackup. Following these guidelines helps us maintain a high standard of quality for the project and makes it easier for us to review and merge your contributions.

## 🤝 Ways to Contribute

There are many ways you can contribute to HeelonBackup:

### 🐛 Reporting Bugs

If you find a bug, please report it! Bug reports help us improve the software for everyone.

**Before submitting a bug report:**

1. Check if the issue has already been reported
2. Verify you're using the latest version
3. Try to reproduce the issue with a minimal configuration

**How to submit a good bug report:**

1. Use a clear, descriptive title
2. Include the version of HeelonBackup you're using
3. Describe your environment (OS, Rust version, NAS model, etc.)
4. Provide clear steps to reproduce the issue
5. Include relevant configuration details (remove sensitive information)
6. Add any error messages or log output
7. Explain the expected vs. actual behavior

### 💡 Suggesting Enhancements

Have an idea for a new feature or improvement? We'd love to hear it!

**Before submitting a feature request:**

1. Check if it's already been requested
2. Consider if it fits within the scope of HeelonBackup
3. Think about how it would work and potential edge cases

**How to submit a good feature request:**

1. Use a clear, descriptive title
2. Explain the use case and why it's valuable
3. Describe the proposed solution (if you have one)
4. Include examples or mockups if applicable

### 👩‍💻 Code Contributions

We welcome pull requests! Here's how to contribute code:

#### Getting Started

1. **Fork the repository** on GitHub
2. **Clone your fork** locally:
   ```bash
   git clone https://github.com/your-username/heelonbackup.git
   cd heelonbackup
   ```
3. **Create a feature branch**:
   ```bash
   git checkout -b feature/your-feature-name
   ```
4. **Set up the development environment**:
   ```bash
   cargo build
   cargo test
   ```

#### Development Guidelines

1. **Code Style**:
   - Follow Rust's standard style conventions
   - Use `cargo fmt` to format your code
   - Run `cargo clippy` to catch linting issues

2. **Commits**:
   - Use clear, descriptive commit messages
   - Follow the [Conventional Commits](https://www.conventionalcommits.org/) convention
   - Keep commits atomic (one logical change per commit)
   - Reference issue numbers when applicable

3. **Tests**:
   - Add tests for new functionality
   - Ensure existing tests still pass
   - Include edge cases in your tests

4. **Documentation**:
   - Update documentation for any changes
   - Add doc comments for new public APIs
   - Keep README and other docs up-to-date

#### Pull Request Guidelines

1. **Title**: Use a clear, descriptive title prefixed with `feat:`, `fix:`, `docs:`, `refactor:`, etc.
2. **Description**: Include a clear description of what the PR does
3. **Linked Issues**: Reference any related issues
4. **Tests**: Ensure all tests pass
5. **Checks**: Ensure CI passes (format, lint, build, test)

**Example PR title:**
```
feat: add support for SMB3 encryption configuration
fix: handle network timeouts gracefully
```

### 📚 Documentation Contributions

Improving documentation is always welcome! This includes:

- Fixing typos or unclear explanations
- Adding examples
- Improving existing documentation
- Translating documentation to other languages
- Writing tutorials or guides

### 🌍 Community Contributions

You can also contribute by:

- Answering questions on discussions or issues
- Helping others with their problems
- Reviewing pull requests
- Promoting the project
- Sharing your experiences

## 🛠️ Development Setup

### Prerequisites

- Rust 2024 edition (latest stable recommended)
- `smbclient` installed (`sudo dnf install samba-client` or equivalent)
- Git
- Cargo (comes with Rust)

### Install Dependencies

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install smbclient
# Fedora/RHEL:
sudo dnf install samba-client
# Debian/Ubuntu:
sudo apt-get install smbclient
# Arch:
sudo pacman -S smbclient
```

### Build and Test

```bash
# Clone the repository
git clone https://github.com/heelon/heelonbackup.git
cd heelonbackup

# Build the project
cargo build

# Run tests
cargo test

# Check formatting
cargo fmt --check

# Run clippy for linting
cargo clippy

# Run all checks
cargo check
```

### Running Locally

```bash
# Build and run
cargo run -- backup

# Or build in release mode
cargo build --release
./target/release/heelonbackup --help
```

## 🏗️ Project Structure

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
│   └── hasher.rs        # SHA-256 checksums
└── storage/             # Remote side (SMB, manifests, transfers)
    ├── mod.rs           # Exports: SmbClient, BackupManifest, Transfer
    ├── smb_client.rs    # smbclient wrapper
    ├── backup_manifest.rs # Backup manifest and history
    └── transfer.rs      # Parallel transfer logic
└── cli/                 # CLI commands
    ├── mod.rs           # Main CLI with routing
    ├── common.rs        # Common code
    ├── backup.rs        # Backup command
    ├── status.rs        # Status command
    ├── list.rs          # List command
    ├── verify.rs        # Verify command
    ├── restore.rs       # Restore command
    └── config_cmd.rs    # Configuration management
```

## 📝 Coding Standards

### Rust Standards

- Follow the [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- Use idiomatic Rust code
- Prefer immutable data where possible
- Use proper error handling with `thiserror` and `anyhow`
- Document public APIs with doc comments
- Use appropriate types and avoid primitive obsession

### Error Handling

- Use custom error types from `error.rs`
- Provide context in error messages
- Handle errors gracefully, don't panic
- Use `?` operator for propagation
- Use `anyhow::Context` for additional error context

### Logging

- Use the `tracing` crate for logging
- Use appropriate log levels:
  - `debug!` for detailed debugging information
  - `info!` for important operational messages
  - `warn!` for potentially harmful situations
  - `error!` for errors that should be investigated
- Include meaningful context in log messages

### Testing

- Use Rust's built-in test framework
- Place tests in the same file as the code they test (module-level)
- Use `#[cfg(test)]` for test modules
- Test both success and error cases
- Use `tempfile` for temporary files in tests

### Documentation

- Use Markdown for documentation
- Follow the style of existing documentation
- Use clear, concise language
- Include examples where helpful
- Document all public APIs

## ✅ Pull Request Checklist

Before submitting a pull request, make sure:

- [ ] Code follows the project's coding standards
- [ ] All existing tests pass
- [ ] New tests are added for new functionality
- [ ] Code is properly formatted (`cargo fmt`)
- [ ] No clippy warnings (`cargo clippy`)
- [ ] Documentation is updated (if applicable)
- [ ] Commit messages follow conventions
- [ ] PR title is clear and descriptive
- [ ] PR description explains the changes

## 📋 Review Process

Once you submit a pull request:

1. **Initial Review**: Maintainers will review your PR within a few days
2. **Feedback**: You may receive feedback or requests for changes
3. **CI Checks**: All checks must pass before merging
4. **Approval**: At least one maintainer must approve the PR
5. **Merge**: Once approved and all checks pass, your PR will be merged

We strive to provide constructive feedback and guide you through the process. Don't be discouraged by requests for changes - they're meant to help improve your contribution!

## 🎁 Recognition

All meaningful contributions will be recognized:

- Code contributors will be added to the project's contributors list
- Significant contributions may receive special recognition
- Security vulnerability reporters may be credited in security advisories

## 🤔 Need Help?

If you have questions about contributing:

1. **Check this document** for guidelines
2. **Look at existing PRs** for examples
3. **Open a discussion** on GitHub for general questions
4. **Ask in issues** if you're working on a specific problem
5. **Contact maintainers** directly for sensitive matters

## 📜 License

By contributing to HeelonBackup, you agree that your contributions will be licensed under the same license as the project (Apache 2.0).

## 🙏 Thank You!

Your contributions help make HeelonBackup better for everyone. Whether it's a bug report, feature request, code contribution, or documentation improvement, we appreciate your time and effort!

Happy coding! 🚀

---

**Maintainers**: HeelonBackup Team

**Last Updated**: October 7, 2026
