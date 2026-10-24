# rsenv
Rust CLI that unifies hierarchical env-var management, file guarding (symlink-to-vault), and file swap-in/out around a per-project "vault" stored outside the repo.

## Setup
- The Rust crate lives in `rsenv/`, not the repo root. ALL `cargo` invocations must `cd rsenv` first (the Makefile does this; raw `cargo` at repo root will not find the manifest).
- The repo's own `.envrc` is a symlink into a real rsenv vault under `~/.rsenv/vaults/`. Do NOT replace it with file content or `git add` through it.

## Commands
- Tests (mandatory flag): `cd rsenv && cargo test -- --test-threads=1`. Single test: append the test name before `--`.
- Verbose tests with logs: `make test-verbose` (sets `RUST_LOG=DEBUG --nocapture`).
- Lint pipeline (auto-fixes): `make lint` runs `cargo clippy --fix` then `cargo fix --tests`. Use `make check` for a no-codegen compile check.
- Release: use `make bump-{major,minor,patch,prenum,pre}` or `make release`. These call `bump-my-version`, which rewrites `VERSION`, `rsenv/Cargo.toml` and the `rsenv` entry in `rsenv/Cargo.lock` in one commit and creates the tag.

## Architectural Invariants
- I/O traits live in `rsenv/src/infrastructure/traits.rs`: `FileSystem`, `CommandRunner`, `Selector`, `Editor`. Services (`application/services/`) are concrete structs that receive them as `Arc<dyn …>` and MUST NOT call `std::fs` / `std::process` for anything the traits cover. Exception: directory enumeration uses `walkdir` directly, because `FileSystem` has no `read_dir`. `main.rs` (CLI layer) is out of scope.
- Tests exercise `FileSystem` against a real `TempDir` with `RealFileSystem`; there is no mock filesystem, by choice. Only `Selector` and `Editor` are mocked (`tests/selector_test.rs`, `tests/editor_test.rs`), plus a test-local `FakeSops` `CommandRunner` in `tests/sops_service_test.rs`, because real sops needs a key the test environment does not have.
- Error layering is strict and one-way: `DomainError → ApplicationError → InfraError → CliError`. NEVER let a lower layer depend on a higher one.
- Services are constructed in the `main.rs` command handlers, which inject their dependencies. NEVER introduce globals, `lazy_static` services, or hand-rolled singletons.
- The `# rsenv: parent.env` directive at the top of env files is the v1 wire format. It is locked for backward compatibility — do NOT change its syntax or parsing semantics.

## Strict Antipatterns
- NEVER run `cargo test` without `--test-threads=1`. Tests mutate real tempdirs and rely on per-test process state; parallel runs flake non-deterministically.
- NEVER read a full-suite result without `--no-fail-fast`: `cargo test` stops at the first failing test binary and silently skips the rest, so one environment-specific failure (e.g. `hook_test` in a sandbox that denies `.git/hooks`) hides every later suite.
- NEVER hand-edit `VERSION` or the `version =` line in `rsenv/Cargo.toml`. Use `make bump-*`; the three files MUST stay in sync (`make test` runs `--locked`, so a stale lockfile fails).
- NEVER commit a plaintext file that lives inside a vault's `guarded/` tree — `rsenv guard add` does NOT gitignore them, and the gitignores only cover SOPS name patterns (`*.env`, `*.envrc`, configured file names), so any other guarded name is visible to `git add`.

## Gitignore tiers
- `<base_dir>/.gitignore` holds the global SOPS patterns; `<vault>/.gitignore` holds ONLY the vault `.rsenv.toml`'s own items (no defaults, no global copies), and git stacks the two. Vault `!x` MUST become a git un-ignore `!*.x`: omitting it would not lift the parent rule. Keep tier 2 a pure function of the vault file, so a global config change never fans out to every vault.
- Local config is read ONLY from a discovered vault, never from the cwd: `sops --dir X` must not change behaviour depending on where it runs.
- A plain `--dir` repo (outside `<base_dir>`) has no parent `.gitignore`, so it gets ALL global patterns. `sops encrypt --dir` syncs it first only if an rsenv-managed section already exists (opt-in); never create or touch a foreign `.gitignore` otherwise.

## Gotchas
- Dotfiles inside the vault are renamed on entry: `.envrc → dot.envrc`, `.gitignore → dot.gitignore`. Code that walks the vault or constructs vault paths must use the renamed form.
- Swap sentinel filenames embed the current hostname: `<file>@@<hostname>@@rsenv_active` (note the `@@` separators, and that `<file>` is the NEUTRALIZED name, e.g. `dot.claude`). Tests that fabricate swap state must compute the same hostname.
- A sentinel's *contents* are neutralized; a `*.rsenv_original` backup's contents are NOT. Anything mapping vault paths back to project paths must use `restore_path` and only in that direction — `neutralize_name` is not injective (`.foo` and a literal `dot.foo` both map to `dot.foo`).
- Encrypted files are content-addressed: `<name>.<sha256-prefix>.enc`. Re-encryption produces a NEW filename; the old one becomes stale and must be removed.
- `swap_in`/`swap_out` bump the vault `dot.envrc`'s **mtime** on purpose (`SwapService::notify_direnv`) — that is the only way a swap in one shell refreshes `RSENV_SWAPPED` in another. Never write its *bytes*: the content hash is baked into the `.enc` filename and into `direnv allow`.
- The `target/` directory under `rsenv/` is the only build output; nothing is generated at repo root.

## Domain glossary
- **Vault** — sentinel-identified directory outside the project (default `~/.rsenv/vaults/<name>-<id>/`) that holds env files, guarded secrets, and swap state.
- **Guard** — permanently move a file into the vault and leave a symlink in the project (reversible only via `guard restore`; `guard delete` destroys vault copy and link).
- **Swap** — temporarily replace a project file with a vault copy; the original is backed up to `vault/swap/<rel>.rsenv_original`.
- **Stale** (SOPS) — the plaintext's current SHA-256 no longer matches the hash baked into the encrypted filename.

## When in doubt
- `README.md` has the conceptual diagrams for vault / swap / guard / SOPS.
- `CLAUDE.md` carries the project-specific style and TDD rules and points at the Rust styleguide.
- Feature-level documentation lives in the GitHub wiki (`rs-env.wiki`).
