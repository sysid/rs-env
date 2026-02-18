<img src="doc/logo.png" alt="rsenv logo" width="240">


[![License: BSD-3-Clause](https://img.shields.io/badge/License-BSD--3--Clause-blue.svg)](https://opensource.org/licenses/BSD-3-Clause)
[![Rust](https://img.shields.io/badge/rust-2021-orange.svg)](https://www.rust-lang.org/)

> This is a complete rewrite of V1 with 100% functional compatibility and new features.

## Why?

- Working on several projects, with different teams, many repos is my reality.
- I need a personal workspace, which is attached to the project, but does not become part of its
  official repository (e.g. patched docker-compose.yml, bespoke Java test classes, etc.)
- My environment configurations are not DRY, they share variables, often follow even a hierarchy
  (globlal -> company -> region -> stage)


---

## 1. The Vault

The vault is a directory **outside the project** that holds a personal
workspace: env files, secrets, dev overrides. 

It is linked to the project via a single `.envrc` symlink — **the only trace of rsenv in your repo**.

```
  YOUR PROJECT                              THE VAULT
  ~/projects/myapp/                         ~/.rsenv/vaults/myapp-a1b2c3d4/
  ┌─────────────────────────┐               ┌──────────────────────────────┐
  │                         │               │                              │
  │  .envrc ───── symlink ──────────────────── dot.envrc                   │
  │                         │               │                              │
  │  src/                   │               │  envs/                       │
  │  Makefile               │               │    local.env                 │
  │  docker-compose.yml     │               │    prod.env                  │
  │  config/                │               │                              │
  │    secrets.yaml         │               │  guarded/     (secrets)      │
  │    database.yml         │               │  swap/        (dev overrides)│
  │                         │               │  .rsenv.toml  (vault config) │
  └─────────────────────────┘               └──────────────────────────────┘
        git-tracked                              outside project git
        minimal footprint:                       your personal workspace
        ONE symlink
```

**How it connects**: `rsenv init vault` creates the vault, moves the `.envrc`
there (as `dot.envrc`), and creates the symlink.

---

## 2. File Swapping

Swap temporarily replaces project files with your dev versions.
Unlike guard, this is reversible — swap in when you start work, swap out when done.

```
  ┌─ NORMAL STATE (swapped out) ──────────────────────────────────────────┐
  │                                                                       │
  │  Project                              Vault/swap                      │
  │  ┌──────────────────────┐             ┌──────────────────────┐        │
  │  │ docker-compose.yml   │             │ docker-compose.yml   │        │
  │  │ (official version)   │             │ (your dev version)   │        │
  │  └──────────────────────┘             └──────────────────────┘        │
  │                                                                       │
  └───────────────────────────────────────────────────────────────────────┘
                          │ rsenv swap in
                          ▼
  ┌─ SWAPPED IN (working) ────────────────────────────────────────────────┐
  │                                                                       │
  │  Project                              Vault/swap                      │
  │  ┌──────────────────────┐             ┌──────────────────────────┐    │
  │  │ docker-compose.yml   │             │ docker-compose.yml       │    │
  │  │ (your dev version)   │             │   .rsenv_original        │    │
  │  │  ◄── moved here      │             │   ◄── backup of official │    │
  │  └──────────────────────┘             │                          │    │
  │                                       │ docker-compose.yml       │    │
  │                                       │   .<hostname>.rsenv_active    │
  │                                       │   ◄── sentinel (who did it)   │
  │                                       └──────────────────────────┘    │
  │                                                                       │
  └───────────────────────────────────────────────────────────────────────┘
                          │ rsenv swap out
                          ▼
               back to normal state
        (your changes to dev version are PRESERVED)
```

**Hostname tracking**: The sentinel `.<hostname>.rsenv_active` records which
machine swapped the file in, preventing conflicts when sharing vaults.

**Key commands**:
- `rsenv swap init <files>` — set up files for swapping (first time)
- `rsenv swap in` — replace project files with vault versions
- `rsenv swap out` — restore originals (no args = all files)
- `rsenv swap status` — show what's swapped in, by which host

---

## 3. Environment Hierarchy

Env files form a tree using the `# rsenv: parent.env` directive.
Children inherit all parent variables and can override them.

```
  File contents:                          Resulting tree:

  ┌─ base.env ─────────────────┐               base.env
  │ export DB_HOST=localhost   │              /        \
  │ export DB_PORT=5432        │         local.env    cloud.env
  │ export LOG_LEVEL=info      │                      /       \
  └────────────────────────────┘             staging.env    prod.env

  ┌─ cloud.env ────────────────┐
  │ # rsenv: base.env          │  ◄── links to parent
  │ export DB_HOST=rds.aws.com │  ◄── overrides parent
  └────────────────────────────┘

  ┌─ prod.env ─────────────────┐
  │ # rsenv: cloud.env         │  ◄── links to parent
  │ export LOG_LEVEL=error     │  ◄── overrides grandparent
  └────────────────────────────┘
```

**Build result** — `rsenv env build prod.env` merges the chain:

```
  prod.env ──inherits──► cloud.env ──inherits──► base.env

  Merged output (child wins):
  ┌────────────────────────────────────────────┐
  │ export DB_HOST=rds.aws.com   ◄ cloud.env   │
  │ export DB_PORT=5432          ◄ base.env    │
  │ export LOG_LEVEL=error       ◄ prod.env    │
  └────────────────────────────────────────────┘
```

**Key commands**:
- `rsenv env tree` — visualize the hierarchy
- `rsenv env select` — fuzzy-pick an env, write to `.envrc`
- `rsenv env build <file>` — merge and output variables
- `rsenv env envrc <file>` — update the vars section of `dot.envrc`

---

## 4. File Guarding

Guard permanently moves sensitive files to the vault and leaves a symlink behind.
Git sees the symlink, not the secret.

```
  BEFORE                              rsenv guard add config/secrets.yaml
  ═══════                             ═══════════════════════════════════

  Project                              Project                     Vault/guarded
  ┌───────────────────┐                ┌───────────────────┐       ┌───────────────────┐
  │ config/           │                │ config/           │       │ config/           │
  │   secrets.yaml    │   ──guard──►   │   secrets.yaml ──────►   │   secrets.yaml     │
  │   (real file)     │                │   (symlink)       │       │   (real file)     │
  └───────────────────┘                └───────────────────┘       └───────────────────┘

  git tracks: real file                git tracks: symlink          safe, outside git
              (dangerous)                          (harmless)
```

**Dotfile neutralization**: Dotfiles are renamed in the vault to prevent
side effects: `.gitignore` → `dot.gitignore`, `.envrc` → `dot.envrc`.

**Key commands**:
- `rsenv guard add <file>` — move to vault, create symlink
- `rsenv guard list` — show all guarded files
- `rsenv guard restore <file>` — move back to project

---

## 5. SOPS Encryption

Vault contents can be encrypted at rest using SOPS (with GPG or Age).
rsenv uses content-addressed filenames to detect staleness.

```
  Plaintext                    Encrypted
  secrets.env      ──encrypt──►  secrets.env.a1b2c3d4.enc
                                            ^^^^^^^^
                                            SHA-256 hash prefix of plaintext

  Modify secrets.env → hash changes → rsenv detects "stale"
  Re-encrypt         → new hash      → secrets.env.f9e8d7c6.enc
```

**Status categories**:

```
  ┌──────────────────┬──────────────────────────────────┬──────────────┐
  │ Status           │ Meaning                          │ Action       │
  ├──────────────────┼──────────────────────────────────┼──────────────┤
  │ current          │ Hash matches, up-to-date         │ None         │
  │ stale            │ Plaintext changed since encrypt  │ Re-encrypt   │
  │ pending_encrypt  │ No encrypted version exists      │ Encrypt      │
  │ orphaned         │ .enc exists but plaintext gone   │ Can delete   │
  └──────────────────┴──────────────────────────────────┴──────────────┘
```

A pre-commit hook (`rsenv hook install`) blocks commits when files are
stale or unencrypted. Plaintext files are auto-added to `.gitignore`.

---

## Putting It All Together

```
  ┌─────────────────────────────────────────────────────────────────────────┐
  │                          rsenv workflow                                 │
  │                                                                         │
  │  1. rsenv init vault         create vault, link via .envrc symlink      │
  │  2. rsenv env select         pick environment, export variables         │
  │  3. rsenv guard add .env     move secrets to vault (permanent)          │
  │  4. rsenv swap in            swap in dev overrides (temporary)          │
  │  5. rsenv sops encrypt       encrypt vault at rest                      │
  │                                                                         │
  │  ... work ...                                                           │
  │                                                                         │
  │  6. rsenv swap out           restore originals, no traces               │
  │  7. rsenv sops encrypt       re-encrypt if changed                      │
  │                                                                         │
  └─────────────────────────────────────────────────────────────────────────┘

  Defense in depth:
  ├── Vault location ──── secrets live outside project directory/git
  ├── Symlinks ────────── git commits harmless symlinks, not secrets
  ├── SOPS encryption ─── vault contents encrypted at rest
  └── .gitignore sync ─── plaintext auto-ignored by git
```

[See all features in the wiki](https://github.com/sysid/rs-env/wiki)

## Quick Start

```bash
# macOS (Homebrew)
brew tap sysid/rsenv
brew install rsenv

# Or via Cargo
cargo install rsenv

rsenv init vault            # Create vault for project
rsenv guard add .env        # Move .env to vault, create symlink
rsenv env tree              # View environment hierarchy
rsenv env select            # Interactive environment selection
```

[Full quick start guide](https://github.com/sysid/rs-env/wiki/Quick-Start)

[Full command reference](https://github.com/sysid/rs-env/wiki/Command-Reference)

## Documentation

**Getting Started**: [Installation](https://github.com/sysid/rs-env/wiki/Installation) · [Quick Start](https://github.com/sysid/rs-env/wiki/Quick-Start) · [Core Concepts](https://github.com/sysid/rs-env/wiki/Core-Concepts)

**Features**: [Environment Variables](https://github.com/sysid/rs-env/wiki/Environment-Variables) · [Vault Management](https://github.com/sysid/rs-env/wiki/Vault-Management) · [File Swapping](https://github.com/sysid/rs-env/wiki/File-Swapping) · [SOPS Encryption](https://github.com/sysid/rs-env/wiki/SOPS-Encryption)

**Reference**: [Commands](https://github.com/sysid/rs-env/wiki/Command-Reference) · [Configuration](https://github.com/sysid/rs-env/wiki/Configuration) · [Troubleshooting](https://github.com/sysid/rs-env/wiki/Troubleshooting) · [Migration Guide](https://github.com/sysid/rs-env/wiki/MIGRATION)

## License

BSD-3-Clause
