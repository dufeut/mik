# Security Policy

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 0.1.x   | :white_check_mark: |
| < 0.1   | :x:                |

## Reporting a Vulnerability

We take security vulnerabilities seriously. If you discover a security issue, please report it responsibly.

### How to Report

1. **Do NOT** open a public GitHub issue for security vulnerabilities
2. Use GitHub's private vulnerability reporting: [Report a vulnerability](../../security/advisories/new)

### What to Include

- Description of the vulnerability
- Steps to reproduce
- Potential impact
- Suggested fix (if any)

### Response Timeline

- **Acknowledgment**: Within 48 hours
- **Initial Assessment**: Within 7 days
- **Fix Timeline**: Depends on severity
  - Critical: 24-72 hours
  - High: 7 days
  - Medium: 30 days
  - Low: Next release

### Security Measures

This project implements several security measures:

- **Sandboxed execution**: WASM modules run in isolated Wasmtime sandboxes
- **No filesystem access**: Handlers cannot access the host filesystem
- **No raw network**: Scripts have no network access; handlers use controlled HTTP
- **Path traversal prevention**: All paths are validated and canonicalized
- **Rate limiting**: Per-module and global request limits
- **Circuit breakers**: Automatic failure isolation
- **Input validation**: Request body size limits, header validation
- **Dependency auditing**: Automated `cargo-audit` in CI

### Dependency Security

We use automated tools to monitor dependencies:

- `cargo-audit` runs on every PR via GitHub Actions
- `cargo-deny` validates licenses and bans known-bad crates
- Dependabot alerts are enabled

## Security Best Practices for Operators

1. Run behind a TLS-terminating reverse proxy (Caddy/nginx)
2. Use systemd security hardening (see `deploy/mik.service`)
3. Enable firewall rules to block direct access to mik ports
4. Regularly update to the latest version
5. Monitor `/metrics` for anomalies
6. Set appropriate resource limits (memory, file descriptors)
