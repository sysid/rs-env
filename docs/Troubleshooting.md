# Troubleshooting

Solutions to common rsenv issues.

## Installation Issues

### "command not found: rsenv"

rsenv isn't in your PATH:

```bash
# If installed via cargo, add to PATH
export PATH="$HOME/.cargo/bin:$PATH"

# Add to shell config for persistence
echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.bashrc
source ~/.bashrc
```

### Cargo install fails

```bash
# Update Rust
rustup update

# Clean and retry
cargo clean
cargo install rsenv
```

## Vault Issues

### "Vault not initialized"

Project doesn't have a vault:

```bash
# Initialize
rsenv init

# Or check if you're in the right directory
pwd
ls -la .envrc
```

### Broken .envrc symlink

```bash
# Check symlink target
ls -la .envrc

# If vault exists but symlink is broken
rsenv init  # Will detect existing vault

# If vault was deleted
rsenv init  # Creates new vault
```

### "Cannot find vault" after directory move

If you moved your project directory:

```bash
# Symlinks may be broken if relative
# Reinitialize with absolute paths
rsenv init reset
rsenv init --absolute
```

### Wrong vault connected

```bash
# Check current vault
rsenv info

# Reset and reinitialize
rsenv init reset
rsenv init
```

## Environment Issues

### "File not found" for parent

Parent env file doesn't exist or path is wrong:

```bash
# Check the rsenv directive
cat problematic.env | grep "# rsenv:"

# Verify parent exists
ls -la $(dirname problematic.env)/

# Check path expansion
echo $RSENV_VAULT
```

### Variables not inheriting

```bash
# Check hierarchy order
rsenv env files leaf.env

# View tree
rsenv env tree

# Build and inspect
rsenv env build leaf.env
```

### Circular dependency error

Your environment files have a cycle:

```bash
# Check for cycles manually
# a.env: # rsenv: b.env
# b.env: # rsenv: a.env  <- cycle!

# Remove circular reference
vim offending.env
```

### Environment not loading

direnv might need to be allowed:

```bash
# Allow direnv
direnv allow

# Check if .envrc is executable/readable
ls -la .envrc

# Force reload
direnv reload
```

## Guarding Issues

### "File already guarded"

File is already a symlink to vault:

```bash
# Check file status
ls -la path/to/file

# If you need to re-guard, restore first
rsenv guard restore path/to/file
rsenv guard add path/to/file
```

### "File not within project"

Can only guard files inside the project directory:

```bash
# Check project root
rsenv info

# File must be under this directory
pwd
ls -la file-to-guard
```

### Guarded file missing from vault

```bash
# Check vault location
ls -la $RSENV_VAULT/guarded/

# The path mirrors project structure
ls -la $RSENV_VAULT/guarded/config/secrets.yaml
```

### Can't restore guarded file

```bash
# Check if file is actually a symlink
ls -la path/to/file

# Check vault file exists
ls -la $(readlink path/to/file)

# Manual restore if needed
cp $RSENV_VAULT/guarded/path/to/file ./path/to/file
rm path/to/file  # Remove symlink first
mv vault-file ./path/to/file
```

## Swap Issues

### "File already swapped on host X"

Another host has the file swapped in:

```bash
# Check status
rsenv swap status

# If safe, force swap
rsenv swap in --force file.yml
```

### Original file lost

Backup exists with `.rsenv_original` suffix:

```bash
# Find backup
ls -la *.rsenv_original

# Manual restore
mv file.yml.rsenv_original file.yml
rm file.yml.*.rsenv_active  # Clean up marker
```

### Swap file not in vault

Initialize it first:

```bash
rsenv swap init path/to/file
```

## SOPS Issues

### "gpg: decryption failed: No secret key"

Your GPG key isn't available:

```bash
# List your keys
gpg --list-secret-keys

# Import if needed
gpg --import /path/to/key.asc

# Check configured key matches
rsenv config show | grep gpg_key
```

### "SOPS could not find configuration"

No GPG key configured:

```bash
# Edit config
vim ~/.config/rsenv/rsenv.toml

# Add:
[sops]
gpg_key = "YOUR_FINGERPRINT"
```

### "No files match patterns"

Check your encryption patterns:

```bash
# Show config
rsenv config show

# Check what would be encrypted
rsenv sops status

# List files manually
ls -la $RSENV_VAULT/*.env
```

### Encrypted file won't decrypt

```bash
# Check key used to encrypt
sops --decrypt --verbose file.enc 2>&1 | grep key

# Verify you have that key
gpg --list-secret-keys | grep FINGERPRINT
```

## direnv Issues

### "direnv: error .envrc"

Syntax error in .envrc:

```bash
# Check .envrc content
cat .envrc

# Remember: it's a symlink
cat $RSENV_VAULT/dot.envrc

# Fix and reload
vim $RSENV_VAULT/dot.envrc
direnv allow
```

### "direnv: error blocked"

```bash
# Allow the .envrc
direnv allow

# If still blocked, check direnv config
cat ~/.config/direnv/direnv.toml
```

### Environment not updating

```bash
# Force reload
direnv reload

# Or leave and re-enter directory
cd .. && cd -
```

## Configuration Issues

### Config file not found

```bash
# Check paths
rsenv config path

# Create if missing
rsenv config init --global
```

### Environment variable not applied

```bash
# Verify export
echo $RSENV_VAULT_BASE_DIR

# Must be exported, not just set
export RSENV_VAULT_BASE_DIR=~/my-vaults
```

### TOML parse error

```bash
# Check syntax
cat ~/.config/rsenv/rsenv.toml

# Common issues:
# - Missing quotes around values with special chars
# - Incorrect array syntax (use ["a", "b"])
# - Tabs instead of spaces
```

## General Debugging

### Enable verbose output

```bash
rsenv --verbose <command>
```

### Check versions

```bash
rsenv --version
sops --version
gpg --version
direnv --version
```

### Reset everything

If all else fails:

```bash
# Backup vault first!
cp -r $RSENV_VAULT ~/vault-backup

# Reset project
rsenv init reset

# Delete vault
rm -rf $RSENV_VAULT

# Start fresh
rsenv init
```

## Getting Help

### Check Documentation

- [Core Concepts](Core-Concepts) - Understanding how rsenv works
- [Configuration](Configuration) - Config options and precedence

### File an Issue

If you've found a bug:

1. Check existing issues: https://github.com/sysid/rsenv/issues
2. Include:
   - rsenv version (`rsenv --version`)
   - OS and version
   - Steps to reproduce
   - Expected vs actual behavior
   - Relevant config (redact secrets!)

### Debug Information

Collect for bug reports:

```bash
rsenv --version
rsenv info
rsenv config show
rsenv config path
ls -la .envrc
echo $RSENV_VAULT
```
