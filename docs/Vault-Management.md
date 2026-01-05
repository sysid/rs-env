# Vault Management

The vault is the heart of rsenv - a project-associated directory that stores sensitive files, environment configurations, and development overrides outside your git repository.

## Understanding the Vault

### What's in a Vault?

```
~/.rsenv/vaults/myproject-a1b2c3d4/
├── dot.envrc           # The "real" .envrc (your project symlinks to this)
├── envs/               # Environment hierarchy files
│   ├── local.env
│   ├── test.env
│   ├── int.env
│   └── prod.env
├── guarded/            # Sensitive files moved from project
│   └── config/
│       └── secrets.yaml
└── swap/               # Development override files
    └── application.yml
```

### The Bidirectional Link

```
PROJECT                              VAULT
.envrc ──────── symlink ───────────► dot.envrc
   │                                    │
   │                                    ├── export RSENV_VAULT=...
   │                                    │
   └── RSENV_VAULT points to ──────────►│
```

- **Project → Vault**: `.envrc` is a symlink to vault's `dot.envrc`
- **Vault → Project**: `RSENV_VAULT` variable provides path back

## Initialization

### Initialize a Project

```bash
cd /path/to/myproject
rsenv init
```

**What this does:**
1. Creates vault directory: `~/.rsenv/vaults/myproject-{id}/`
2. Creates subdirectories: `envs/`, `guarded/`, `swap/`
3. Moves existing `.envrc` to vault as `dot.envrc` (preserving content)
4. Creates default env files: `local.env`, `test.env`, `int.env`, `prod.env`
5. Injects rsenv section into `dot.envrc`
6. Creates symlink: `.envrc` → `dot.envrc`

### Initialization Options

```bash
# Use absolute paths for symlinks (default: relative)
rsenv init --absolute

# Initialize specific directory
rsenv init /path/to/project
```

### Check Status

```bash
rsenv info
```

Output:
```
Project: /home/user/myproject
Vault: /home/user/.rsenv/vaults/myproject-a1b2c3d4
  Sentinel ID: myproject-a1b2c3d4
  Guarded files: 2
  Swap files: 1
```

### Reset (Undo Initialization)

```bash
rsenv init reset
```

**What this does:**
1. Restores all guarded files from vault back to project
2. Removes `.envrc` symlink
3. Restores original `.envrc` content (removes rsenv section)

**Note:** The vault directory is NOT deleted. Remove manually if needed:
```bash
rm -rf ~/.rsenv/vaults/myproject-a1b2c3d4
```

## File Guarding

Guarding moves sensitive files to the vault and creates symlinks in their place.

### Guard a File

```bash
rsenv guard add path/to/sensitive/file.yaml
```

**Before:**
```
project/
└── config/
    └── secrets.yaml   # actual file with secrets
```

**After:**
```
project/                              vault/guarded/
└── config/                           └── config/
    └── secrets.yaml → symlink ──────────► secrets.yaml
```

### Guard Options

```bash
# Use absolute symlink paths
rsenv guard add --absolute secrets.yaml

# Guard from project root with -C
rsenv -C /path/to/project guard add config/secrets.yaml
```

### List Guarded Files

```bash
rsenv guard list
```

Output:
```
Guarded files:
  config/secrets.yaml → vault/guarded/config/secrets.yaml
  .credentials/api-key → vault/guarded/.credentials/api-key
```

### Restore (Unguard) a File

```bash
rsenv guard restore config/secrets.yaml
```

This moves the file back from vault to project, removing the symlink.

### What Can Be Guarded?

Any file within your project directory:
- Configuration files (`secrets.yaml`, `credentials.json`)
- Certificate files (`.pem`, `.key`, `.p12`)
- Database configs
- API keys and tokens
- Anything you don't want in git

**Note:** You cannot guard files outside the project directory.

## The dot.envrc File

The `dot.envrc` in your vault contains:

