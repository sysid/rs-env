# Installation

## Requirements

- **Rust** 1.70 or later (for cargo install)
- **direnv** - recommended for automatic environment loading
- **SOPS** - required for encryption features (optional)
- **GPG** - required for GPG-based encryption (optional)

## Install rsenv

### Via Cargo (Recommended)

```bash
cargo install rsenv
```

### From Source

```bash
git clone https://github.com/sysid/rsenv
cd rsenv/rsenv
cargo install --path .
```

### Verify Installation

```bash
rsenv --version
rsenv info
```

## Install direnv

rsenv works best with [direnv](https://direnv.net/) for automatic environment loading.

### macOS

```bash
brew install direnv
```

### Linux (Debian/Ubuntu)

```bash
apt install direnv
```

### Hook into Shell

Add to your shell config:

**bash** (`~/.bashrc`):
```bash
eval "$(direnv hook bash)"
```

**zsh** (`~/.zshrc`):
```bash
eval "$(direnv hook zsh)"
```

**fish** (`~/.config/fish/config.fish`):
```fish
direnv hook fish | source
```

## Install SOPS (Optional)

SOPS is required for vault encryption features.

### macOS

```bash
brew install sops
```

### Linux

```bash
# Download from releases
curl -LO https://github.com/getsops/sops/releases/download/v3.8.1/sops-v3.8.1.linux.amd64
chmod +x sops-v3.8.1.linux.amd64
sudo mv sops-v3.8.1.linux.amd64 /usr/local/bin/sops
```

### Verify

```bash
sops --version
```

## Shell Completions

Generate completions for your shell:

```bash
# bash
rsenv completion bash > ~/.local/share/bash-completion/completions/rsenv

# zsh
rsenv completion zsh > ~/.zfunc/_rsenv
# Then add to ~/.zshrc: fpath=(~/.zfunc $fpath)

# fish
rsenv completion fish > ~/.config/fish/completions/rsenv.fish

# powershell
rsenv completion powershell > $HOME\Documents\PowerShell\Modules\rsenv\rsenv.psm1
```

## Initial Configuration

Create a global configuration file:

```bash
rsenv config init --global
```

This creates `~/.config/rsenv/rsenv.toml`. Edit to set:
- `vault_base_dir` - where vaults are stored
- `sops.gpg_key` - GPG key for encryption
- `editor` - preferred editor

See [Configuration](Configuration) for details.

## Verify Setup

```bash
# Check configuration
rsenv config show

# Check paths
rsenv config path
```

## Next Steps

- **[Quick Start](Quick-Start)** - Initialize your first vault
- **[Core Concepts](Core-Concepts)** - Understand how rsenv works
- **[Configuration](Configuration)** - Customize settings
