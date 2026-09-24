# TODO

Triaged 2026-09-24. The invariant mismatches recorded on 2026-07-16 were resolved by
amending `AGENTS.md` rather than the code. Items judged not worth doing are listed at the
bottom so they are not rediscovered.

## 1. Delete `ServiceContainer`

`infrastructure/di/service_container.rs` is still the "Phase 0" skeleton: it holds only
`settings`, `fs`, `cmd`, it caches nothing despite its doc comment, and it has no
production call sites. It is referenced only by the `lib.rs` doc example (`lib.rs:20,23`)
and a re-export (`lib.rs:36`). The real composition happens in the `main.rs` handlers.
Delete the module, the `di` mod and the `lib.rs` references.

## 2. Swap: a path given by its vault name is used unnormalized

One root cause, two symptoms. `swap_in` records `SwapFile { project_path: project_file }`
(`swap.rs:635`) from the path the user passed, but `restore_dotfiles` (`swap.rs:633`) may
already have renamed it.

- **Directory, data loss:** `rsenv swap in dot.x` moves the content to `dot.x`, the dir
  branch of `restore_dotfiles` renames the root to `.x`, but `dot.x` is recorded. A later
  `swap out dot.x` finds nothing, silently loses the override and leaves `.x` orphaned.
  Tab completion against the vault offers exactly this neutralized name.
- **File, wrong report:** `status_impl` maps vault→project with `restore_path`
  unconditionally (`swap.rs:1053,1079`), but the file branch of `restore_dotfiles` only
  renames when `is_dotfile(name)`, which is false for `dot.gitignore`. The project file
  keeps the name `dot.gitignore` while `status` claims `.gitignore`. `swap diff` works
  around this by probing the disk.

Fix: normalize the user-given path to its project-side name once at the entry point, so
record, rename and status agree. Write tests for both symptoms first.

## 3. Swap: `neutralize_dotfiles` can overwrite data

The directory branch does `fs.rename(".foo", "dot.foo")` (`swap.rs:302`). On Unix `rename`
silently replaces an existing destination, so a directory containing both `.foo` and a
literal `dot.foo` loses the latter on `swap init`/`swap out`. The single-file path already
guards against this ("vault already has file"). Apply the same guard and fail with an error.

---

## Won't do

| Item | Reason |
|---|---|
| Add `read_dir` to `FileSystem` and migrate the 16 walkdir sites | No mock filesystem exists, so the trait buys no testability. `AGENTS.md` now allows walkdir for enumeration. |
| Mock `FileSystem` | 21 methods. `TempDir` + `RealFileSystem` tests real syscall behaviour and works. |
| Route the `$EDITOR` calls in `main.rs` through `CommandRunner::run_interactive` | CLI layer, out of scope of the invariant; nothing tests it. |
| Route `main.rs` `std::fs` calls (scaffolding, hook install, permissions) through `FileSystem` | CLI layer, out of scope. |
| `@@` in a filename breaks sentinel parsing (`parse_sentinel_name`) | Nobody names files like that. If it ever matters, reject such names at `swap init`. |
| Non-UTF-8 filenames mangled by `restore_path` | Linux-only, no real use case. APFS rejects them. |
