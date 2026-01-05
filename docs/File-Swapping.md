# File Swapping

File swapping lets you temporarily replace project files with development-specific versions stored in your vault. Perfect for local overrides that shouldn't be committed.

## The Swap Concept

Unlike guarding (permanent move to vault), swapping is **temporary**:

```
NORMAL STATE                         AFTER "SWAP IN"
project/                             project/
└── application.yml (original) ───►  └── application.yml (from vault)

vault/swap/                          vault/swap/
└── application.yml (dev version)    └── application.yml.rsenv_original (backup)
```

**Key difference from guarding:**
- **Guard**: Permanent. File lives in vault, symlink in project.
- **Swap**: Temporary. Original backed up, vault version copied to project.

## Commands

### Initialize Swap Files

First, move your development override files to the vault:

```bash
rsenv swap init config/application.yml
```

This:
1. Moves `config/application.yml` from project to `vault/swap/config/application.yml`
2. Leaves the project file in place (it's now the "original")

### Swap In

Replace project files with vault versions:

```bash
rsenv swap in config/application.yml
```

This:
1. Backs up project file: `application.yml` → `application.yml.rsenv_original`
2. Copies vault version to project
3. Creates marker: `application.yml.{hostname}.rsenv_active`

### Swap Out

Restore original files:

```bash
rsenv swap out config/application.yml
```

This:
1. Saves current project file back to vault (preserving your changes)
2. Restores backup: `application.yml.rsenv_original` → `application.yml`
3. Removes active marker

### Check Status

```bash
rsenv swap status
```

Output:
```
Swap status:
  config/application.yml: IN (swapped on myhost)
  config/database.yml: OUT (original in project)
```

### Swap Out All Projects

Batch restore across multiple projects:

```bash
rsenv swap all-out ~/projects
```

Useful before:
- Committing changes
- Switching git branches
- Sharing code with others

### Delete Swap Files

Remove files from swap management entirely:

```bash
rsenv swap delete config/application.yml
```

This:
1. Removes the vault override file (`vault/swap/config/application.yml`)
2. Removes any backup file (`.rsenv_original`)
3. Cleans up all related markers

**Safety**: Refuses to delete if any targeted file is currently swapped in. Swap out first.

**All-or-nothing**: If any file fails validation, no files are deleted.

## Use Cases

### Local Development Overrides

```yaml
# vault/swap/config/application.yml (development version)
server:
  port: 8080
database:
  host: localhost
  name: myapp_dev
logging:
  level: DEBUG
```

```yaml
# project/config/application.yml (production version)
server:
  port: 443
database:
  host: prod-db.internal
  name: myapp
logging:
  level: WARN
```

### Mock Service URLs

```bash
# Create dev version with mock services
rsenv swap init config/services.yml

# Edit vault version
vim $RSENV_VAULT/swap/config/services.yml
# Change: payment_api: https://api.stripe.com
# To:     payment_api: http://localhost:8081/mock
```

### Local Credentials

```bash
# Your local database password
rsenv swap init .env.local

# Swap in when developing
rsenv swap in .env.local

# Swap out before commits
rsenv swap out .env.local
```

## Hostname Tracking

Swap state is tracked per hostname, preventing conflicts when the same vault is accessed from multiple machines (e.g., shared NFS home directories).

### How It Works

When you swap in, rsenv creates:
```
application.yml.myhost.rsenv_active
```

If someone else swaps in from a different host:
```
Error: File already swapped on host 'otherhost'
Use --force to override (may cause data loss)
```

### Force Swap

Override another host's swap (use carefully):

```bash
rsenv swap in --force config/application.yml
```

## RSENV_SWAPPED Marker

rsenv tracks swap state via an environment variable in your vault's `dot.envrc`:

```bash
export RSENV_SWAPPED=1
```

### Behavior

- **swap in**: Adds `RSENV_SWAPPED=1` (idempotent - only added once)
- **swap out**: Removes marker only when ALL files are swapped out

### Use in Scripts

Detect if development overrides are active:

```bash
# In your .envrc or scripts
if [[ -n "$RSENV_SWAPPED" ]]; then
    echo "Warning: Development overrides active"
fi
```

### CI/CD Safety

```bash
# Fail CI if files are swapped
if [[ "$RSENV_SWAPPED" == "1" ]]; then
    echo "Error: Cannot build with swapped files"
    exit 1
fi
```

## .gitignore Handling

When swap files contain `.gitignore` files, rsenv automatically neutralizes them to prevent interference with your vault's git behavior.

### Why This Matters

If you swap a directory containing a `.gitignore`, that file would affect what git sees in your vault - potentially hiding files you want tracked.

### Automatic Behavior

| Operation | .gitignore in vault becomes |
|-----------|----------------------------|
| `swap init` | `.gitignore.rsenv-disabled` |
| `swap out` | `.gitignore.rsenv-disabled` |
| `swap in` | `.gitignore` (restored) |

### Safety Check

`swap in` refuses if a bare `.gitignore` already exists in the vault location. This prevents accidental overwrites. Rename or delete the conflicting file first.

## Workflow Example

### Development Session

```bash
# Start work
cd ~/myproject
rsenv swap in config/application.yml

# Develop with local config...

# Before commit
rsenv swap out config/application.yml
git add -A
git commit -m "Feature complete"
```

### CI/CD Safety

In your CI pipeline, ensure nothing is swapped:

```bash
# In CI script
rsenv swap status --quiet || exit 1
# Exits with error if any files are swapped in
```

### Branch Switching

```bash
# Before switching branches
rsenv swap all-out

git checkout feature-branch

# After switching, swap back in if needed
rsenv swap in config/application.yml
```

## File Locations

### Vault Structure

```
vault/swap/
└── config/
    └── application.yml    # Your development version
```

### Project Markers (when swapped in)

```
project/config/
├── application.yml                      # Currently: vault's version
├── application.yml.rsenv_original       # Backup of original
└── application.yml.myhost.rsenv_active  # Marker file
```

## Best Practices

### Swap Out Before Commits

Never commit swapped-in files. Add a pre-commit hook:

```bash
#!/bin/bash
# .git/hooks/pre-commit
if rsenv swap status --quiet 2>/dev/null; then
    echo "Error: Files are swapped in. Run 'rsenv swap out' first."
    exit 1
fi
```

### Use swap all-out Liberally

```bash
# Add alias
alias swapout='rsenv swap all-out ~'

# Before any commit
swapout && git commit
```

### Document Swap Files

Create a README in your swap directory:

```bash
cat > $RSENV_VAULT/swap/README.md << 'EOF'
# Swap Files

## config/application.yml
Local development config:
- Uses localhost database
- Debug logging enabled
- Mock payment API

## .env.local
Local credentials (not in git)
EOF
```

### Keep Swap Files Updated

When the original changes, update your swap version:

```bash
# See what changed
diff project/config/application.yml $RSENV_VAULT/swap/config/application.yml

# Update swap file
vim $RSENV_VAULT/swap/config/application.yml
```

## Troubleshooting

### "File already swapped on host X"

```bash
# Check status
rsenv swap status

# Force if you're sure
rsenv swap in --force file.yml
```

### Lost original file

The original is backed up with `.rsenv_original` suffix:

```bash
# Find backup
ls -la *.rsenv_original

# Manual restore
mv file.yml.rsenv_original file.yml
rm file.yml.*.rsenv_active
```

### Swap file not in vault

```bash
# Initialize it first
rsenv swap init path/to/file.yml
```

### Multiple swapped files

```bash
# Swap out all at once
rsenv swap out file1.yml file2.yml file3.yml

# Or use all-out
rsenv swap all-out
```

## Guard vs Swap

| Aspect | Guard | Swap |
|--------|-------|------|
| **Purpose** | Protect secrets | Development overrides |
| **Duration** | Permanent | Temporary |
| **Mechanism** | Symlink | File copy |
| **Original location** | Vault (always) | Project (when swapped out) |
| **Git sees** | Symlink | Real file |
| **Use case** | API keys, certs | Local configs, mock URLs |

## Related

- **[Core Concepts](Core-Concepts)** - Understanding vault categories
- **[Vault Management](Vault-Management)** - File guarding (permanent)
- **[Configuration](Configuration)** - rsenv settings
