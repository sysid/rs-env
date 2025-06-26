# rs-env

> [Hierarchical environment variable management](https://sysid.github.io/hierarchical-environment-variable-management/)

## Why
Managing environment variables for different stages, regions, etc. is an unavoidable chore
when working on cloud projects.

Especially the challenge of avoiding duplication and knowing where a particular value is coming from.
Hierarchical variable management seems to be a good solution for this problem.

# Features
- **Hierarchical inheritance**: Compile environment variables from linked `.env` files forming tree structures
- **Variable override**: Child variables override parent variables (last defined wins)
- **Environment variable expansion**: Support for `$VAR` and `${VAR}` syntax in file paths and rsenv comments
- **Interactive selection**: Smart environment selection and editing via built-in FZF
- **Multiple integrations**: 
  - [direnv](https://direnv.net/) integration for automatic environment loading
  - [JetBrains EnvFile plugin](https://plugins.jetbrains.com/plugin/7861-envfile) support
- **Flexible editing**: Side-by-side tree editing and individual file editing

### Concept
![concept](doc/concept.png)


### Installation
```bash
cargo install rs-env
```

### Usage

**File Linking**: Environment files are linked via comments:
```bash
# rsenv: parent.env
# rsenv: $HOME/config/base.env    # Environment variables supported
export MY_VAR=value
```

**Basic Commands**:
```bash
# Build and display environment variables
rsenv build production.env

# Load variables into current shell
source <(rsenv build production.env)

# Interactive environment selection
rsenv select

# View hierarchy structure
rsenv tree
```

**Structure**:
- **Environment variables** must use `export` prefix in `.env` files
- **Tree structure** where child variables override parent variables
- **Multiple hierarchies** supported per project
- See [examples](./rsenv/tests/resources/environments) for detailed usage patterns

```
Hierarchical environment variable management

Usage: rsenv [OPTIONS] [NAME] [COMMAND]

Commands:
  build        Build and display the complete set of environment variables
  envrc        Write environment variables to .envrc file (requires direnv)
  files        List all files in the environment hierarchy
  edit-leaf    Edit an environment file and all its parent files
  edit         Interactively select and edit an environment hierarchy
  select-leaf  Update .envrc with selected environment (requires direnv)
  select       Interactively select environment and update .envrc (requires direnv)
  link         Create parent-child relationships between environment files
  branches     Show all branches (linear representation)
  tree         Show all trees (hierarchical representation)
  tree-edit    Edit all environment hierarchies side-by-side (requires vim)
  leaves       List all leaf environment files
  help         Print this message or the help of the given subcommand(s)

Arguments:
  [NAME]  Name of the configuration to operate on (optional)

Options:
  -d, --debug...              Enable debug logging. Multiple flags (-d, -dd, -ddd) increase verbosity
      --generate <GENERATOR>  Generate shell completion scripts [possible values: bash, elvish, fish, powershell, zsh]
      --info                  Display version and configuration information
  -h, --help                  Print help
  -V, --version               Print version
```

#### Basic
<a href="https://asciinema.org/a/605946?autoplay=1&speed=1.5" target="_blank"><img src="https://asciinema.org/a/605946.svg" /></a>
<br>

#### Select via FZF
<a href="https://asciinema.org/a/605951?autoplay=1&speed=1.5" target="_blank"><img src="https://asciinema.org/a/605951.svg" /></a>
<br>

#### Tree and Branch structure (Smart edit)
<a href="https://asciinema.org/a/605950?autoplay=1&speed=1.5" target="_blank"><img src="https://asciinema.org/a/605950.svg" /></a>
<br>

## Integrations

### direnv
[direnv](https://direnv.net/) automatically activates environments when entering directories:
```bash
# Update .envrc with selected environment
rsenv envrc production.env .envrc
```

### JetBrains IDEs
Use the [EnvFile plugin](https://plugins.jetbrains.com/plugin/7861-envfile) for IDE integration:
- Configure `runenv.sh` as the EnvFile script
- Set `RUN_ENV` environment variable to specify which environment to load
- The plugin will automatically load variables from `<RUN_ENV>.env`

[![jetbrain](doc/jetbrain.png)](doc/jetbrain.png)



## Development
- Tests for "skim" need valid terminal, so they are run via Makefile.
- Test for `rsenv select`: run debug target and check rsenv .envrc file.
