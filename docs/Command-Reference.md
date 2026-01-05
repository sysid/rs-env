# Command Reference

Complete reference for all rsenv commands.

## Global Options

```bash
rsenv [OPTIONS] <COMMAND>
```

| Option | Description |
|--------|-------------|
| `-v, --verbose` | Enable verbose output |
| `-C, --project-dir <PATH>` | Project directory (defaults to current) |
| `--version` | Show version |
| `--help` | Show help |

## init

Initialize a vault for a project.

```bash
rsenv init [OPTIONS] [PROJECT]
```

| Argument | Description |
|----------|-------------|
| `PROJECT` | Project directory (defaults to current) |

| Option | Description |
|--------|-------------|
| `--absolute` | Use absolute paths for symlinks (default: relative) |

**Examples:**
```bash
rsenv init
rsenv init ~/myproject
rsenv init --absolute
```

### init reset

Undo initialization: restore guarded files, remove .envrc symlink.

```bash
rsenv init reset [PROJECT]
```

| Argument | Description |
|----------|-------------|
| `PROJECT` | Project directory (defaults to current) |

**Note:** The vault directory is NOT deleted.

## env

Environment variable hierarchy management.

### env build

Build hierarchical environment variables.

```bash
rsenv env build <FILE>
```

| Argument | Description |
|----------|-------------|
| `FILE` | Leaf env file to build from |

**Example:**
```bash
rsenv env build $RSENV_VAULT/envs/local.env
source <(rsenv env build local.env)
```

### env envrc

Write environment to .envrc file (direnv integration).

```bash
rsenv env envrc <FILE> [OPTIONS]
```

| Argument | Description |
|----------|-------------|
| `FILE` | Leaf env file to build from |

| Option | Description |
|--------|-------------|
| `-e, --envrc <PATH>` | Target .envrc file (default: ./.envrc) |

### env files

List all files in environment hierarchy.

```bash
rsenv env files <FILE>
```

### env select

Interactively select an environment (fuzzy finder).

```bash
rsenv env select [DIR]
```

| Argument | Description |
|----------|-------------|
| `DIR` | Directory to search for env files |

### env tree

Show environment hierarchy as tree.

```bash
rsenv env tree [DIR]
```

### env branches

Show all branches (linear representation).

```bash
rsenv env branches [DIR]
```

### env edit

Edit an environment file (FZF select).

```bash
rsenv env edit [DIR]
```

### env edit-leaf

Edit a leaf file and all its parents.

```bash
rsenv env edit-leaf <FILE>
```

### env tree-edit

Edit all environment hierarchies side-by-side.

```bash
rsenv env tree-edit [DIR]
```

### env leaves

List all leaf environment files.

```bash
rsenv env leaves [DIR]
```

### env link

Link parent-child env files.

```bash
rsenv env link <FILES>...
```

| Argument | Description |
|----------|-------------|
| `FILES` | Files to link (first is root, each subsequent links to previous) |

**Examples:**
```bash
# Link parent to child
rsenv env link base.env local.env

# Create chain: root <- middle <- leaf
rsenv env link base.env cloud.env prod.env
```

### env unlink

Remove parent link from env file.

```bash
rsenv env unlink <FILE>
```

## guard

Guard sensitive files (symlink to vault).

### guard add

Add a file to guard (move to vault, create symlink).

```bash
rsenv guard add <FILE> [OPTIONS]
```

| Argument | Description |
|----------|-------------|
| `FILE` | File to guard |

| Option | Description |
|--------|-------------|
| `--absolute` | Use absolute paths for symlinks |

### guard list

List guarded files.

```bash
rsenv guard list
```

### guard restore

Restore a guarded file from vault.

```bash
rsenv guard restore <FILE>
```

| Argument | Description |
|----------|-------------|
| `FILE` | File to restore |

## swap

Swap files in/out between project and vault.

### swap init

Initialize: move project files to vault (first-time setup).

```bash
rsenv swap init <FILES>...
```

### swap in

Swap files in (replace with vault versions).

```bash
rsenv swap in <FILES>...
```

### swap out

Swap files out (restore originals).

```bash
rsenv swap out <FILES>...
```

### swap status

Show swap status.

```bash
rsenv swap status
```

### swap all-out

Swap out all projects under a directory.

```bash
rsenv swap all-out [BASE_DIR]
```

| Argument | Description |
|----------|-------------|
| `BASE_DIR` | Base directory to search |

### swap delete

Delete swap files from vault (remove override + backup).

```bash
rsenv swap delete <FILES>...
```

| Argument | Description |
|----------|-------------|
| `FILES` | Files to delete from swap management |

**Safety**: Refuses if any file is currently swapped in. All-or-nothing validation prevents partial deletions.

## sops

SOPS encryption/decryption.

### sops encrypt

Encrypt files matching config patterns.

```bash
rsenv sops encrypt [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `-d, --dir <PATH>` | Directory to encrypt (defaults to vault) |

### sops decrypt

Decrypt .enc files.

```bash
rsenv sops decrypt [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `-d, --dir <PATH>` | Directory to decrypt (defaults to vault) |

### sops clean

Delete plaintext files matching encryption patterns.

```bash
rsenv sops clean [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `-d, --dir <PATH>` | Directory to clean (defaults to vault) |

### sops status

Show encryption status.

```bash
rsenv sops status [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `-d, --dir <PATH>` | Directory to check (defaults to vault) |

## config

Configuration management.

### config show

Show effective configuration.

```bash
rsenv config show
```

### config init

Create template config file.

```bash
rsenv config init [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `-g, --global` | Create global config (~/.config/rsenv/rsenv.toml) |

Without `--global`: creates config in project's vault directory.

### config path

Show config file paths.

```bash
rsenv config path
```

## info

Show project and vault status.

```bash
rsenv info
```

## completion

Generate shell completions.

```bash
rsenv completion <SHELL>
```

| Argument | Values |
|----------|--------|
| `SHELL` | `bash`, `zsh`, `fish`, `powershell` |

**Examples:**
```bash
rsenv completion bash > ~/.local/share/bash-completion/completions/rsenv
rsenv completion zsh > ~/.zfunc/_rsenv
rsenv completion fish > ~/.config/fish/completions/rsenv.fish
```

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | General error |
| 2 | Invalid arguments |
| 64 | Usage error |
| 65 | Data error |
| 66 | No input |
| 74 | I/O error |
| 78 | Configuration error |

## Environment Variables

| Variable | Description |
|----------|-------------|
| `RSENV_VAULT` | Path to current project's vault (set by rsenv) |
| `RSENV_SWAPPED` | Set to `1` when files are swapped in (managed by rsenv) |
| `RSENV_VAULT_BASE_DIR` | Override vault base directory |
| `RSENV_EDITOR` | Override editor |
| `RSENV_SOPS_GPG_KEY` | Override SOPS GPG key |

See [Configuration](Configuration) for all environment variables.
