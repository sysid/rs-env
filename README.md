# rs-env

# Features
- Compile your environment variables from a hierarchical list of `<name>.env` files.
- Dependencies form a tree/DAG, each file can have several parents.
- Last defined variable wins, i.e. child tops parent, rightmost sibling tops left sibling.
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

### Usage
- **branch**: a linear list of files, each file can have one parent (no DAG). It defines the resulting set of variables.
- **DAG**: Directed acyclic graph, i.e. files can have multiple parents
- **tree**: a collection of branches (environment files can be part of multiple branches)
- environment variables are defined in files `<name>.env` and must be prefixed with `export` command
- See [examples](./rsenv/tests/resources/environments) for DAG, Tree and Branches
- multiple trees per project are supported
- files are linked into a **branch** either manually by adding the comment line `# rsenv: <name.env>`
or via `rsenv link <root.env> <child1>.env <child2>.env`.

That's it. Now you can source the resulting set of variables as usual:
```bash
source <(rsenv build <leaf-node.env>)
```

```bash
Hierarchical environment variable management

Usage: rsenv [OPTIONS] [NAME] [COMMAND]

Commands:
  build      Build the resulting set of environment variables (DAG/Tree)
  envrc      Write the resulting set of variables to .envrc (requires direnv, DAG/Tree)
  files      Show all files involved in resulting set (DAG/Tree)
  edit       Edit the FZF selected branch/DAG
  select     FZF based selection of environment/branch and update of .envrc file (requires direnv, DAG/Tree)
  link       Link files into a linear dependency branch (root -> parent -> child)
  branches   Show all branches (linear representation)
  tree       Show all trees (hierarchical representation)
  tree-edit  Edit branches of all trees side-by-side (vim required in path)
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
