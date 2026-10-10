# Justfile for HeelonBackup development and testing

# Every recipe runs in the development environment: configuration and history in
# ./.heelonbackup/, backups in a dev_* folder on the NAS (production is never touched)
export HEELONBACKUP_ENV := "dev"

# Default target - build the project
build:
    cargo build

# Build in release mode
build-release:
    cargo build --release

# Run the application
run *args:
    cargo run -- @args

# Configuration commands
config-init:
    cargo run -- config init

config-edit:
    cargo run -- config edit

config-check:
    cargo run -- config check

config-show:
    cargo run -- config show

# Test commands
test:
    cargo test

test-scanner:
    cargo test scanner

test-hasher:
    cargo test hasher

test-transfer:
    cargo test transfer

# Quality checks
clippy:
    cargo clippy

fmt:
    cargo fmt --check

fmt-fix:
    cargo fmt

audit:
    cargo audit

# Combined dev workflow (build + test + lint)
dev: build test clippy fmt

# Clean
clean:
    cargo clean

# Rebuild from scratch
rebuild: clean build

# ============================================
# FIRST BACKUP VALIDATION (RELEASE MODE)
# ============================================

# Scan a selected source without connecting to the NAS, hashing or writing
backup-dryrun source:
    cargo run --release -- backup {{quote(source)}} --dry-run

# Back up a selected source (real transfer; does not trigger retention)
backup-execute source:
    cargo run --release -- backup {{quote(source)}}

# Show the last backup result, including failed and skipped files
status-detailed:
    cargo run --release -- status --detailed

# Check presence and size of the files in a named backup
verify backup:
    cargo run --release -- verify {{quote(backup)}}

# Download every file in a named backup and compare SHA-256 checksums
verify-deep backup:
    cargo run --release -- verify {{quote(backup)}} --deep

# Plan restoration under an explicit target folder without writing
restore-dryrun backup target:
    cargo run --release -- restore {{quote(backup)}} --target {{quote(target)}} --dry-run

# Restore under an explicit target folder (keeps existing differing files)
restore-execute backup target:
    cargo run --release -- restore {{quote(backup)}} --target {{quote(target)}}

# ============================================
# FULL LAPTOP BACKUP (SYSTEM + /home/Patrick)
# ============================================
# Sources, exclusions and NAS folder: .heelonbackup/laptop-full.json
# Runs as root (sudo) to read /etc, /root... with the release binary.

laptop_config := justfile_directory() / ".heelonbackup/laptop-full.json"
inventory_dir := "/home/Patrick/.local/state/heelonbackup/inventory"

