# Configuration

rsenv uses layered configuration with sensible defaults. Customize behavior through config files or environment variables.

## Configuration Precedence

Configuration is loaded in order (later overrides earlier):

1. **Compiled defaults** - Built into rsenv
2. **Global config** - `~/.config/rsenv/rsenv.toml`
3. **Local config** - `<project>/.rsenv.toml`
4. **Environment variables** - `RSENV_*` prefix

## Quick Setup

```bash
# Create global config
rsenv config init --global

# View effective config
rsenv config show

# Show config file paths
rsenv config path
```

## Configuration File

### Location

| Type | Path |
|------|------|
| Global | `~/.config/rsenv/rsenv.toml` |
| Local | `<project>/.rsenv.toml` |

### Full Example

```toml
# ~/.config/rsenv/rsenv.toml

# Base directory for vaults
vault_base_dir = "~/.rsenv/vaults"

# Preferred editor
editor = "nvim"

# SOPS encryption settings
[sops]
# GPG key fingerprint
gpg_key = "60A4127E82E218297532FAB6D750B66AE08F3B90"

# Age public key (alternative to GPG)
# age_key = "age1..."

# File extensions to encrypt
file_extensions_enc = [
    "env",
    "envrc",
    "yaml",
    "yml",
    "json",
]

# Specific filenames to encrypt
file_names_enc = [
    "dot_pypirc",
    "dot_pgpass",
    "kube_config",
]

# Extensions to decrypt
file_extensions_dec = ["enc"]

# Specific filenames to decrypt
file_names_dec = []
```

## Configuration Options

### Core Settings

| Setting | Default | Description |
|---------|---------|-------------|
| `vault_base_dir` | `~/.rsenv/vaults` | Where vaults are stored |
| `editor` | `$EDITOR` or `vim` | Editor for `rsenv env edit` |

### SOPS Settings

| Setting | Default | Description |
|---------|---------|-------------|
| `sops.gpg_key` | None | GPG key fingerprint for encryption |
| `sops.age_key` | None | Age public key (alternative to GPG) |
| `sops.file_extensions_enc` | `["env", "envrc"]` | Extensions to encrypt |
| `sops.file_names_enc` | `[]` | Exact filenames to encrypt |
| `sops.file_extensions_dec` | `["enc"]` | Extensions to decrypt |
| `sops.file_names_dec` | `[]` | Exact filenames to decrypt |

## Environment Variables

Override any setting via environment variables with `RSENV_` prefix:

| Config Key | Environment Variable |
|------------|---------------------|
| `vault_base_dir` | `RSENV_VAULT_BASE_DIR` |
| `editor` | `RSENV_EDITOR` |
| `sops.gpg_key` | `RSENV_SOPS_GPG_KEY` |
| `sops.age_key` | `RSENV_SOPS_AGE_KEY` |
| `sops.file_extensions_enc` | `RSENV_SOPS_FILE_EXTENSIONS_ENC` |
| `sops.file_names_enc` | `RSENV_SOPS_FILE_NAMES_ENC` |

### Array Values

For arrays, use comma-separated values:

```bash
export RSENV_SOPS_FILE_EXTENSIONS_ENC="env,yaml,json"
export RSENV_SOPS_FILE_NAMES_ENC="dot_pypirc,dot_pgpass"
```

### Examples

```bash
# Use custom vault location
export RSENV_VAULT_BASE_DIR=~/my-vaults

# Use different GPG key
export RSENV_SOPS_GPG_KEY="ABC123..."

# Add to shell config for persistence
echo 'export RSENV_VAULT_BASE_DIR=~/my-vaults' >> ~/.bashrc
```

## Project-Specific Config

Create `.rsenv.toml` in your project root to override global settings:

```toml
# myproject/.rsenv.toml

# This project uses a specific GPG key
[sops]
gpg_key = "PROJECT_SPECIFIC_KEY"
```

Project config merges with global config - only specified values are overridden.

## Commands

### Show Effective Configuration

```bash
rsenv config show
```

Output:
```
Configuration:
  vault_base_dir: /home/user/.rsenv/vaults
  editor: nvim

  [sops]
  gpg_key: 60A4127E82E218297532FAB6D750B66AE08F3B90
  file_extensions_enc: ["env", "envrc", "yaml"]
  file_names_enc: ["dot_pypirc"]
  file_extensions_dec: ["enc"]
```

### Show Config Paths

```bash
rsenv config path
```

Output:
```
Configuration paths:
  Global: /home/user/.config/rsenv/rsenv.toml (exists)
  Local: /home/user/myproject/.rsenv.toml (not found)
```

### Initialize Config

```bash
# Create global config template
rsenv config init --global

# Create local project config
rsenv config init
```

## Defaults

If no configuration exists, rsenv uses these defaults:

```toml
vault_base_dir = "~/.rsenv/vaults"
editor = "$EDITOR"  # Falls back to "vim"

[sops]
gpg_key = ""  # Must be set for encryption
age_key = ""
file_extensions_enc = ["env", "envrc"]
file_names_enc = []
file_extensions_dec = ["enc"]
file_names_dec = []
```

## Path Expansion

Paths support:
- `~` - Home directory
- `$VAR` and `${VAR}` - Environment variables

```toml
vault_base_dir = "~/my-vaults"
vault_base_dir = "$HOME/my-vaults"
vault_base_dir = "${XDG_DATA_HOME}/rsenv/vaults"
```

## XDG Compliance

rsenv follows XDG Base Directory spec:

| Purpose | Default Location |
|---------|------------------|
| Config | `~/.config/rsenv/rsenv.toml` |
| Data (vaults) | `~/.rsenv/vaults` |

Set `XDG_CONFIG_HOME` to change config location:

```bash
export XDG_CONFIG_HOME=~/.myconfig
# Config now at ~/.myconfig/rsenv/rsenv.toml
```

## Per-Vault Configuration

Vaults can contain a local `rsenv.toml`:

```
~/.rsenv/vaults/myproject-abc123/
├── rsenv.toml     # Vault-specific overrides
├── dot.envrc
└── envs/
```

This is loaded when operating within that vault.

## Troubleshooting

### Config not loading

```bash
# Check paths
rsenv config path

# Verify file syntax
cat ~/.config/rsenv/rsenv.toml

# Check effective values
rsenv config show
```

### Environment variable not applied

```bash
# Verify export
echo $RSENV_VAULT_BASE_DIR

# Check if it's in the effective config
rsenv config show | grep vault_base_dir
```

### Local config not found

```bash
# Must be in project root, not subdirectory
pwd
ls -la .rsenv.toml
```

## Related

- **[Installation](Installation)** - Initial setup
- **[SOPS Encryption](SOPS-Encryption)** - SOPS-specific configuration
- **[Troubleshooting](Troubleshooting)** - Common issues
