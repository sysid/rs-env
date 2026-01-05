# rsenv Documentation

Welcome to the rsenv documentation. rsenv is an opinionated CLI tool for managing development
environments, sensitive files, and configuration overrides through a unified vault-like system.

## The Problem

Development projects struggle with three related challenges:

1. **Environment variable duplication** - Copy-pasting configs across `local.env`, `test.env`, `prod.env`
2. **Secrets living dangerously** - Credentials in project directories, one `git add .` away from disaster
3. **Override friction** - Swapping files for development, forgetting to swap back

## The Solution

rsenv introduces **Vault** - a project-associated directory that lives *outside* your
project but is *bidirectionally linked* to it.

```
project/.envrc ←──symlink──→ vault/dot.envrc
     │                            │
     └── RSENV_VAULT points to ───┘
```

With this single connection, rsenv provides:
- **Hierarchical environments** - Build complex configs from simple, inheriting pieces
- **File guarding** - Move sensitive files to vault, leave symlinks behind
- **File swapping** - Temporary development overrides with automatic tracking
- **SOPS encryption** - Encrypt guarded contents for backup and sharing

## Getting Started

New to rsenv? Start here:

1. **[Quick Start](Quick-Start)** - Get productive in 5 minutes
2. **[Installation](Installation)** - Install rsenv on your system
3. **[Core Concepts](Core-Concepts)** - Understand vaults, linking, and the three capabilities
4. **[Configuration](Configuration)** - Customize rsenv for your workflow

## Features

### Environment Management
- **[Environment Variables](Environment-Variables)** - Hierarchical `.env` files with inheritance
- **[direnv Integration](Environment-Variables#direnv-integration)** - Automatic environment loading

### Security
- **[Vault Management](Vault-Management)** - Initialize projects, guard sensitive files
- **[SOPS Encryption](SOPS-Encryption)** - Encrypt selective content with GPG or Age

### Development Workflow
- **[File Overrides](File-Swapping)** - Temporary file overrides for development without polluting
  the original project

## Quick Command Reference

| Command | Description |
|---------|-------------|
| `rsenv init` | Initialize vault for current project |
| `rsenv env build <file>` | Build hierarchical environment |
| `rsenv env select` | Interactive environment selection |
| `rsenv guard add <file>` | Move file to vault, create symlink |
| `rsenv swap in <files>` | Replace with vault versions |
| `rsenv sops encrypt` | Encrypt vault contents |
| `rsenv info` | Show project and vault status |

For complete command reference, see [Command Reference](Command-Reference).

## Philosophy

rsenv is **opinionated by design**:

- **One vault per project** - Single source of truth, no confusion
- **Minimal pollution** - Maximal symlinks change in your project
- **Defense in depth** - Vault location + symlinks + encryption
- **Battle tested infra** - [direnv](https://direnv.net), gpg

## Reference

- **[Command Reference](Command-Reference)** - All commands and options
- **[Troubleshooting](Troubleshooting)** - Common issues and solutions
- **[Configuration](Configuration)** - Settings and customization

## External Resources

- [GitHub Repository](https://github.com/sysid/rsenv)
- [Crates.io](https://crates.io/crates/rsenv)
