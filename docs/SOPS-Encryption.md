# SOPS Encryption

rsenv integrates with [SOPS](https://github.com/getsops/sops) to encrypt sensitive files in your vault. This enables secure backup, version control, and sharing of encrypted vaults.

## Overview

SOPS (Secrets OPerationS) encrypts file contents while preserving structure. rsenv provides batch operations:

| Command | Purpose |
|---------|---------|
| `rsenv sops encrypt` | Encrypt files matching configured patterns |
| `rsenv sops decrypt` | Decrypt `.enc` files |
| `rsenv sops clean` | Delete plaintext files (after encryption) |
| `rsenv sops status` | Show encryption state |

## Setup

### 1. Install SOPS

```bash
# macOS
brew install sops

# Linux
curl -LO https://github.com/getsops/sops/releases/download/v3.8.1/sops-v3.8.1.linux.amd64
chmod +x sops-v3.8.1.linux.amd64
sudo mv sops-v3.8.1.linux.amd64 /usr/local/bin/sops
```

### 2. Create GPG Key (if needed)

```bash
# Generate key
gpg --gen-key

# List keys (note the fingerprint)
gpg --list-keys --keyid-format long
```

### 3. Configure rsenv

```bash
# Create global config
rsenv config init --global
```

Edit `~/.config/rsenv/rsenv.toml`:

```toml
[sops]
gpg_key = "60A4127E82E218297532FAB6D750B66AE08F3B90"  # Your GPG fingerprint

# File extensions to encrypt
file_extensions_enc = ["env", "envrc", "yaml", "yml", "json"]

# Specific filenames to encrypt
file_names_enc = ["dot_pypirc", "dot_pgpass", "kube_config"]

# Extensions to decrypt (encrypted files)
file_extensions_dec = ["enc"]
```

## Commands

### Encrypt Files

```bash
# Encrypt files in vault (default)
rsenv sops encrypt

# Encrypt files in specific directory
rsenv sops encrypt --dir /path/to/dir
```

**What happens:**
1. Finds files matching `file_extensions_enc` and `file_names_enc`
2. Encrypts each: `file.env` → `file.env.enc`
3. Updates `.gitignore` to exclude plaintext files
4. Original files remain (use `clean` to remove)

### Decrypt Files

```bash
# Decrypt .enc files in vault
rsenv sops decrypt

# Decrypt in specific directory
rsenv sops decrypt --dir /path/to/dir
```

**What happens:**
1. Finds files matching `file_extensions_dec` (default: `.enc`)
2. Decrypts each: `file.env.enc` → `file.env`
3. Encrypted files remain

### Clean Plaintext

```bash
# Remove plaintext files (after encryption)
rsenv sops clean
```

**What happens:**
1. Finds files matching encryption patterns (that have `.enc` counterparts)
2. Deletes the plaintext versions
3. Only encrypted files remain

### Check Status

```bash
rsenv sops status
```

Output:
```
SOPS Status for /home/user/.rsenv/vaults/myproject-abc123:
  Pending encryption:
    envs/local.env
    envs/prod.env
  Encrypted:
    envs/local.env.enc
    envs/prod.env.enc
  Would be cleaned:
    envs/local.env
    envs/prod.env
```

## Workflow

### Standard Encryption Workflow

```bash
# 1. Work on plaintext files
vim $RSENV_VAULT/envs/local.env

# 2. Encrypt when done
rsenv sops encrypt

# 3. Remove plaintext (optional, for sharing)
rsenv sops clean

# 4. Commit only encrypted files
cd $RSENV_VAULT
git add *.enc
git commit -m "Update encrypted configs"
```

### Standard Decryption Workflow

```bash
# 1. Decrypt to work on files
rsenv sops decrypt

# 2. Edit plaintext
vim $RSENV_VAULT/envs/local.env

# 3. Re-encrypt
rsenv sops encrypt

# 4. Clean up
rsenv sops clean
```

## .gitignore Management

When you run `rsenv sops encrypt` on the vault directory, rsenv automatically updates `.gitignore`:

```gitignore
# ---------------------------------- rsenv-sops-start -----------------------------------
*.env  # sops-managed 2024-01-15 10:30:45
*.envrc  # sops-managed 2024-01-15 10:30:45
*.yaml  # sops-managed 2024-01-15 10:30:45
dot_pgpass  # sops-managed 2024-01-15 10:30:45
# ---------------------------------- rsenv-sops-end -----------------------------------
```

**Patterns come from:**
- `file_extensions_enc` → `*.{ext}` patterns
- `file_names_enc` → exact filename patterns

**Benefits:**
- Prevents accidental commit of plaintext secrets
- Updated automatically during encryption
- Preserved across encrypt/decrypt cycles

## File Format Handling

SOPS auto-detects file formats:

| Extension | Handling |
|-----------|----------|
| `.json` | Structured (keys encrypted, structure visible) |
| `.yaml`, `.yml` | Structured |
| `.env`, `.envrc` | dotenv format (`--input-type dotenv`) |
| Other | Binary (entire file encrypted) |

### dotenv Files

For `.env` files, rsenv tells SOPS to preserve the dotenv format:

```bash
# Plaintext
DATABASE_URL=postgres://localhost/mydb
API_KEY=sk-secret-123

# Encrypted (structure preserved)
DATABASE_URL=ENC[AES256_GCM,data:...,tag:...]
API_KEY=ENC[AES256_GCM,data:...,tag:...]
```

## Configuration Reference

### Full SOPS Configuration

```toml
# ~/.config/rsenv/rsenv.toml

[sops]
# GPG key fingerprint (required for GPG encryption)
gpg_key = "60A4127E82E218297532FAB6D750B66AE08F3B90"

# Age public key (alternative to GPG)
# age_key = "age1..."

# Extensions to encrypt
file_extensions_enc = [
    "env",
    "envrc",
    "yaml",
    "yml",
    "json",
    "p12",       # PKCS#12 certificates
    "keystore",  # Java keystores
]

# Specific filenames to encrypt
file_names_enc = [
    "dot_pypirc",
    "dot_pgpass",
    "kube_config",
]

# Extensions to decrypt (typically just "enc")
file_extensions_dec = ["enc"]

# Specific filenames to decrypt (usually empty)
file_names_dec = []
```

### Environment Variables

Override config via environment:

```bash
export RSENV_SOPS_GPG_KEY="fingerprint"
export RSENV_SOPS_FILE_EXTENSIONS_ENC="env,yaml,json"
```

## Parallel Processing

rsenv uses parallel execution (via rayon) for batch operations:
- Default: 8 parallel threads
- Speeds up encryption/decryption of many files

## Age Backend (Alternative to GPG)

SOPS also supports [Age](https://github.com/FiloSottile/age) encryption:

```bash
# Generate Age key
age-keygen -o key.txt

# Configure
[sops]
age_key = "age1..."
```

Age is simpler than GPG but less widely supported.

## Security Model

### Defense in Depth

1. **Vault location**: Outside git repository
2. **Symlinks**: Git only sees symlinks, not actual secrets
3. **Encryption**: Vault contents encrypted at rest
4. **gitignore**: Auto-excludes plaintext files

### Typical Security Flow

```bash
# Work locally (plaintext)
rsenv sops decrypt
vim $RSENV_VAULT/envs/prod.env

# Before backup/share (encrypted)
rsenv sops encrypt
rsenv sops clean

# Vault now contains only *.enc files
```

## Troubleshooting

### "gpg: decryption failed: No secret key"

Your GPG key isn't available:

```bash
# List available keys
gpg --list-secret-keys

# Import if needed
gpg --import key.asc
```

### "Could not find SOPS configuration"

Ensure config is set:

```bash
rsenv config show | grep sops
```

### "No files match patterns"

Check your patterns:

```bash
# Show what would be encrypted
rsenv sops status

# Verify config
rsenv config show
```

### Encrypted file won't decrypt

SOPS stores key info in the encrypted file. Verify you have the right key:

```bash
# Check what key was used
sops --decrypt --verbose file.enc 2>&1 | grep -i key
```

## Related

- **[Configuration](Configuration)** - Full rsenv configuration options
- **[Vault Management](Vault-Management)** - Vault structure and guarding
- **[Core Concepts](Core-Concepts)** - Security philosophy
