# rs-env

> [Hierarchical environment variable management](https://sysid.github.io/hierarchical-environment-variable-management/)

## Why
Managing environment variables for different stages, regions, etc. is an unavoidable chore
when working on cloud projects.

Especially the challenge of avoiding duplication and knowing where a particular value is coming from.
Hierarchical variable management seems to be a good solution for this problem.

# Features
- Compile a resulting set of environment variables from a linked list of `<name>.env` files.
- Linked `.env` files form trees. Paths from leave-nodes to root (branches) form the resulting set of variables.
- Last defined variable wins, i.e. child tops parent.
- Smart environment selection via builtin FZF (fuzzy find).
- Quick edit via builtin FZF.
- Side-by-side Tree edit.
- [direnv](https://direnv.net/) integration: Have the resulting variable list written to your `.envrc` file.
- [JetBrains](https://www.jetbrains.com/) integration via [EnvFile](https://plugins.jetbrains.com/plugin/7861-envfile) plugin.

### Concept
![concept](doc/concept.png)


### Installation
```bash
cargo install rs-env
```

### Usage
The resulting set of environment variables comes from a merge of all linked `.env` files.

- **branch**: a linear list of files, each file can have one parent (no DAG).
- **tree**: a collection of branches (files can be part of multiple branches, but only one parent)
- environment variables are defined in files `<name>.env` and must be prefixed with `export` command
- See [examples](./rsenv/tests/resources/environments)
- multiple trees/branches per project are supported
- files are linked by adding the comment line `# rsenv: <name.env>` or via: `rsenv link <root.env> <child1>.env <child2>.env`.

Publish the resulting set of variables to the shell:
```bash
source <(rsenv build <leaf-node.env>)
```

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
[direnv](https://direnv.net/) activates environments automatically.
- rs-env can update the `.envrc` file with the dependency graph variables.


### JetBrains Integration
Life injection of environment variables:
- Plugin [EnvFile](https://plugins.jetbrains.com/plugin/7861-envfile) can be used to life-inject environment variables.
- Use the script `runenv.sh` as the "EnvFile" script (tick executable checkbox !).
- The environment variable `RUN_ENV` parametrizes which environment to load.
- It will look for a file `<RUN_ENV>.env` in the specified directory.

[![jetbrain](doc/jetbrain.png)](doc/jetbrain.png)



## Development
- Tests for "skim" need valid terminal, so they are run via Makefile.
- Test for `rsenv select`: run debug target and check rsenv .envrc file.