```bash
# Your original .envrc content preserved here
export EXISTING_VAR=value

#------------------------------- rsenv start --------------------------------
# config.relative = true
# config.version = 2
# state.sentinel = 'myproject-a1b2c3d4'
# state.timestamp = '2024-01-15T10:30:00Z'
# state.sourceDir = '$HOME/myproject'
export RSENV_VAULT=$HOME/.rsenv/vaults/myproject-a1b2c3d4
#dotenv $RSENV_VAULT/envs/local.env
#-------------------------------- rsenv end ---------------------------------
```

**Key elements:**
- Original content is preserved above the rsenv section
- `RSENV_VAULT` provides path to vault
- Commented `dotenv` line for loading environments
- Metadata comments for rsenv internal use

### Customizing dot.envrc

You can add your own content above or below the rsenv section:

```bash
# Your custom setup
export PATH="$HOME/bin:$PATH"
source ~/.secrets

#------------------------------- rsenv start --------------------------------
# ... rsenv managed section ...
#-------------------------------- rsenv end ---------------------------------

# Your custom teardown
echo "Environment loaded"
```

## Vault Location

### Default Location

```
~/.rsenv/vaults/{project-name}-{short-id}/
```

### Custom Location

Set in configuration:

```toml
# ~/.config/rsenv/rsenv.toml
vault_base_dir = "~/my-vaults"
```

Or via environment variable:

```bash
export RSENV_VAULT_BASE_DIR=~/my-vaults
```

### Finding Your Vault

```bash
# From within project
echo $RSENV_VAULT

# Or
rsenv info
```

## Working with Multiple Projects

Each project has its own vault:

```
~/.rsenv/vaults/
├── project-a-12345678/
├── project-b-87654321/
└── project-c-abcdef01/
```

### Listing All Vaults

```bash
ls ~/.rsenv/vaults/
```

### Cleaning Up Old Vaults

Vaults are not automatically deleted when you `rsenv init reset`. Manual cleanup:

```bash
# List vaults
ls -la ~/.rsenv/vaults/

# Remove specific vault (after resetting project)
rm -rf ~/.rsenv/vaults/old-project-12345678
```

## Best Practices

### Guard Early

Guard sensitive files immediately when creating a project:

```bash
rsenv init
rsenv guard add .credentials/api-key
rsenv guard add config/secrets.yaml
```

### Keep Vault Backed Up

The vault contains your sensitive files. Back it up securely:

```bash
# Encrypt first
rsenv sops encrypt

# Then backup the vault directory
```

### Use Relative Paths

Relative symlinks (default) work better if you move your home directory or use different machines:

```bash
# Default behavior - relative
rsenv init
rsenv guard add secrets.yaml

# Explicit relative
rsenv init  # default is relative
```

### Document Your Vault Structure

Create a README in your vault:

```bash
cat > $RSENV_VAULT/README.md << 'EOF'
# Vault for myproject

## Environment Hierarchy
- base.env: Shared settings
- local.env: Local development (inherits base)
- prod.env: Production (inherits base)

## Guarded Files
- config/secrets.yaml: API keys and credentials
- .credentials/db-password: Database password

## Encryption
Encrypted with GPG key: ABC123...
EOF
```

## Troubleshooting

### "Vault not initialized"

```bash
# Initialize the vault
rsenv init
```

### Broken symlink

```bash
# Check if vault exists
ls -la $RSENV_VAULT

# Reinitialize if needed
rsenv init
```

### Wrong vault path

```bash
# Check current vault
rsenv info

# Verify symlink target
ls -la .envrc
```

### Files not guarding

```bash
# File must be within project directory
# File must not already be a symlink

# Check file status
ls -la path/to/file

# Check project root detection
rsenv info
```

## Related

- **[Core Concepts](Core-Concepts)** - Understanding vaults conceptually
- **[Environment Variables](Environment-Variables)** - Using the envs/ directory
- **[SOPS Encryption](SOPS-Encryption)** - Encrypting vault contents
- **[File Swapping](File-Swapping)** - Using the swap/ directory
