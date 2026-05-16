# rsenv
Rust CLI that unifies hierarchical env-var management, file guarding (symlink-to-vault), and file swap-in/out around a per-project "vault" stored outside the repo.

## Setup
- The Rust crate lives in `rsenv/`, not the repo root. ALL `cargo` invocations must `cd rsenv` first (the Makefile does this; raw `cargo` at repo root will not find the manifest).
- The repo's own `.envrc` is a symlink into a real rsenv vault under `~/.rsenv/vaults/`. Do NOT replace it with file content or `git add` through it.

## Commands
- Tests (mandatory flag): `cd rsenv && cargo test -- --test-threads=1`. Single test: append the test name before `--`.
- Verbose tests with logs: `make test-verbose` (sets `RUST_LOG=DEBUG --nocapture`).
- Lint pipeline (auto-fixes): `make lint` runs `cargo clippy --fix` then `cargo fix --tests`. Use `make check` for a no-codegen compile check.
- Release: use `make bump-{major,minor,patch,prenum,pre}` or `make release`. These call `bump-my-version`, which rewrites BOTH `VERSION` and `rsenv/Cargo.toml` and creates the tag.

## Architectural Invariants
- All external I/O goes through traits defined in `rsenv/src/infrastructure/traits.rs` (`FileSystem`, `CommandRunner`, `Clipboard`). Services are concrete structs and MUST NOT call `std::fs` / `std::process` directly — that breaks testability with the mock implementations.
- Error layering is strict and one-way: `DomainError → ApplicationError → InfraError → CliError`. NEVER let a lower layer depend on a higher one.
- `ServiceContainer` (`infrastructure/di`) is the single composition root. NEVER introduce globals, `lazy_static` services, or hand-rolled singletons.
- The `# rsenv: parent.env` directive at the top of env files is the v1 wire format. It is locked for backward compatibility — do NOT change its syntax or parsing semantics.

## Strict Antipatterns
- NEVER run `cargo test` without `--test-threads=1`. Tests mutate real tempdirs and rely on per-test process state; parallel runs flake non-deterministically.
- NEVER hand-edit `VERSION` or the `version =` line in `rsenv/Cargo.toml`. Use `make bump-*`; the two files MUST stay in sync.
- NEVER commit a plaintext file that lives inside a vault's `guarded/` tree — those are auto-added to `.gitignore` by `rsenv guard add` for a reason.

## Gotchas
- Dotfiles inside the vault are renamed on entry: `.envrc → dot.envrc`, `.gitignore → dot.gitignore`. Code that walks the vault or constructs vault paths must use the renamed form.
- Swap sentinel filenames embed the current hostname: `<file>.<hostname>.rsenv_active`. Tests that fabricate swap state must compute the same hostname.
- Encrypted files are content-addressed: `<name>.<sha256-prefix>.enc`. Re-encryption produces a NEW filename; the old one becomes stale and must be removed.
- The `target/` directory under `rsenv/` is the only build output; nothing is generated at repo root.

## Domain glossary
- **Vault** — sentinel-identified directory outside the project (default `~/.rsenv/vaults/<name>-<id>/`) that holds env files, guarded secrets, and swap state.
- **Guard** — permanently move a file into the vault and leave a symlink in the project (reversible only via `guard restore`).
- **Swap** — temporarily replace a project file with a vault copy; the original is backed up to `vault/swap/<rel>.rsenv_original`.
- **Stale** (SOPS) — the plaintext's current SHA-256 no longer matches the hash baked into the encrypted filename.

## When in doubt
- `README.md` has the conceptual diagrams for vault / swap / guard / SOPS.
- `CLAUDE.md` carries the project-specific style and TDD rules and points at the Rust styleguide.
- Feature-level documentation lives in the GitHub wiki (`rs-env.wiki`).
