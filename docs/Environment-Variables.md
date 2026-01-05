# Environment Variables

rsenv's environment hierarchy system eliminates duplication by letting child configurations inherit from parents.

## The Hierarchy Concept

Environment files form **directed acyclic graphs (DAGs)** where:
1. Child files link to parents via `# rsenv: parent.env` comments
2. Variables are inherited and merged from parent to child
3. Child values override parent values
4. Multiple parents are supported for complex setups

```
base.env                    # Shared settings
   ├── local.env           # Local dev (inherits base)
   ├── test.env            # Testing (inherits base)
   └── cloud.env           # Cloud base (inherits base)
          ├── staging.env  # Staging (inherits cloud)
          └── prod.env     # Production (inherits cloud)
```

## File Format

### Parent Link Syntax

Use `# rsenv:` to specify parent files:

```bash
# Single parent
# rsenv: base.env

# Multiple parents (space-separated)
# rsenv: base.env shared.env

# Path with variables
# rsenv: $RSENV_VAULT/envs/base.env
```

### Variable Syntax

rsenv parses `export VAR=value` lines:

```bash
# Standard export
export DATABASE_HOST=localhost

# With quotes
export MESSAGE="Hello World"
export PATH_VAR='$HOME/bin'

# Comments are preserved
# This is a comment
export DEBUG=true  # inline comments work too
```

## Commands

### Build Environment

Compile a complete environment from a hierarchy:

```bash
rsenv env build path/to/leaf.env
```

This traverses all parents (breadth-first), merging variables. Child values override parents.

**Example:**
```bash
# base.env
export DATABASE_PORT=5432
export LOG_LEVEL=info

# local.env
# rsenv: base.env
export DATABASE_HOST=localhost
export LOG_LEVEL=debug

# Build
rsenv env build local.env
# Output:
# export DATABASE_PORT=5432    <- from base
# export DATABASE_HOST=localhost
# export LOG_LEVEL=debug       <- overridden by local
```

### View Hierarchy

```bash
# Show tree structure
rsenv env tree [directory]

# Show all branches (linear)
rsenv env branches [directory]

# List leaf files only
rsenv env leaves [directory]

# List all files in a hierarchy
rsenv env files leaf.env
```

### Interactive Selection

```bash
# Fuzzy-select and update .envrc
rsenv env select [directory]

# Fuzzy-select and edit
rsenv env edit [directory]
```

### Create Links

```bash
# Link parent to child
rsenv env link parent.env child.env

# Link chain: root <- middle <- leaf
rsenv env link root.env middle.env leaf.env

# Remove parent link
rsenv env unlink file.env
```

### Write to .envrc

```bash
# Generate .envrc content from hierarchy
rsenv env envrc leaf.env

# Write to specific file
rsenv env envrc leaf.env --envrc /path/to/.envrc
```

## Variable Resolution

### Override Order

When the same variable appears in multiple files, the **last definition wins**:

```bash
# base.env
export LOG_LEVEL=info

# middle.env
# rsenv: base.env
export LOG_LEVEL=warn

# leaf.env
# rsenv: middle.env
export LOG_LEVEL=debug

# rsenv env build leaf.env
# LOG_LEVEL=debug (leaf wins)
```

### Multiple Parents

With multiple parents, they're processed left-to-right:

```bash
# leaf.env
# rsenv: base.env overrides.env

# Processing order:
# 1. base.env (and its parents)
# 2. overrides.env (and its parents)
# 3. leaf.env itself
```

### Path Expansion

Paths in `# rsenv:` directives support:
- `$VAR` and `${VAR}` - environment variables
- `~` - home directory

```bash
# rsenv: $RSENV_VAULT/envs/base.env
# rsenv: ~/configs/shared.env
```

## direnv Integration

### Automatic Loading

Configure your vault's `dot.envrc` to load environments:

```bash
# In vault's dot.envrc (the file .envrc symlinks to)
#------------------------------- rsenv start -------------------------------
export RSENV_VAULT=$HOME/.rsenv/vaults/myproject-abc123
#-------------------------------- rsenv end --------------------------------

# Load environment hierarchy
eval "$(rsenv env build $RSENV_VAULT/envs/${RUN_ENV:-local}.env)"
```

### Switching Environments

Set `RUN_ENV` to switch:

```bash
# Use local environment
export RUN_ENV=local

# Use production environment
export RUN_ENV=prod

# Reload
direnv allow
```

### Selection Helper

Use interactive selection to update `.envrc`:

```bash
rsenv env select
# 1. Shows fuzzy finder with available environments
# 2. Updates .envrc with selection
# 3. Triggers direnv reload
```

## Best Practices

### Organize by Purpose

```
vault/envs/
├── base.env           # Truly shared settings
├── local.env          # Local development
├── test.env           # Test environment
├── ci.env             # CI/CD pipelines
└── cloud/
    ├── base.env       # Cloud-specific base
    ├── staging.env    # Staging
    └── prod.env       # Production
```

### Keep Hierarchies Shallow

Deep hierarchies (5+ levels) become hard to debug. Prefer:
- 2-3 levels for most projects
- Use multiple parents instead of deep chains

### Document Inheritance

```bash
# local.env
# Inherits: base.env → local.env
# Purpose: Local development with debug logging
# rsenv: base.env

export LOG_LEVEL=debug
```

### Use Explicit Paths

For clarity, use `$RSENV_VAULT` in directives:

```bash
# rsenv: $RSENV_VAULT/envs/base.env
```

## Troubleshooting

### "File not found" errors

```bash
# Check if parent exists
ls -la $(dirname leaf.env)/parent.env

# Check path expansion
echo $RSENV_VAULT
```

### Variables not overriding

Build order is breadth-first, then left-to-right for parents. Verify hierarchy:

```bash
rsenv env tree
rsenv env files leaf.env  # Shows processing order
```

### Circular dependencies

rsenv detects cycles and errors. Check your `# rsenv:` directives for loops.

## Related

- **[Core Concepts](Core-Concepts)** - Understanding the vault model
- **[Quick Start](Quick-Start)** - Basic setup
- **[Vault Management](Vault-Management)** - Managing vault structure
