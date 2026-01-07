# Release Notes: rsenv 2.0.0-alpha.1 - Environments Evolved

**Release Date**: 2026-01-06

This is a major release that transforms rsenv from a single-purpose environment variable manager into a comprehensive development environment management tool.

## Breaking Changes

### Command Structure Reorganized

All v1 environment commands now live under the `env` subcommand:

| v1.5.0 | v2.0.0-alpha.1 |
|--------|----------------|
| `rsenv build <file>` | `rsenv env build <file>` |
| `rsenv select [dir]` | `rsenv env select [dir]` |
| `rsenv tree [dir]` | `rsenv env tree [dir]` |
| `rsenv branches [dir]` | `rsenv env branches [dir]` |
| `rsenv edit [dir]` | `rsenv env edit [dir]` |
| `rsenv edit-leaf <file>` | `rsenv env edit-leaf <file>` |
| `rsenv tree-edit [dir]` | `rsenv env tree-edit [dir]` |
| `rsenv leaves [dir]` | `rsenv env leaves [dir]` |
| `rsenv link <files>` | `rsenv env link <files>` |
| `rsenv unlink <file>` | `rsenv env unlink <file>` |
| `rsenv envrc <file>` | `rsenv env envrc <file>` |
| `rsenv files <file>` | `rsenv env files <file>` |

## New Features

### Vault System

Unified secure storage that lives *outside* your project but is *bidirectionally linked* to it.

```bash
rsenv init                    # Create vault for current project
rsenv init reset              # Undo initialization
rsenv info                    # Show project and vault status
```

### File Guarding

Move sensitive files to the vault, leaving symlinks in their place. Files are protected from accidental commits while remaining functional.

```bash
rsenv guard add .env          # Move to vault, create symlink
rsenv guard add --absolute .env  # Use absolute symlink paths
rsenv guard list              # Show guarded files
rsenv guard restore .env      # Restore file from vault
```

### File Swapping

Temporary development overrides without polluting your project or risking production configs.

```bash
rsenv swap init config.yaml   # Initialize: move file to vault
rsenv swap in config.yaml     # Swap in development version
rsenv swap out config.yaml    # Restore original
rsenv swap status             # Show swap status
rsenv swap all-out            # Restore all files across projects
rsenv swap delete config.yaml # Remove from swap management
```

### SOPS Encryption

Encrypt vault contents for secure backup and sharing using GPG or Age keys.

```bash
rsenv sops encrypt            # Encrypt matching files
rsenv sops decrypt            # Decrypt .enc files
rsenv sops clean              # Delete unencrypted originals
rsenv sops status             # Show encryption status
```

### Configuration Management

Layered configuration with sensible defaults.

```bash
rsenv config init             # Create local config (in vault)
rsenv config init --global    # Create global config
rsenv config show             # Show effective merged config
rsenv config path             # Show config file paths
```

**Config precedence**: Defaults < Global (`~/.config/rsenv/rsenv.toml`) < Local (`<vault>/.rsenv.toml`) < Environment variables (`RSENV_*`)

### New Environment Variables

| Variable | Purpose |
|----------|---------|
| `RSENV_VAULT` | Path to current project's vault |
| `RSENV_SWAPPED` | Set to `1` when files are swapped in |
| `RSENV_VAULT_BASE_DIR` | Override vault base directory |

### Global Project Directory Option

```bash
rsenv -C /path/to/project <command>   # Run in different project
```

## Backward Compatibility

### File Format Unchanged

The `# rsenv:` directive works exactly as before:

```bash
# rsenv: parent.env
export MY_VAR=value
```

Your existing `.env` files require no modification.

### Gradual Adoption

Vault features are optional. Use rsenv purely for environment hierarchy management by sticking to `rsenv env` commands.

### Terminal Coloring

Semantic color output for improved readability:

| Element | Color |
|---------|-------|
| Errors | Red (bold) |
| Warnings | Yellow |
| Success (✓) | Green |
| Failures (✗) | Red |
| Actions | Green |
| Headers | Cyan (bold) |
| Diff additions (+) | Green |
| Diff removals (-) | Red |

Respects `NO_COLOR`, `CLICOLOR`, and `CLICOLOR_FORCE` environment variables per [no-color.org](https://no-color.org) standard.

## Internal Improvements

- **Architecture**: Layered error handling (DomainError -> ApplicationError -> InfraError -> CliError) with `thiserror`
- **Testability**: Traits at I/O boundaries (`FileSystem`, `CommandRunner`, `Clipboard`)
- **CLI Output**: Centralized output module with semantic coloring functions
- **Documentation**: Comprehensive wiki-style documentation under `docs/`
- **Development**: Pre-commit hooks for `cargo fmt`, `cargo clippy`, `cargo check`

## Migration

See [docs/MIGRATION.md](docs/MIGRATION.md) for a detailed migration guide.

Quick migration:

```bash
# 1. Update rsenv
cargo install rsenv

# 2. Update shell aliases
# Before: alias envbuild='rsenv build'
# After:  alias envbuild='rsenv env build'

# 3. Verify
rsenv --version
```

## Statistics

- **+13,933 lines** added
- **-5,348 lines** removed
- **108 files** changed
- Comprehensive test suite with v1 compatibility tests
