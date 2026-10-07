# Security Policy

## Supported Versions

This security policy applies to all versions of HeelonBackup that are currently supported. We recommend always using the latest stable version for the best security.

| Version | Supported | Maintenance Status |
|---------|----------|-------------------|
| 0.2.x   | ✅ Yes   | Active development |
| 0.1.x   | ✅ Yes   | Maintenance mode |
| < 0.1.0 | ❌ No    | Not maintained |

## Reporting a Vulnerability

We take security seriously. If you discover any security vulnerability in HeelonBackup, please report it to us responsibly.

### How to Report

**Please do NOT report security vulnerabilities through public GitHub issues.**

Instead, send us an email with details about the vulnerability:

📧 **Security Contact**: security@heelonbackup.dev (or use maintainer contact if this address doesn't exist)

Alternatively, if you don't have our direct contact, you can:
1. Open a **private** GitHub issue (if you have repository access)
2. Use GitHub's security advisory feature
3. Contact us through other secure channels

### What to Include in Your Report

To help us understand and reproduce the vulnerability, please include:

- A clear description of the vulnerability
- Steps to reproduce the issue
- The version of HeelonBackup you're using
- Your operating system and environment details
- Any relevant configuration information
- Potential impact of the vulnerability
- Any suggestions for mitigation or fixes

### Response Time

We will acknowledge your report within **48 hours** and provide an initial response.

| Severity | Response Time | Resolution Time |
|----------|---------------|-----------------|
| Critical | 24 hours | 7 days |
| High | 48 hours | 14 days |
| Medium | 72 hours | 30 days |
| Low | 1 week | 60 days |

### Security Updates

When a security vulnerability is discovered:

1. **Private Notification**: We notify maintainers and key contributors privately
2. **Fix Development**: We work on a fix in a private branch
3. **Testing**: The fix is thoroughly tested
4. **Release**: A patched version is released as soon as possible
5. **Disclosure**: We publicly disclose the vulnerability after the fix is released
6. **Advisory**: We publish a security advisory with details and mitigation steps

### Disclosure Policy

We follow a **coordinated disclosure** approach:

1. We give users reasonable time to update before disclosing details
2. We work with distributors to ensure patches are available
3. We provide clear guidance on mitigation steps
4. We credit the reporter (if they wish to be credited)

## Security Best Practices

### For Users

1. **Keep Updated**: Always use the latest version of HeelonBackup
2. **Secure Configuration**: Store passwords in environment variables, not in config files
3. **Network Security**: Use encrypted connections (SMB3 encryption enabled by default)
4. **Access Control**: Limit access to your configuration files
5. **Audit Logs**: Regularly review backup logs for suspicious activity

### For SMB/NAS Security

1. **Use Strong Passwords**: Ensure your SMB credentials are strong
2. **Enable Encryption**: Always use SMB3 encryption when available
3. **Firewall Rules**: Restrict SMB access to trusted networks only
4. **Regular Updates**: Keep your NAS firmware and SMB server updated
5. **Audit Shares**: Regularly review share permissions on your NAS

## Known Security Considerations

### SMB Client Dependencies

HeelonBackup uses the system `smbclient` for SMB operations. Security considerations:

- Ensure `smbclient` is from a trusted source (your distribution's package manager)
- Keep `smbclient` updated to the latest version
- Be aware that `smbclient` has its own security considerations

### Data Transmission

- **Without Encryption**: Data transmitted via SMB may be visible on the network
- **With Encryption**: SMB3 encryption protects data in transit (enabled by default)
- **At Rest**: Data on the NAS is stored as-is; consider NAS-side encryption if needed

### Password Storage

- **Recommended**: Use `HEELONBACKUP_SMB_PASSWORD` environment variable
- **Alternative**: Store in configuration file (less secure)
- **Not Recommended**: Never hardcode passwords in scripts or command lines

## Security Features

HeelonBackup includes the following security features:

1. **SHA-256 Checksums**: Every file is verified using cryptographic hashes
2. **Secure Password Handling**: Passwords can be passed via environment variables
3. **SMB3 Encryption**: Support for encrypted SMB connections
4. **No Plaintext Passwords**: Passwords are never logged or displayed in plaintext
5. **Minimal Dependencies**: Reduced attack surface through minimal external dependencies

## Credit

We want to give proper credit to security researchers who responsibly disclose vulnerabilities. If you report a security issue, let us know if you'd like to be credited in our release notes and security advisories.

## Legal

By reporting a vulnerability to us, you:

1. Agree that we may use the information to improve security
2. Grant us a non-exclusive license to reproduce, test, and fix the issue
3. Agree not to publicly disclose the vulnerability until we've had a reasonable time to respond

## Resources

- [OWASP Security Guidance](https://owasp.org/)
- [CVE Details](https://www.cvedetails.com/)
- [NIST National Vulnerability Database](https://nvd.nist.gov/)

---

**Last Updated**: October 7, 2026

**Maintainer**: HeelonBackup Team

For general questions (not security-related), please use the regular GitHub issues tracker.
