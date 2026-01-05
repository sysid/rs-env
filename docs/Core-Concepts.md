# Core Concepts

Understanding these core concepts will help you use rsenv effectively and appreciate why it works the way it does.

## The Three Problems

Development projects face three distinct but related challenges with sensitive configuration:

### 1. Environment Variable Chaos

Different environments (local, test, staging, production) need different values. Traditional `.env` files lead to:
- Massive duplication across `local.env`, `test.env`, `prod.env`
- Forgotten updates when shared settings change
- No visibility into what differs between environments

### 2. Secrets in Dangerous Places

Sensitive files (credentials, certificates, API keys) live in your project directory:
- One careless `git add .` exposes secrets forever
- Developers constantly worry about accidental commits
- No central place to encrypt or manage sensitive files

### 3. Development Override Friction

Development often needs temporary configuration overrides:
- Swap in a mock service URL during testing
- Use local database credentials instead of shared ones
- Risk forgetting to swap back before committing

## The Vault: A Unified Solution

rsenv introduces the **Vault** - a project-associated secure storage directory that lives *outside* your project but is *bidirectionally linked* to it.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              The Vault Concept                              │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  YOUR PROJECT                          ~/.rsenv/vaults/myproject-a1b2c3d4  │
│  (git-tracked)                         (vault - not in git)                │
│                                                                             │
│  .envrc ────────── symlink ──────────► dot.envrc                           │
│     │                                     │                                │
│     │                                     ├── # rsenv section              │
│     │                                     │   export RSENV_VAULT=...       │
│     │                                     │                                │
│     │                                     └── (your original .envrc)       │
│     │                                                                      │
│     └── RSENV_VAULT points back ───────► [vault root]                      │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Bidirectional Linking

The power of rsenv is the **bidirectional link** between project and vault:

| Direction | Mechanism | Purpose |
|-----------|-----------|---------|
| Project → Vault | `.envrc` symlink points to `dot.envrc` | Loads vault configuration automatically |
| Vault → Project | `RSENV_VAULT` variable in `.envrc` | Scripts and tools can find the vault |

This two-way connection means:
- Your project automatically loads vault configuration
- Your tools always know where the vault is
- No manual path management required

### Minimal Project Pollution

rsenv is designed for **minimal interference** with your project:

| What Changes | What Doesn't |
|--------------|--------------|
| `.envrc` becomes a symlink | No new files created |
| Guarded files become symlinks | Project structure unchanged |
| | No rsenv config files in project |
| | No hidden directories added |
| | gitignore untouched |

**The only visible change**: symlinks replace actual files. Your project structure, your git history, your `.gitignore` - all remain untouched.

## Three Capabilities, One Tool

rsenv unifies three previously separate tools into one coherent system:

### 1. Environment Hierarchy

Build complex environment configurations from simple, inheritable pieces:

```bash
# base.env - shared across all environments
export DATABASE_PORT=5432
export LOG_FORMAT=json

# local.env - inherits from base
# rsenv: base.env
export DATABASE_HOST=localhost
export LOG_LEVEL=debug

# prod.env - also inherits from base
# rsenv: base.env
export DATABASE_HOST=prod-db.internal
export LOG_LEVEL=warn
```

**Key insight**: The `# rsenv: base.env` comment creates parent-child inheritance. Variables flow down; children override parents.

### 2. File Guarding

Move sensitive files to the vault, leaving symlinks in their place:

```
project/                              vault/guarded/
├── config/                           └── config/
│   └── secrets.yaml → symlink ─────────► secrets.yaml (actual file)
└── certs/
    └── private.key → symlink ─────────► certs/private.key
```

**Guard** a file: move it to vault, create symlink
**Unguard** a file: move it back, remove symlink

The project structure stays identical. Git sees symlinks, not secrets.

### 3. File Swapping

Temporarily replace project files with vault versions for development:

```
Normal state:                         After "swap in":
project/application.yml (original) → project/application.yml (from vault)
vault/swap/application.yml (dev)     vault/swap/application.yml.original (backup)
```

**Swap in**: Replace project file with vault's development version
**Swap out**: Restore original, save changes to vault

Swap state is tracked per-hostname, preventing conflicts on shared directories.

## The Vault Structure

When you initialize a project, rsenv creates:

```
~/.rsenv/vaults/{project-name}-{short-id}/
├── dot.envrc           # The "real" .envrc (symlinked from project)
├── envs/               # Environment hierarchy files
│   ├── local.env       # export RUN_ENV="local"
│   ├── test.env        # export RUN_ENV="test"
│   ├── int.env         # export RUN_ENV="int"
│   └── prod.env        # export RUN_ENV="prod"
├── guarded/            # Sensitive files moved from project
│   └── (mirrors project structure)
└── swap/               # Development override files
    └── (mirrors project structure)
```

## How It All Fits Together

Here's a typical workflow:

```bash
# 1. Initialize vault for your project
cd ~/myproject
rsenv init

# 2. Set up environment hierarchy in vault
#    (edit vault/envs/local.env, create additional .env files)

# 3. Guard sensitive files
rsenv guard add config/secrets.yaml
rsenv guard add .credentials/api-key

# 4. Build environment for current context
rsenv env build $RSENV_VAULT/envs/local.env

# 5. Encrypt vault for backup/sharing
rsenv sops encrypt
```

## Why This Design?

### Single Point of Truth

Each project has exactly one vault. All sensitive configuration, environment variables, and development overrides live there. No scattered files, no confusion.

### Defense in Depth

Multiple layers protect your secrets:
1. **Vault location**: Outside project directory, outside git
2. **Symlinks**: Git commits harmless symlinks, not secrets
3. **SOPS encryption**: Vault contents can be encrypted at rest
4. **gitignore automation**: SOPS commands auto-update `.gitignore`

### Reversibility

Every rsenv operation is reversible:
- `rsenv init` → `rsenv init reset`
- `rsenv guard add` → `rsenv guard restore`
- `rsenv swap in` → `rsenv swap out`

You can always return to your original project state.

## Next Steps

- **[Quick Start](Quick-Start)** - Get productive in 5 minutes
- **[Environment Variables](Environment-Variables)** - Master hierarchical environments
- **[Vault Management](Vault-Management)** - Guard and manage sensitive files
- **[File Swapping](File-Swapping)** - Development overrides