# Record what the backup cannot store (symlinks, owners, packages...) to rebuild the system after a crash
system-inventory:
    #!/usr/bin/env bash
    set -uo pipefail
    dir={{quote(inventory_dir)}}
    sys_paths=(/etc /root /usr/local /opt /var/spool/cron)
    mkdir -p "$dir"
    echo "=== System inventory in $dir ==="
    sudo -v || exit 1
    {
        cat /etc/os-release; echo; uname -a; echo; hostnamectl 2>/dev/null
    } > "$dir/os.txt"
    rpm -qa --qf '%{NAME}\n' | sort -u > "$dir/packages-all.txt"
    dnf repoquery --userinstalled --queryformat '%{name}\n' 2>/dev/null | sort -u > "$dir/packages-userinstalled.txt"
    flatpak list --app --columns=application,origin,installation > "$dir/flatpaks.txt" 2>/dev/null
    systemctl list-unit-files --state=enabled --no-legend > "$dir/systemd-enabled.txt"
    systemctl --user list-unit-files --state=enabled --no-legend > "$dir/systemd-user-enabled.txt" 2>/dev/null
    lsblk -o NAME,FSTYPE,SIZE,MOUNTPOINTS,UUID,LABEL > "$dir/disks.txt"
    sudo btrfs subvolume list / >> "$dir/disks.txt" 2>/dev/null
    code --list-extensions --show-versions > "$dir/vscode-extensions.txt" 2>/dev/null
    cargo install --list > "$dir/cargo-installed.txt" 2>/dev/null
    uv tool list > "$dir/uv-tools.txt" 2>/dev/null
    pipx list --short > "$dir/pipx.txt" 2>/dev/null
    # Owners, groups, permissions and ACLs: restore with `setfacl --restore=FILE`
    sudo getfacl -R -p --absolute-names "${sys_paths[@]}" > "$dir/system-permissions.acl" 2>/dev/null
    # Symbolic links (enabled services, alternatives...) are not backed up: path<TAB>target
    sudo find "${sys_paths[@]}" -xdev -type l -printf '%p\t%l\n' > "$dir/system-symlinks.tsv" 2>/dev/null
    find /home/Patrick -xdev \( -path /home/Patrick/.cache -o -path /home/Patrick/.local/share/containers \) -prune \
        -o -type l -printf '%p\t%l\n' > "$dir/home-symlinks.tsv" 2>/dev/null
    cat > "$dir/RESTORE.txt" <<'EOF'
    After a reinstallation of Fedora (same user Patrick, uid 1001):
    1. sudo dnf install $(cat packages-userinstalled.txt)          # then flatpaks.txt, vscode-extensions.txt...
    2. heelonbackup restore BACKUP --target /tmp/restore            # never restore /etc directly over the new system
    3. Copy back the needed files from /tmp/restore/etc, /tmp/restore/root, /tmp/restore/home/Patrick...
    4. sudo setfacl --restore=system-permissions.acl                # owners and permissions (missing files are reported)
    5. Recreate symlinks: while IFS=$'\t' read -r p t; do sudo ln -sfn "$t" "$p"; done < system-symlinks.tsv
    6. sudo restorecon -RF /etc /root /usr/local /opt /home/Patrick  # SELinux labels
    7. Python venvs, node_modules, Rust toolchains, containers and caches are not saved: reinstall/rebuild them.
    EOF
    ls -lh "$dir"

# Simulate the full laptop backup as root (no NAS transfer). Add --list to print every file
backup-laptop-dryrun *flags: build-release system-inventory
    #!/usr/bin/env bash
    set -uo pipefail
    bin="$(cargo metadata --format-version 1 --no-deps | jq -r .target_directory)/release/heelonbackup"
    sudo --preserve-env=HEELONBACKUP_ENV,RUST_LOG "$bin" -c {{quote(laptop_config)}} backup --dry-run {{flags}}
    code=$?
    sudo chown -R "$(id -u):$(id -g)" {{quote(justfile_directory() / ".heelonbackup")}}
    exit $code

# Full laptop backup as root: system essentials + /home/Patrick (real transfer, applies retention)
backup-laptop: build-release system-inventory
    #!/usr/bin/env bash
    set -uo pipefail
    bin="$(cargo metadata --format-version 1 --no-deps | jq -r .target_directory)/release/heelonbackup"
    echo "=== Full laptop backup (system + /home/Patrick) — close browsers, VS Code and VMs first ==="
    sudo --preserve-env=HEELONBACKUP_ENV,HEELONBACKUP_SMB_PASSWORD,RUST_LOG \
        "$bin" -c {{quote(laptop_config)}} backup
    code=$?
    # The history was written by root: give it back to the user
    sudo chown -R "$(id -u):$(id -g)" {{quote(justfile_directory() / ".heelonbackup")}}
    exit $code

# ============================================
# SPECIFIC TEST COMMANDS FOR PATRICK
# ============================================

# Setup configuration for Patrick's test
setup-patrick:
    @echo "=== Setting up configuration for Patrick ==="
    cargo run -- config init
    @echo ""
    @echo "Now edit .heelonbackup/config.json (dev configuration) to set:"
    @echo "  - smb.url: your NAS SMB share URL"
    @echo "  - smb.username: your NAS username"
    @echo "  - Set HEELONBACKUP_SMB_PASSWORD environment variable"
    @echo ""

# Test backup of /home/Patrick/Documents (DRY RUN - safe mode)
backup-patrick-dryrun:
    @echo "=== Testing backup of /home/Patrick/Documents (DRY RUN) ==="
    @echo "This will only scan metadata: no NAS connection, no hashing, no transfer"
    @echo ""
    cargo run -- backup /home/Patrick/Documents --dry-run

