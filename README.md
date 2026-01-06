# rsenv

> Hierarchical environment management with secure vault storage.

[![License: BSD-3-Clause](https://img.shields.io/badge/License-BSD--3--Clause-blue.svg)](https://opensource.org/licenses/BSD-3-Clause)
[![Rust](https://img.shields.io/badge/rust-2021-orange.svg)](https://www.rust-lang.org/)

## What is rsenv?

Managing environment variables across development, staging, and production shouldn't require
copy-paste or risk exposing secrets. rsenv solves this with **hierarchical env files** that inherit
from parents and a **vault system** that keeps sensitive files outside your repository while
remaining fully functional through symlinks.

This is a complete rewrite of V1 with 100% functional compatibility and a lot of new features.

## Features

- **Hierarchical env files** - Link files with `# rsenv: parent.env`, children override parents
- **Vault storage** - Move sensitive files outside your project, replaced by symlinks
- **File swapping** - Overwrite files with locala custom versions, but keep them out of version control
- **SOPS encryption** - Encrypt sensitive files in the vault with GPG or Age
- **Shell integration** - Works seamlessly with direnv

[See all features in the wiki](https://github.com/sysid/rs-env/wiki)

## Quick Start

```bash
cargo install rsenv

rsenv init                  # Create vault for project
rsenv guard add .env        # Move .env to vault, create symlink
rsenv env tree              # View environment hierarchy
rsenv env select            # Interactive environment selection
```

[Full quick start guide](https://github.com/sysid/rs-env/wiki/Quick-Start)

## Commands

| Command | Purpose |
|---------|---------|
| `init`  | Create vault for project |
| `env`   | Manage environment hierarchy |
| `guard` | Protect sensitive files |
| `swap`  | Toggle dev/prod configs |
| `sops`  | Encrypt/decrypt vault |
| `config`| Manage settings |
| `info`  | Show project status |

[Full command reference](https://github.com/sysid/rs-env/wiki/Command-Reference)

## Documentation

**Getting Started**: [Installation](https://github.com/sysid/rs-env/wiki/Installation) · [Quick Start](https://github.com/sysid/rs-env/wiki/Quick-Start) · [Core Concepts](https://github.com/sysid/rs-env/wiki/Core-Concepts)

**Features**: [Environment Variables](https://github.com/sysid/rs-env/wiki/Environment-Variables) · [Vault Management](https://github.com/sysid/rs-env/wiki/Vault-Management) · [File Swapping](https://github.com/sysid/rs-env/wiki/File-Swapping) · [SOPS Encryption](https://github.com/sysid/rs-env/wiki/SOPS-Encryption)

**Reference**: [Commands](https://github.com/sysid/rs-env/wiki/Command-Reference) · [Configuration](https://github.com/sysid/rs-env/wiki/Configuration) · [Troubleshooting](https://github.com/sysid/rs-env/wiki/Troubleshooting) · [Migration Guide](https://github.com/sysid/rs-env/wiki/MIGRATION)

## License

BSD-3-Clause
