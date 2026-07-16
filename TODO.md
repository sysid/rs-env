# TODO — unimplemented AGENTS.md invariants

Recorded 2026-07-16.

Two invariants in `AGENTS.md` are stated as absolutes (`MUST NOT`, `NEVER`, "single
composition root") but are not implemented in the code. They are violated in every
service, and in most cases the trait layer offers no way to comply.

This file records what is actually true, so the next reader trusts the code over the
prose. **Nothing here says the code is wrong.** For most items the honest fix may be to
amend `AGENTS.md`, not to change the code — see the decisions at the bottom.

---

## 1. "All external I/O goes through traits" — not implemented

> All external I/O goes through traits defined in `rsenv/src/infrastructure/traits.rs`
> (`FileSystem`, `CommandRunner`, `Clipboard`). Services are concrete structs and MUST NOT
> call `std::fs` / `std::process` directly — that breaks testability with the mock
> implementations.

Three separate problems: the trait list is wrong, the stated rationale does not exist, and
the trait is missing the one method that would make compliance possible.

### 1a. The named traits do not match the code

| AGENTS.md says | Reality (`infrastructure/traits.rs`) |
|---|---|
| `FileSystem` | Exists — `traits.rs:11` |
| `CommandRunner` | Exists — `traits.rs:81` |
| `Clipboard` | **Does not exist anywhere in the repo** |
| — | `Selector` (`traits.rs:99`) — real, unlisted |
| — | `Editor` (`traits.rs:110`) — real, unlisted |

### 1b. "the mock implementations" do not exist for `FileSystem`

`impl FileSystem for RealFileSystem` (`traits.rs:124`) is the **only** implementation in
`src/` or `tests/`. There is no mock, fake, or in-memory filesystem. Every test builds a
real `TempDir` and uses `RealFileSystem` — and that works fine, with 361 tests passing.

The mock pattern *was* applied, but only to the two small traits:
- `MockEditor` — `tests/editor_test.rs:10`
- `MockSelector` — `tests/selector_test.rs:12`

So the rule's justification ("breaks testability with the mock implementations") describes
a testing strategy this project does not use for `FileSystem`. A mock would mean
implementing all 21 trait methods.

### 1c. Root cause: `FileSystem` cannot enumerate a directory

The trait has 21 methods and **not one lists a directory** — no `read_dir`, no `walk`.
Directory enumeration is therefore *impossible* through the trait. Every site below is not
a shortcut; it is the only option available.

`environment.rs:204` already admits this in a code comment:
> `// Skip if not a file (use WalkDir entry method, not filesystem syscall)`

**Enumeration sites — blocked on the missing trait method (16):**

| File | Lines | Context |
|---|---|---|
| `application/services/swap.rs` | 147, 188, 246, 308, 928, 1027, 1083 | sentinel scan, swap in/out, vault-wide sweeps |
| `application/services/sops.rs` | 68, 104, 343 | 343 uses `std::fs::read_dir` directly, not WalkDir |
| `application/services/environment.rs` | 197, 252, 470 | `get_hierarchy`, `init_files`, `is_dag` |
| `application/services/vault.rs` | 754 | `reset` — collects guarded files |
| `main.rs` | 861, 926 | guarded-file listing |

Adding `read_dir` to `FileSystem` would unblock all 16 at once.

### 1d. `CommandRunner` cannot express interactive commands

Not a missing-method problem — a shape problem. `CommandRunner::run` (`traits.rs:83`)
returns `io::Result<Output>`, which **captures** stdout/stderr. An editor must *inherit*
the terminal, so it needs `.status()`, which the trait cannot express. Hence:

| File | Line | Call |
|---|---|---|
| `main.rs` | 374 | `std::process::Command::new(&editor_cmd)` — `$EDITOR -O`, needs TTY |
| `main.rs` | 447 | `std::process::Command::new("vim")` — `vim -S`, needs TTY |

`CommandRunner` is used in exactly one place: `application/services/sops.rs`. `main.rs`
bypasses it entirely. These two sites are bypassing it **for a real reason**, not laziness.

### 1e. Direct `std::fs` in `main.rs` where the trait *would* suffice

Unlike the sites above, these are not forced — `FileSystem` has `write`, `read_to_string`,
`create_dir_all`, `remove_file`. They just do not use it.

| File | Lines | Context |
|---|---|---|
| `main.rs` | 510, 525, 561, 628, 635, 658, 667 | config scaffolding / templates |
| `main.rs` | 1494, 1504, 1551 | pre-commit hook install |
| `main.rs` | 1517, 1528 | `std::fs::metadata` + `set_permissions` — **no trait method exists** (second capability gap, same shape as 1c) |

Note the scope ambiguity: the rule's second sentence targets *"Services"*, and `main.rs` is
the CLI layer, not a service. But the first sentence says *"All external I/O"*. Whether
`main.rs` is in scope is undecided — see decision 2.

---

## 2. "ServiceContainer is the single composition root" — not implemented

> `ServiceContainer` (`infrastructure/di`) is the single composition root. NEVER introduce
> globals, `lazy_static` services, or hand-rolled singletons.

The prohibition half **holds** — there are no globals, no `lazy_static`, no singletons.
The "single composition root" half is entirely aspirational:

- `infrastructure/di/service_container.rs:13` says so itself: *"In Phase 0, this is a
  skeleton that will be populated as services are implemented."*
- It holds only `settings`, `fs`, `cmd`. Every service field is commented out, tagged
  `// Phase 1` … `// Phase 4`.
- Its doc comment claims *"Services are created lazily and cached"* — nothing is cached;
  there are no services to cache.
- **Zero production call sites.** It appears only in a `lib.rs` doc-comment example
  (`lib.rs:20,23`) and a re-export (`lib.rs:36`).
- The real composition root is each `main.rs` handler, constructing services ad hoc:
  `EnvironmentService::new(Arc::new(RealFileSystem))`.

Consequence for contributors: following the stated invariant is impossible today. Wiring a
new service through `ServiceContainer` would make it the *only* service there, and every
handler would still construct its own.

---

## Decisions needed

Each of these is a fork between *fix the code* and *fix the doc*. They are listed roughly
in dependency order.

1. **Add `read_dir` to `FileSystem`?** Unblocks all 16 enumeration sites and is the single
   highest-leverage change here. Alternative: amend the invariant to explicitly permit
   `walkdir` for enumeration, which is what the code already does consistently.
2. **Does the invariant cover `main.rs`, or only `application/services/`?** Decides whether
   §1e is 12 violations or 12 non-issues.
3. **Extend `CommandRunner` for interactive commands** (e.g. a `status()`-style method that
   inherits the TTY), or document the editor sites as a permanent exemption.
4. **Add a permissions API to `FileSystem`** for `main.rs:1517,1528`, or exempt it.
5. **Is a mock `FileSystem` actually wanted?** 21 methods. Today's `TempDir` +
   `RealFileSystem` approach works and is arguably better (it tests real syscall
   behaviour). If the answer is no, the rule's rationale in `AGENTS.md` should be retired —
   it currently justifies a rule with a benefit the project does not collect.
6. **`ServiceContainer`: wire it or delete it.** It has been "Phase 0" long enough to be
   dead code with an aspirational doc comment. Either is defensible; the status quo teaches
   contributors a rule the codebase does not follow.
7. **Correct the `AGENTS.md` trait list** regardless of the above: drop `Clipboard`, add
   `Selector` and `Editor`.

## Why this matters

An invariant that the codebase violates in every service is worse than no invariant. It
trains readers to discount `AGENTS.md`, which devalues the rules in it that *are* real and
load-bearing — the strict one-way error layering, the locked `# rsenv:` v1 wire format, the
mandatory `--test-threads=1`. Those are followed. These two are not, and the gap should be
closed from one end or the other.
