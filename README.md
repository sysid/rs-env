# rs-env

# Features
- Compile your environment variables from a hierarchical list of `<name>.env` files.
- Dependencies form a tree, each file can have one parent (no DAG).
- Last defined variable wins, i.e. child tops parent.
- Quick selection of environments via builtin FZF (fuzzy find).
- Quick edit via builtin FZF.
- Smart edit of dependency trees side-by-side
- Chain your dependencies with one command
- [direnv](https://direnv.net/) integration: Have the resulting variable list documented in your `.envrc` file.
- [JetBrains](https://www.jetbrains.com/) integration via [EnvFile](https://plugins.jetbrains.com/plugin/7861-envfile) plugin.

### Concept
![concept](doc/concept.png)


### Installation
```bash
cargo install rs-env
```

### Basic Usage
```bash
# simple activation of environment
source <(rsenv build <name.env>)
```

### Comprehensive Usage
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

<br>

## Integrations
### direnv
[direnv](https://direnv.net/) activates environments automatically.
- rs-env can update the `.envrc` file with the selected dependency graph variables.


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