# Test backup of /home/Patrick/Documents (REAL BACKUP)
backup-patrick:
    @echo "=== WARNING: This will transfer files to your NAS ==="
    @echo "Make sure your NAS configuration is correct!"
    @echo ""
    @echo "To proceed with actual backup, run:"
    @echo "  just backup-patrick-execute"

# Execute actual backup (no confirmation - use with caution)
backup-patrick-execute:
    @echo "=== Executing REAL backup of /home/Patrick/Documents ==="
    cargo run --release -- backup /home/Patrick/Documents

# Quick test sequence
quick-test: build backup-patrick-dryrun

# Full test sequence
full-test: build test backup-patrick-dryrun

# ============================================
# INFORMATION COMMANDS
# ============================================

# Show the actual commands to run for first backup
backup-commands:
    @echo "=========================================="
    @echo "COMMANDS FOR FIRST BACKUP TEST"
    @echo "=========================================="
    @echo ""
    @echo "Step 1: Build the optimized application"
    @echo "  just build-release"
    @echo ""
    @echo "Step 2: Initialize configuration"
    @echo "  just config-init"
    @echo ""
    @echo "Step 3: Edit .heelonbackup/config.json (dev configuration)"
    @echo "  Set these values:"
    @echo "    smb.url: \"smb://<NAS_IP>/backup\""
    @echo "    smb.username: \"your_nas_username\""
    @echo "    smb.workgroup: \"WORKGROUP\" (or your workgroup)"
    @echo "    smb.encrypt: true"
    @echo "    smb.timeout: 60"
    @echo "    storage.base_dir: \"dev_heelonbackup\" (must start with dev_ in dev)"
    @echo ""
    @echo "Step 4: Set password as environment variable"
    @echo "  export HEELONBACKUP_SMB_PASSWORD=\"your_password\""
    @echo ""
    @echo "Step 5: Test with dry-run (RECOMMENDED FIRST)"
    @echo "  just backup-patrick-dryrun"
    @echo ""
    @echo "Step 6: Back up a small, stable subfolder first (replace the example path)"
    @echo "  just backup-execute '/home/Patrick/Documents/SOUS_DOSSIER_TEST'"
    @echo "  just status-detailed"
    @echo ""
    @echo "Step 7: Check its integrity (replace NOM_DU_BACKUP with the printed YYYYMMDD_HHMMSS)"
    @echo "  just verify-deep NOM_DU_BACKUP"
    @echo ""
    @echo "Step 8: Restore to a dedicated empty folder, first as a dry run"
    @echo "  just restore-dryrun NOM_DU_BACKUP '/home/Patrick/heelonbackup-restore-test'"
    @echo "  just restore-execute NOM_DU_BACKUP '/home/Patrick/heelonbackup-restore-test'"
    @echo "  Original absolute paths are recreated under the target folder."
    @echo ""
    @echo "Step 9: Back up all Documents after the small test succeeds"
    @echo "  just backup-patrick-execute"
    @echo "  Close applications writing to Documents and check free space on the NAS first."
    @echo ""
    @echo "=========================================="
    @echo "WORKFLOW CONFIRMATION"
    @echo "=========================================="
    @echo "HeelonBackup workflow:"
    @echo "  1. SCAN: Discovers all files in source directory"
    @echo "  2. HASH: Computes SHA-256 checksums for each file"
    @echo "  3. TRANSFER: Parallel transfer to NAS via SMB"
    @echo "  4. MANIFEST: Stores backup metadata"
    @echo "Dry run only scans metadata; it does not connect to the NAS nor test writing."
    @echo "Automatic verification (backup.verify): presence and size, NOT SHA-256."
    @echo "Use just verify-deep NOM_DU_BACKUP to check SHA-256 on downloaded NAS files."
    @echo ""
    @echo "Note: HeelonBackup does NOT compress locally first."
    @echo "It transfers files directly to NAS via smbclient."
    @echo "The NAS receives the files as-is (no compression)."
    @echo ""
