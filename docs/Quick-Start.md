# Quick Start Guide

Get productive in 5 minutes.

## Prerequisites

- [direnv](https://direnv.net/) installed and hooked into your shell
- A project directory you want to manage

## Installation

```bash
# Via Cargo (Rust)
cargo install rsenv

# Verify installation
rsenv --version
```

For detailed installation instructions, see [Installation](Installation).

## Your First Vault

```bash
# Navigate to your project
cd ~/myproject

# Initialize a vault
rsenv init

# Check what happened
rsenv info
```

**What just happened:**
- Created vault at `~/.rsenv/vaults/myproject-{id}/`
- Moved `.envrc` to vault as `dot.envrc` (or created empty one)
- Created symlink: `.envrc` → vault's `dot.envrc`
- Set up `RSENV_VAULT` environment variable

## Set Up Environment Hierarchy

```bash
# Create a base environment (shared settings)
cat > $RSENV_VAULT/envs/base.env << 'EOF'
export DATABASE_PORT=5432
export LOG_FORMAT=json
export TIMEOUT=30
EOF

# Make local.env inherit from base
cat > $RSENV_VAULT/envs/local.env << 'EOF'
# rsenv: base.env
export RUN_ENV="local"
export DATABASE_HOST=localhost
export LOG_LEVEL=debug
EOF

# Build and see the merged result
rsenv env build $RSENV_VAULT/envs/local.env
```

**Expected output:**
```bash
export DATABASE_PORT=5432    # from base.env
export LOG_FORMAT=json       # from base.env
export TIMEOUT=30            # from base.env
export RUN_ENV="local"       # from local.env
export DATABASE_HOST=localhost
export LOG_LEVEL=debug
```

## Guard a Sensitive File

```bash
# Create a sensitive file
mkdir -p config
echo 'api_key: "sk-secret-123"' > config/secrets.yaml

# Guard it (move to vault, create symlink)
rsenv guard add config/secrets.yaml

# Verify
ls -la config/secrets.yaml
# config/secrets.yaml -> ~/.rsenv/vaults/.../guarded/config/secrets.yaml
```

**Result:** `config/secrets.yaml` is now a symlink. The actual file lives safely in your vault, outside git.

## Interactive Selection

```bash
# Interactively select an environment (uses fuzzy finder)
rsenv env select

# View hierarchy as tree
rsenv env tree
```

## Essential Workflows

### Load environment into shell

```bash
# Build and source in one command
source <(rsenv env build $RSENV_VAULT/envs/local.env)

# Verify
echo $DATABASE_HOST  # localhost
```

### Switch environments

```bash
# Edit dot.envrc to load different env file
# Change: dotenv $RSENV_VAULT/envs/local.env
# To:     dotenv $RSENV_VAULT/envs/prod.env

# Reload with direnv
direnv allow
```

### Encrypt vault for backup

```bash
# First, configure GPG key
rsenv config init --global
# Edit ~/.config/rsenv/rsenv.toml, set sops.gpg_key

# Encrypt all matching files
rsenv sops encrypt

# Check status
rsenv sops status
```

## Resetting

If you need to undo initialization:

```bash
# Restore all guarded files, remove .envrc symlink
rsenv init reset

# Note: Vault directory is NOT deleted (manual cleanup required)
```

## Common Commands

```bash
# Show project and vault status
rsenv info

# Show effective configuration
rsenv config show

# List guarded files
rsenv guard list

# Edit environment files (fuzzy select)
rsenv env edit

# View environment hierarchy
rsenv env tree
```

## Next Steps

Now that you've got the basics:

- **[Core Concepts](Core-Concepts)** - Understand the vault philosophy
- **[Environment Variables](Environment-Variables)** - Master hierarchical environments
- **[Vault Management](Vault-Management)** - Guard more files, manage your vault
- **[SOPS Encryption](SOPS-Encryption)** - Encrypt vault contents
- **[Configuration](Configuration)** - Customize rsenv settings

## Troubleshooting

**direnv not loading .envrc:**
```bash
direnv allow
```

**"Vault not initialized" error:**
```bash
rsenv init
```

**Wrong environment loaded:**
```bash
# Check what's configured
rsenv info

# Verify symlink
ls -la .envrc
```

For more troubleshooting, see [Troubleshooting](Troubleshooting).
