# HeelonBackup - Secrets and Configuration Security Guide

## 🔒 Best Practices for Secure Configuration

HeelonBackup provides multiple ways to configure your SMB connection securely. **Never commit credentials to git.**

---

## 📁 Option 1: Environment Variables (Recommended)

### Create your `.env` file

1. Copy the example file:
   ```bash
   cp .env.example .env
   ```

2. Edit `.env` with your NAS credentials:
   ```bash
   nano .env
   ```

3. For your NAS at **192.168.1.22**, add:
   ```bash
   HEELONBACKUP_SMB_URL=smb://192.168.1.22/backup
   HEELONBACKUP_SMB_USERNAME=your_username
   HEELONBACKUP_SMB_PASSWORD=your_secure_password
   HEELONBACKUP_SMB_WORKGROUP=WORKGROUP
   HEELONBACKUP_SMB_ENCRYPT=true
   ```

4. Load the environment variables before running:
   ```bash
   # Using bash/zsh
   source .env
   heelonbackup backup
   
   # Or run directly
   env $(cat .env | xargs) heelonbackup backup
   ```

### Security Notes
- ✅ `.env` is in `.gitignore` - it will **never** be committed
- ✅ Environment variables are not logged or displayed
- ✅ Variables are only available for the current session

---

## 📁 Option 2: Separate Secrets File

If you prefer JSON configuration but want to keep secrets separate:

### 1. Create a secrets file
```bash
mkdir -p ~/.config/heelonbackup
nano ~/.config/heelonbackup/secrets.json
```

### 2. Add your credentials
```json
{
  "smb": {
    "url": "smb://192.168.1.22/backup",
    "username": "your_username",
    "password": "your_secure_password",
    "workgroup": "WORKGROUP",
    "encrypt": true
  }
}
```

### 3. Set restrictive permissions
```bash
chmod 600 ~/.config/heelonbackup/secrets.json
```

### 4. Use with HeelonBackup
```bash
# Load secrets from file and run
heelonbackup --config ~/.config/heelonbackup/secrets.json backup
```

### Security Notes
- ✅ File permissions set to read/write by owner only
- ✅ Located in user's home directory (not in project)
- ⚠️ Still a plain text file - consider environment variables instead

---

## 📁 Option 3: Split Configuration (Recommended for Security)

Keep public configuration in `config.json` and secrets in `.env`:

### 1. Public configuration (`~/.config/heelonbackup/config.json`)
```json
{
  "backup": {
    "sources": ["/home/user/Documents", "/home/user/Pictures"],
    "excludes": ["*.cache*", "*.thumbnails*"],
    "workers": 4
  },
  "storage": {
    "base_dir": "heelonbackup",
    "retention": 5
  }
}
```

### 2. Secrets in `.env`
```bash
HEELONBACKUP_SMB_URL=smb://192.168.1.22/backup
HEELONBACKUP_SMB_USERNAME=your_username
HEELONBACKUP_SMB_PASSWORD=your_secure_password
HEELONBACKUP_SMB_WORKGROUP=WORKGROUP
```

### 3. Run with both
```bash
source .env
heelonbackup backup
```

---

## 🛡️ Security Checklist

### Before Committing
- [ ] Run `git status` to check for uncommitted files
- [ ] Ensure `.env` is NOT in the output
- [ ] Ensure `secrets.json` is NOT in the output
- [ ] Verify `.gitignore` includes `.env`, `.env.*`, and `secrets.json`

### File Permissions
```bash
# Check permissions
ls -la ~/.config/heelonbackup/

# Set secure permissions (if using secrets file)
chmod 600 ~/.config/heelonbackup/secrets.json
chmod 700 ~/.config/heelonbackup/
```

### Never Do
- ❌ Commit `.env` files
- ❌ Commit `secrets.json` files
- ❌ Hardcode credentials in source code
- ❌ Store passwords in plain text configuration files in the repo

---

## 🧪 Testing Your Configuration

### Test SMB connection manually
```bash
# Using smbclient directly
smbclient //192.168.1.22/backup -U your_username

# With password from environment
PASSWD=your_password smbclient //192.168.1.22/backup -U your_username -c "ls"
```

### Test HeelonBackup connection
```bash
# With environment variables
source .env
heelonbackup config check

# Or with secrets file
heelonbackup --config ~/.config/heelonbackup/secrets.json config check
```

---

## 📝 Example: Full Setup for Your NAS (192.168.1.22)

### Step 1: Create .env file
```bash
cp .env.example .env
```

### Step 2: Edit .env
```bash
nano .env
```

Add:
```bash
# NAS at 192.168.1.22
HEELONBACKUP_SMB_URL=smb://192.168.1.22/backup
HEELONBACKUP_SMB_USERNAME=your_backup_user
HEELONBACKUP_SMB_PASSWORD=your_secure_password
HEELONBACKUP_SMB_WORKGROUP=WORKGROUP
HEELONBACKUP_SMB_ENCRYPT=true
HEELONBACKUP_SMB_TIMEOUT=60

# Logging
RUST_LOG=heelonbackup=debug
```

### Step 3: Load and test
```bash
source .env
heelonbackup config init
heelonbackup config check
```

### Step 4: Run your first backup
```bash
source .env
heelonbackup backup --dry-run  # Test without transferring
heelonbackup backup             # Real backup
```

---

## 🔄 Managing Multiple NAS Configurations

For multiple NAS servers:

### Option A: Multiple .env files
```bash
# For NAS at work
cp .env.example .env.work
# Edit with work credentials

# For NAS at home
cp .env.example .env.home
# Edit with home credentials

# Use the appropriate file
source .env.home
heelonbackup backup
```

### Option B: Configuration profiles
```bash
# Create separate config directories
mkdir -p ~/.config/heelonbackup/work
mkdir -p ~/.config/heelonbackup/home

# Each has its own config.json and secrets.json
```

---

## 💡 Pro Tips

1. **Use a password manager** to generate and store strong passwords
2. **Enable SMB encryption** (`encrypt: true`) for secure transfers
3. **Use wired connection** (Ethernet) instead of Wi-Fi for better performance
4. **Regularly update** your NAS firmware and smbclient package
5. **Test your backups** with `heelonbackup verify`

---

## 🚨 What If My Password Is Compromised?

1. **Immediately change** your NAS password
2. **Rotate** any other credentials that might have been exposed
3. **Audit** your backup logs for unauthorized access
4. **Review** file permissions on your configuration files

---

## 📚 See Also

- [User Guide](docs/user_guide.md) - Complete user documentation
- [Configuration Reference](docs/configuration.md) - All configuration options
- [Security Policy](SECURITY.md) - Security policies and procedures
