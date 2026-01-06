# rsenv

> V2 is a refactoring of V1 which delivers same functionality plus a lot more! The UX has changed
> (MIGRATION.md under docs/)

Hierarchical environment management with secure vault storage.

[![License: BSD-3-Clause](https://img.shields.io/badge/License-BSD--3--Clause-blue.svg)](https://opensource.org/licenses/BSD-3-Clause)
[![Rust](https://img.shields.io/badge/rust-2021-orange.svg)](https://www.rust-lang.org/)

## Overview

rsenv is a CLI tool for managing development environments. It combines:

- **Hierarchical env files** with parent linking (`# rsenv: parent.env`)
- **Vault storage** for sensitive files (symlinked back to project)
- **File swapping** for development-specific configs
- **SOPS encryption** for securing vault contents

## Features

- Build merged environment variables from hierarchical env files
- Guard sensitive files by moving them to a vault with symlinks
- Swap files in/out for development vs production configs
- Interactive fuzzy selection with skim
- Shell integration with direnv
- Optional SOPS encryption for vault contents

## Getting Started

### Prerequisites

- Rust toolchain (1.70+)
- [direnv](https://direnv.net/) (recommended)
- [SOPS](https://github.com/getsops/sops) (optional, for encryption)

### Installation

```bash
cd rsenv
cargo install --path .
```

### Quick Start

```bash
# Initialize vault for current project
rsenv init

# Guard a sensitive file (moves to vault, creates symlink)
rsenv guard add .env

# View environment hierarchy
rsenv env tree

# Build merged environment from leaf file
rsenv env build envs/prod.env

# Interactively select and activate environment
rsenv env select
```

## CLI Reference

| Command | Subcommands | Description |
|---------|-------------|-------------|
| `init` | `reset` | Create vault for project, or reset (undo) initialization |
| `env` | `build`, `envrc`, `files`, `select`, `tree`, `branches`, `edit`, `edit-leaf`, `tree-edit`, `leaves`, `link`, `unlink` | Environment hierarchy management |
| `guard` | `add`, `list`, `restore` | Guard sensitive files (move to vault with symlink) |
| `swap` | `in`, `out`, `init`, `status`, `all-out` | Swap files between project and vault |
| `sops` | `encrypt`, `decrypt`, `clean`, `status` | SOPS encryption/decryption |
| `config` | `show`, `init`, `path` | Configuration management |
| `info` | - | Show project and vault status |
| `completion` | - | Generate shell completions |

### Global Options

```
-v, --verbose         Enable verbose output
-C, --project-dir     Project directory (defaults to current directory)
```

### Environment Hierarchy

Link env files with a comment directive:

```bash
# envs/prod.env
# rsenv: base.env
export API_URL=https://api.prod.example.com
```

Build merged variables (children override parents):

```bash
rsenv env build envs/prod.env
# Outputs: export KEY="value" format
```

### File Guarding

```bash
# Move .env to vault, replace with symlink
rsenv guard add .env

# List guarded files
rsenv guard list

# Restore file from vault
rsenv guard restore .env
```

### File Swapping

```bash
# Initialize: move project file to vault for first-time setup
rsenv swap init config.yaml

# Swap in development version
rsenv swap in config.yaml

# Swap out (restore original)
rsenv swap out config.yaml

# Check status
rsenv swap status
```

## Configuration

Config files are loaded in order (later overrides earlier):

1. Defaults
2. Global: `~/.config/rsenv/rsenv.toml`
3. Local: `<vault>/.rsenv.toml`
4. Environment variables: `RSENV_*`

### Create Config

```bash
# Create local config (in vault)
rsenv config init

# Create global config
rsenv config init --global

# Show effective config
rsenv config show
```

### Key Settings

| Setting | Default | Description |
|---------|---------|-------------|
| `vault_base_dir` | `~/.rsenv/vaults` | Where vaults are stored |
| `editor` | `$EDITOR` or `vim` | Editor for `env edit` commands |
| `sops.gpg_key` | - | GPG key for SOPS encryption |
| `sops.age_key` | - | Age key for SOPS encryption |

## Development

```bash
cd rsenv

# Build
cargo build

# Test (single-threaded required)
cargo test -- --test-threads=1

# Format and lint
cargo fmt
cargo clippy
```

### Pre-commit Hooks

```bash
# Install pre-commit (if not already installed)
pip install pre-commit

# Install hooks
pre-commit install

# Run manually
pre-commit run --all-files
```

Hooks run: `cargo fmt`, `cargo clippy`, `cargo check`, plus file hygiene checks.

## License

BSD-3-Clause
