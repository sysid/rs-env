# rsenv

**Hierarchical environment variable management for modern development workflows**

> [Documentation](https://sysid.github.io/hierarchical-environment-variable-management/)

## Overview

Managing environment variables across different environments (development, staging, production) and configurations (regions, features, services) is a common challenge in cloud-native projects. `rsenv` solves this by implementing hierarchical inheritance where child configurations automatically inherit and can override parent values.

**Key Benefits:**
- **Zero Duplication**: Share common variables across environments while customizing specific values
- **Clear Provenance**: Easily trace where each variable value originates in the hierarchy  
- **Type Safety**: Built-in validation and structured configuration management
- **Developer Experience**: Interactive selection, editing, and integration with popular tools

## Features

### Core Functionality
- **Hierarchical Inheritance**: Build environment trees from `.env` files with parent-child relationships
- **Smart Override Logic**: Child variables automatically override parent values with clear precedence rules
- **Standalone File Support**: Automatically detect and include independent `.env` files as single-node trees
- **Environment Expansion**: Full support for `$VAR` and `${VAR}` syntax in paths and comments
- **Flexible Linking**: Link files with comments supporting variable spacing: `# rsenv: parent.env`

### Interactive Tools
- **Fuzzy Selection**: Built-in fuzzy finder (skim) for rapid environment discovery and selection
- **Smart Editing**: Edit entire hierarchies side-by-side or individual files with full context
- **Tree Visualization**: Display hierarchical relationships and identify leaf nodes
- **Branch Analysis**: Linear representation of all environment chains

### Integrations
- **[direnv](https://direnv.net/)**: Automatic environment activation when entering directories
- **[JetBrains IDEs](https://plugins.jetbrains.com/plugin/7861-envfile)**: Native IDE integration via EnvFile plugin
- **Shell Integration**: Generate completion scripts for bash, zsh, fish, and powershell

### Concept
![concept](doc/concept.png)


## Installation

### From crates.io
```bash
cargo install rs-env
```

### From source
```bash
git clone https://github.com/sysid/rs-env
cd rs-env/rsenv
cargo install --path .
```

## Quick Start

### 1. File Structure
Create linked environment files using `# rsenv:` comments:

```bash
# base.env
export DATABASE_HOST=localhost
export LOG_LEVEL=info

# production.env
# rsenv: base.env
export DATABASE_HOST=prod.example.com
export LOG_LEVEL=warn
export ENVIRONMENT=production
```

### 2. Basic Usage
```bash
# Build complete environment from hierarchy
rsenv build production.env

# Load into current shell
source <(rsenv build production.env)

# Interactive selection with fuzzy finder
rsenv select environments/

# View tree structure
rsenv tree environments/

# Find all leaf environments
rsenv leaves environments/
```

### 3. Advanced Features
```bash
# Edit entire hierarchy side-by-side
rsenv tree-edit environments/

# Update direnv integration
rsenv envrc production.env

# Link files programmatically
rsenv link base.env staging.env production.env
```

## File Format

### Environment Files
- Use `export VAR=value` syntax (shell-compatible)
- Support single and double quotes
- Include `# rsenv: parent.env` comments for hierarchy

### Linking Syntax
```bash
# Basic parent reference
# rsenv: parent.env

# Multiple parents (creates DAG structure)
# rsenv: base.env shared.env

# Environment variable expansion
# rsenv: $HOME/config/base.env

# Flexible spacing (all valid)
# rsenv:parent.env
# rsenv: parent.env  
# rsenv:   parent.env
```

## Command Reference

### Core Commands
| Command | Description | Example |
|---------|-------------|---------|
| `build` | Build complete environment from hierarchy | `rsenv build prod.env` |
| `leaves` | List all leaf environment files | `rsenv leaves environments/` |
| `tree` | Display hierarchical structure | `rsenv tree environments/` |
| `branches` | Show linear representation of all chains | `rsenv branches environments/` |

### Interactive Commands  
| Command | Description | Example |
|---------|-------------|---------|
| `select` | Interactive environment selection + direnv update | `rsenv select environments/` |
| `edit` | Interactive selection and editing | `rsenv edit environments/` |
| `tree-edit` | Side-by-side editing of hierarchies | `rsenv tree-edit environments/` |

### Management Commands
| Command | Description | Example |
|---------|-------------|---------|
| `envrc` | Update .envrc file for direnv | `rsenv envrc prod.env .envrc` |
| `files` | List all files in hierarchy | `rsenv files prod.env` |
| `link` | Create parent-child relationships | `rsenv link base.env prod.env` |

### Global Options
- `-d, --debug`: Enable debug logging (use multiple times for increased verbosity)
- `--generate`: Generate shell completion scripts (bash, zsh, fish, powershell)
- `--info`: Display version and configuration information

## Examples

### Basic Workflow
<a href="https://asciinema.org/a/605946?autoplay=1&speed=1.5" target="_blank"><img src="https://asciinema.org/a/605946.svg" /></a>

### Interactive Selection  
<a href="https://asciinema.org/a/605951?autoplay=1&speed=1.5" target="_blank"><img src="https://asciinema.org/a/605951.svg" /></a>

### Tree Editing
<a href="https://asciinema.org/a/605950?autoplay=1&speed=1.5" target="_blank"><img src="https://asciinema.org/a/605950.svg" /></a>

## Integrations

### direnv Integration
[direnv](https://direnv.net/) provides automatic environment activation:

```bash
# Generate .envrc for automatic loading
rsenv envrc production.env .envrc

# Interactive selection with direnv update
rsenv select environments/

# Manual direnv commands
direnv allow    # Enable .envrc
direnv reload   # Refresh environment
```

### JetBrains IDEs
Integration via [EnvFile plugin](https://plugins.jetbrains.com/plugin/7861-envfile):

1. Install the EnvFile plugin
2. Create a run configuration script:
   ```bash
   #!/bin/bash
   # runenv.sh
   rsenv build "${RUN_ENV}.env"
   ```
3. Configure the plugin to use `runenv.sh`
4. Set `RUN_ENV` environment variable in your run configuration

[![jetbrain](doc/jetbrain.png)](doc/jetbrain.png)

### Shell Integration
Generate completion scripts for your shell:

```bash
# Bash
rsenv --generate bash > ~/.bash_completion.d/rsenv

# Zsh  
rsenv --generate zsh > ~/.zsh/completions/_rsenv

# Fish
rsenv --generate fish > ~/.config/fish/completions/rsenv.fish
```



## Development

### Building from Source
```bash
git clone https://github.com/sysid/rs-env
cd rs-env/rsenv
cargo build --release
```

### Running Tests
```bash
# Run all tests (single-threaded to prevent conflicts)
cargo test -- --test-threads=1

# Run with debug output
RUST_LOG=debug cargo test -- --test-threads=1

# Test via Makefile (includes interactive tests)
make test
```

### Project Structure
- **`src/lib.rs`**: Core environment expansion and tree building logic
- **`src/builder.rs`**: TreeBuilder for constructing hierarchical structures  
- **`src/arena.rs`**: Arena-based tree data structures
- **`src/cli/`**: Command-line interface and command implementations
- **`tests/`**: Comprehensive test suite with example environments

### Testing Notes
- Interactive tests using skim require a valid terminal and are run via Makefile
- Tests run single-threaded to prevent file system conflicts
- Test resources in `tests/resources/environments/` demonstrate complex hierarchies

### Contributing
1. Fork the repository
2. Create a feature branch
3. Add tests for new functionality
4. Ensure all tests pass: `cargo test -- --test-threads=1`
5. Run formatting: `cargo fmt`
6. Run linting: `cargo clippy`
7. Submit a pull request
