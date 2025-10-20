# rsenv

**Hierarchical environment variable management for modern development workflows**

> [Documentation](https://sysid.github.io/hierarchical-environment-variable-management/) | [Wiki](https://github.com/sysid/rs-env/wiki)

[![Crates.io](https://img.shields.io/crates/v/rsenv.svg)](https://crates.io/crates/rsenv)
[![License](https://img.shields.io/badge/licensee-MIT-blue.svg)](LICENSE)

## Why rs-env?

Managing environment variables across different environments (development, staging, production) and
configurations (regions, features, services) creates massive duplication in traditional `.env`
files. 

**rs-env solves this** by implementing hierarchical inheritance where child configurations
automatically inherit and override parent values.

## How It Works

Environment files form **directed acyclic graphs (DAGs)** where:
1. Child files link to parents via `# rsenv: parent.env` comments
2. Variables are inherited and merged from parent to child
3. Child values override parent values (last defined wins)
4. The `build` command compiles the complete environment

![Concept](doc/concept.png)

## Quick Demo

<a href="https://asciinema.org/a/605946?autoplay=1&speed=1.5" target="_blank"><img src="https://asciinema.org/a/605946.svg" /></a>

## Installation

```bash
# From crates.io
cargo install rs-env

# From source
git clone https://github.com/sysid/rs-env
cd rs-env/rsenv
cargo install --path .
```

**Requirements**: Rust 1.70+ ([Install Rust](https://rustup.rs/))

## 30-Second Example

Create a hierarchy where child environments inherit from parents:

```bash
# base.env - Shared configuration
export DATABASE_HOST=localhost
export LOG_LEVEL=info

# production.env - Inherits from base, overrides specific values
# rsenv: base.env
export DATABASE_HOST=prod-db.example.com
export LOG_LEVEL=error
export ENVIRONMENT=production
```

Build the complete environment:

```bash
# Build production environment
rsenv build production.env

# Load into your shell
source <(rsenv build production.env)

# Verify
echo $DATABASE_HOST  # prod-db.example.com (from production.env)
echo $LOG_LEVEL      # error (from production.env)
```

**Result**: `production.env` inherits `base.env` variables and overrides what changes. The `# rsenv: base.env` comment creates the parent-child link.

## Core Features

### Hierarchical Inheritance
- Build environment trees from `.env` files with parent-child relationships
- Smart override logic: child variables automatically override parent values
- Standalone file support: independent `.env` files work as single-node trees

### Interactive Tools
- **Fuzzy Selection** - Built-in fuzzy finder for rapid environment discovery
- **Smart Editing** - Edit entire hierarchies side-by-side or individual files
- **Tree Visualization** - Display relationships and identify leaf nodes

### Integrations
- **[direnv](https://direnv.net/)** - Automatic environment activation when entering directories
- **[JetBrains IDEs](https://plugins.jetbrains.com/plugin/7861-envfile)** - Native IDE integration via EnvFile plugin
- **Shell Completion** - bash, zsh, fish, and powershell support

## Essential Commands

| Command | Purpose |
|---------|---------|
| `rsenv build <file>` | Build complete environment from hierarchy |
| `rsenv select <dir>` | Interactive selection + direnv update |
| `rsenv tree <dir>` | Display hierarchical structure |
| `rsenv edit <dir>` | Interactive selection and editing |

**[Full Command Reference](https://github.com/sysid/rs-env/wiki/Command-Reference)**

## Documentation

**New to rs-env?** Start with the [Quick Start Guide](https://github.com/sysid/rs-env/wiki/Quick-Start)

**Comprehensive documentation** is available in the [rs-env Wiki](https://github.com/sysid/rs-env/wiki):

- **Getting Started**: [Quick Start](https://github.com/sysid/rs-env/wiki/Quick-Start), [Installation](https://github.com/sysid/rs-env/wiki/Installation), [Core Concepts](https://github.com/sysid/rs-env/wiki/Core-Concepts)
- **Core Features**: [Building Environments](https://github.com/sysid/rs-env/wiki/Building-Environments), [Viewing Hierarchies](https://github.com/sysid/rs-env/wiki/Viewing-Hierarchies), [File Format](https://github.com/sysid/rs-env/wiki/File-Format)
- **Interactive Tools**: [Interactive Selection](https://github.com/sysid/rs-env/wiki/Interactive-Selection), [Tree Editing](https://github.com/sysid/rs-env/wiki/Tree-Editing)
- **Integrations**: [direnv](https://github.com/sysid/rs-env/wiki/direnv-Integration), [JetBrains IDEs](https://github.com/sysid/rs-env/wiki/JetBrains-IDEs), [Shell Completion](https://github.com/sysid/rs-env/wiki/Shell-Completion)
- **Advanced**: [Managing Links](https://github.com/sysid/rs-env/wiki/Managing-Links), [Complex Hierarchies](https://github.com/sysid/rs-env/wiki/Complex-Hierarchies)
- **Reference**: [Command Reference](https://github.com/sysid/rs-env/wiki/Command-Reference), [Troubleshooting](https://github.com/sysid/rs-env/wiki/Troubleshooting)

## Contributing

Contributions are welcome! See the [Development Guide](https://github.com/sysid/rs-env/wiki/Development) for details on building, testing, and contributing.

```bash
# Quick start for contributors
git clone https://github.com/sysid/rs-env
cd rs-env/rsenv
cargo test -- --test-threads=1
cargo build --release
```

## License

MIT License - see [LICENSE](LICENSE) for details.

## Links

- 📦 [crates.io Package](https://crates.io/crates/rsenv)
- 📖 [Wiki Documentation](https://github.com/sysid/rs-env/wiki)
- 🐛 [Issue Tracker](https://github.com/sysid/rs-env/issues)
