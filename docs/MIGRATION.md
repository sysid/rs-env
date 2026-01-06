# Migration Guide: rs-env (v1) to rsenv (v2)

Migrating from rs-env v1 to rsenv v2 takes about 5 minutes. Your environment files work unchanged.

## Quick Migration

```bash
# 1. Install v2
cargo install rsenv

# 2. Uninstall v1 (optional)
cargo uninstall rs-env

# 3. Update shell aliases (see below)

# 4. Verify
rsenv --version
```

## Command Mapping

All v1 commands moved under the `env` subcommand:

| v1 (rs-env) | v2 (rsenv) |
|-------------|------------|
| `rsenv build <file>` | `rsenv env build <file>` |
| `rsenv envrc <file>` | `rsenv env envrc <file>` |
| `rsenv files <file>` | `rsenv env files <file>` |
| `rsenv select [dir]` | `rsenv env select [dir]` |
| `rsenv tree [dir]` | `rsenv env tree [dir]` |
| `rsenv branches [dir]` | `rsenv env branches [dir]` |
| `rsenv edit [dir]` | `rsenv env edit [dir]` |
| `rsenv edit-leaf <file>` | `rsenv env edit-leaf <file>` |
| `rsenv tree-edit [dir]` | `rsenv env tree-edit [dir]` |
| `rsenv leaves [dir]` | `rsenv env leaves [dir]` |
| `rsenv link <files>` | `rsenv env link <files>` |
| `rsenv unlink <file>` | `rsenv env unlink <file>` |

## Shell Alias Updates

If you have aliases, update them:

```bash
# Before (v1)
alias envbuild='rsenv build'
alias envselect='rsenv select'
alias envtree='rsenv tree'

# After (v2)
alias envbuild='rsenv env build'
alias envselect='rsenv env select'
alias envtree='rsenv env tree'
```

## File Format: No Changes

The `# rsenv:` directive is **fully backward compatible**:

```bash
# rsenv: parent.env
export MY_VAR=value
```

Your existing `.env` files work without modification.

## What's New in v2

v2 adds three major capabilities beyond environment management:

### Vault System

Unified secure storage outside your project:

```bash
rsenv init              # Create vault for project
rsenv info              # Show vault status
```

### File Guarding

Move sensitive files to vault, leave symlinks:

```bash
rsenv guard add secrets.yaml
rsenv guard list
rsenv guard restore secrets.yaml
```

### File Swapping

Temporary development overrides:

```bash
rsenv swap init config.yml    # Move to vault
rsenv swap in config.yml      # Use vault version
rsenv swap out config.yml     # Restore original
rsenv swap status
```

### SOPS Encryption

Encrypt vault contents:

```bash
rsenv sops encrypt
rsenv sops decrypt
rsenv sops status
```

### Configuration

Unified config at `~/.config/rsenv/rsenv.toml`:

```bash
rsenv config init --global
rsenv config show
rsenv config path
```

## Environment Variables

New variables in v2:

| Variable | Purpose |
|----------|---------|
| `RSENV_VAULT` | Path to current project's vault |
| `RSENV_SWAPPED` | Set to `1` when files are swapped in |
| `RSENV_VAULT_BASE_DIR` | Override vault base directory |

## Gradual Adoption

You can adopt v2 features incrementally:

1. **Day 1**: Use `rsenv env` commands (drop-in replacement)
2. **Later**: Initialize vault with `rsenv init`
3. **As needed**: Guard sensitive files, set up swapping

The vault features are optional. You can use rsenv purely for environment hierarchy management, just like v1.

## Troubleshooting

### "command not found: rsenv"

```bash
# Check installation
which rsenv
cargo install rsenv
```

### Old aliases still use v1 syntax

```bash
# Find aliases
grep -r "rsenv build\|rsenv select\|rsenv tree" ~/.bashrc ~/.zshrc ~/.config/fish

# Update to v2 syntax
```

### v1 and v2 both installed

```bash
# Remove v1
cargo uninstall rs-env

# Verify only v2
rsenv --version
```

## See Also

- [Quick Start](Quick-Start) - Get started with v2
- [Core Concepts](Core-Concepts) - Understand the vault philosophy
- [Command Reference](Command-Reference) - Complete command documentation
