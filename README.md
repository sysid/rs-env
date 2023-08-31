# rs-env

## Features
- Build environment variable set from a hierarchical list of `<name>.env` files.
- Hierarchy forms a tree, each file can point to one parent (no DAG).
- Last defined variable wins, i.e. child tops parent.
- Have the final variable list updated in your `.envrc` file for clean environment management.
- Quick selection of environments via FZF.
- Quick edit via FZF.
- Smart edit of parallel dependency trees side-by-side for full transparency

```bash
# simple activation of environment
source <(rsenv build)
```

### Usage
```bash
Hierarchical environment variable management

Usage: rsenv [OPTIONS] [NAME] [COMMAND]

Commands:
  build      Build the resulting set of environment variables
  envrc      Write the resulting set of environment variables to .envrc (requires direnv)
  files      Show all files involved in building the variable set
  edit       Edit the FZF selected file and its linked parents (dependency chain)
  select     FZF based selection of environment and update of .envrc file (requires direnv)
  link       Link files into a dependency tree
  tree       Show all dependency trees
  tree-edit  Edit all dependency trees side-by-side (vim required)
  help       Print this message or the help of the given subcommand(s)

Arguments:
  [NAME]  Optional name to operate on

Options:
  -d, --debug...              Turn debugging information on
      --generate <GENERATOR>  [possible values: bash, elvish, fish, powershell, zsh]
      --info                  
  -h, --help                  Print help
  -V, --version               Print version
```

# Direnv Integration
[EnvFile](https://plugins.jetbrains.com/plugin/7861-envfile) activates environments automatically.
- rs-env can update the `.envrc` file with the selected dependency graph variables.

# JetBrains Integration: Life injection of environment variables
- Plugin [EnvFile](https://plugins.jetbrains.com/plugin/7861-envfile) can be used to life-inject environment variables.
- Use the script `runenv.sh` as the "EnvFile" script (tick executable checkbox !).
- The environment variable `RUN_ENV` and will tell the script which environment to load.
- It will look for a file `<RUN_ENV>.env` in the specified directory.


# Development
- Tests for "skim" need valid terminal, so they are run via Makefile.
- Test for `rsenv select`: run debug target and check rsenv .envrc file.
