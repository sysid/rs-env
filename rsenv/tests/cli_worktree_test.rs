//! rsenv in linked git worktrees.
//!
//! A vault holds one swapped-in copy per host, and that copy lives in the main
//! checkout. A linked worktree reaches the same vault through its `.envrc` and borrows
//! main's swapped-in files through symlinks. Commands that change the vault must
//! therefore run in main; run in a worktree they corrupt the vault or silently do
//! nothing. See thoughts/designs/2026-11-15-worktree-support.md.

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use rstest::rstest;
use tempfile::TempDir;
use walkdir::WalkDir;

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.com")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.com")
        .output()
        .expect("run git");
    assert!(out.status.success(), "git {args:?}: {out:?}");
}

struct Fixture {
    _temp: TempDir,
    home: PathBuf,
    base_dir: PathBuf,
    /// Main checkout: vault initialized (default relative `.envrc`), `thoughts`
    /// swapped in, `secret.txt` guarded.
    main: PathBuf,
    /// Linked worktree of `main`.
    wt: PathBuf,
    vault: PathBuf,
}

impl Fixture {
    fn rsenv(&self, cwd: &Path, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_rsenv"))
            .args(args)
            .current_dir(cwd)
            .env("HOME", &self.home)
            .env_remove("XDG_CONFIG_HOME")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .expect("run rsenv")
    }

    fn rsenv_ok(&self, cwd: &Path, args: &[&str]) -> Output {
        let out = self.rsenv(cwd, args);
        assert!(out.status.success(), "rsenv {args:?}: {out:?}");
        out
    }

    /// Global config with `[worktree] borrow` set to `paths`.
    fn set_global_borrow(&self, paths: &[&str]) {
        let list: Vec<String> = paths.iter().map(|p| format!("\"{p}\"")).collect();
        fs::write(
            self.home
                .join("Library/Application Support/rsenv/rsenv.toml"),
            format!(
                "base_dir = \"{}\"\n[worktree]\nborrow = [{}]\n",
                self.base_dir.display(),
                list.join(", ")
            ),
        )
        .unwrap();
    }

    /// What `rsenv worktree attach` will do; done by hand here so the guards can be
    /// tested on their own.
    fn attach_by_hand(&self) {
        symlink(self.vault.join("dot.envrc"), self.wt.join(".envrc")).unwrap();
        symlink(self.main.join("thoughts"), self.wt.join("thoughts")).unwrap();
        symlink(self.main.join("secret.txt"), self.wt.join("secret.txt")).unwrap();
    }
}

/// Main checkout with a vault, one swap entry swapped in, one guarded file, and a
/// linked worktree next to it (`<repo>__worktrees/<branch>` layout).
fn fixture() -> Fixture {
    let temp = TempDir::new().unwrap();
    // Canonical: macOS temp dirs live behind the /var -> /private/var symlink.
    let root = fs::canonicalize(temp.path()).unwrap();
    let home = root.join("home");
    let base_dir = root.join("base");
    fs::create_dir_all(&base_dir).unwrap();
    let config_dir = home.join("Library/Application Support/rsenv");
    fs::create_dir_all(&config_dir).unwrap();
    fs::write(
        config_dir.join("rsenv.toml"),
        format!("base_dir = \"{}\"\n", base_dir.display()),
    )
    .unwrap();

    let main = root.join("proj");
    fs::create_dir_all(&main).unwrap();
    git(&main, &["init", "-q", "--template=", "-b", "main"]);
    fs::write(main.join(".gitignore"), "/.envrc\n/thoughts\n/secret.txt\n").unwrap();
    git(&main, &["add", ".gitignore"]);
    git(&main, &["commit", "-q", "-m", "init"]);

    let mut fx = Fixture {
        _temp: temp,
        home,
        base_dir,
        main: main.clone(),
        wt: root.join("proj__worktrees/feat"),
        vault: PathBuf::new(),
    };

    fx.rsenv_ok(&main, &["vault", "init"]);
    fx.vault = fs::canonicalize(main.join(".envrc"))
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();

    fs::create_dir_all(main.join("thoughts")).unwrap();
    fs::write(main.join("thoughts/note.md"), "note\n").unwrap();
    fx.rsenv_ok(&main, &["swap", "init", "thoughts"]);
    fx.rsenv_ok(&main, &["swap", "in", "thoughts"]);

    fs::write(main.join("secret.txt"), "s3cret\n").unwrap();
    fx.rsenv_ok(&main, &["guard", "add", "secret.txt"]);

    git(
        &main,
        &[
            "worktree",
            "add",
            "-q",
            "../proj__worktrees/feat",
            "-b",
            "feat",
        ],
    );
    fx
}

/// Every entry under `dir`: symlinks by target, files by content. Two equal
/// snapshots mean nothing was moved, renamed, created or rewritten.
fn snapshot(dir: &Path) -> BTreeMap<PathBuf, String> {
    WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .map(|e| {
            let rel = e.path().strip_prefix(dir).unwrap().to_path_buf();
            let what = if e.path_is_symlink() {
                format!("-> {}", fs::read_link(e.path()).unwrap().display())
            } else if e.file_type().is_dir() {
                "dir".to_string()
            } else {
                String::from_utf8_lossy(&fs::read(e.path()).unwrap()).into_owned()
            };
            (rel, what)
        })
        .collect()
}

// ============================================================
// Slice 1: vault-changing commands are refused in a linked worktree
// ============================================================

#[rstest]
#[case::swap_in(&["swap", "in", "thoughts"])]
#[case::swap_out_all(&["swap", "out"])]
#[case::swap_out_path(&["swap", "out", "thoughts"])]
#[case::swap_init(&["swap", "init", "local.txt"])]
#[case::swap_delete(&["swap", "delete", "-y", "thoughts"])]
#[case::guard_add(&["guard", "add", "local.txt"])]
#[case::guard_restore(&["guard", "restore", "secret.txt"])]
#[case::guard_delete(&["guard", "delete", "-y", "secret.txt"])]
#[case::vault_init(&["vault", "init"])]
#[case::vault_reset(&["vault", "reset"])]
#[case::vault_commit(&["vault", "commit", "--auto"])]
fn given_linked_worktree_when_vault_changing_command_then_refused_and_nothing_changes(
    #[case] args: &[&str],
) {
    // Arrange
    let fx = fixture();
    fx.attach_by_hand();
    fs::write(fx.wt.join("local.txt"), "local\n").unwrap();
    let vault_before = snapshot(&fx.vault);
    let main_before = snapshot(&fx.main);
    let wt_before = snapshot(&fx.wt);

    // Act
    let out = fx.rsenv(&fx.wt, args);

    // Assert
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(77), "{args:?}: {out:?}");
    assert!(stderr.contains("linked git worktree"), "{args:?}: {stderr}");
    assert!(
        stderr.contains("run this there") && stderr.contains(&format!("{} ", fx.main.display())),
        "must say where to run it instead: {stderr}"
    );
    assert_eq!(snapshot(&fx.vault), vault_before, "{args:?}: vault changed");
    assert_eq!(snapshot(&fx.main), main_before, "{args:?}: main changed");
    assert_eq!(snapshot(&fx.wt), wt_before, "{args:?}: worktree changed");
}

/// `vault reconnect` from a worktree would rewrite the vault's sourceDir to the
/// worktree; the vault would then follow the worktree instead of main.
#[test]
fn given_linked_worktree_when_vault_reconnect_then_refused_and_source_dir_kept() {
    // Arrange
    let fx = fixture();
    fx.attach_by_hand();
    let dot_envrc = fx.vault.join("dot.envrc");
    let before = fs::read_to_string(&dot_envrc).unwrap();

    // Act
    let out = fx.rsenv(&fx.wt, &["vault", "reconnect", dot_envrc.to_str().unwrap()]);

    // Assert
    assert_eq!(out.status.code(), Some(77), "{out:?}");
    assert_eq!(fs::read_to_string(&dot_envrc).unwrap(), before);
}

/// The guard reads git state of the worktree, not of the cwd: `-C <worktree>` from
/// main is refused just the same.
#[test]
fn given_dash_c_pointing_at_linked_worktree_when_swap_out_then_refused() {
    // Arrange
    let fx = fixture();
    fx.attach_by_hand();

    // Act
    let out = fx.rsenv(&fx.main, &["-C", fx.wt.to_str().unwrap(), "swap", "out"]);

    // Assert
    assert_eq!(out.status.code(), Some(77), "{out:?}");
    assert!(fx.wt.join("thoughts").is_symlink());
}

#[rstest]
#[case::swap_status(&["swap", "status"])]
#[case::guard_list(&["guard", "list"])]
#[case::info(&["info"])]
#[case::info_check(&["info", "--check"])]
fn given_linked_worktree_when_read_only_command_then_allowed(#[case] args: &[&str]) {
    // Arrange
    let fx = fixture();
    fx.attach_by_hand();

    // Act
    let out = fx.rsenv(&fx.wt, args);

    // Assert
    assert!(out.status.success(), "{args:?}: {out:?}");
}

/// `--global` walks the vaults through their recorded sourceDir, never through the
/// cwd, so where it is typed does not matter.
#[test]
fn given_linked_worktree_when_swap_out_global_then_allowed_and_main_swapped_out() {
    // Arrange
    let fx = fixture();
    fx.attach_by_hand();

    // Act
    let out = fx.rsenv(&fx.wt, &["swap", "out", "--global"]);

    // Assert
    assert!(out.status.success(), "{out:?}");
    assert!(!fx.main.join("thoughts").exists(), "main is swapped out");
}

#[test]
fn given_main_checkout_without_borrowers_when_swap_out_then_allowed() {
    // Arrange
    let fx = fixture();

    // Act
    let out = fx.rsenv(&fx.main, &["swap", "out", "thoughts"]);

    // Assert
    assert!(out.status.success(), "{out:?}");
    assert!(!fx.main.join("thoughts").exists());
}

// ============================================================
// Slice 2: swap-out in main while linked worktrees borrow
// ============================================================

#[rstest]
#[case::swap_out_all(&["swap", "out"])]
#[case::swap_out_path(&["swap", "out", "thoughts"])]
#[case::swap_delete(&["swap", "delete", "-y", "thoughts"])]
#[case::vault_commit(&["vault", "commit", "--auto"])]
fn given_worktree_borrows_entry_when_swapping_it_out_in_main_then_refused(#[case] args: &[&str]) {
    // Arrange
    let fx = fixture();
    fx.attach_by_hand();
    let vault_before = snapshot(&fx.vault);
    let main_before = snapshot(&fx.main);

    // Act
    let out = fx.rsenv(&fx.main, args);

    // Assert
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(77), "{args:?}: {out:?}");
    assert!(
        stderr.contains(&fx.wt.display().to_string()),
        "names the worktree: {stderr}"
    );
    assert!(stderr.contains("thoughts"), "names the entry: {stderr}");
    assert!(
        stderr.contains("rsenv worktree detach") && stderr.contains("--allow-borrowed"),
        "names both ways out: {stderr}"
    );
    assert_eq!(snapshot(&fx.vault), vault_before, "{args:?}: vault changed");
    assert_eq!(snapshot(&fx.main), main_before, "{args:?}: main changed");
}

#[rstest]
#[case::swap_out_path(&["swap", "out", "--allow-borrowed", "thoughts"])]
#[case::swap_delete(&["swap", "delete", "-y", "--allow-borrowed", "thoughts"])]
fn given_worktree_borrows_entry_when_allow_borrowed_then_proceeds_and_link_dangles(
    #[case] args: &[&str],
) {
    // Arrange
    let fx = fixture();
    fx.attach_by_hand();

    // Act
    let out = fx.rsenv(&fx.main, args);

    // Assert
    assert!(out.status.success(), "{args:?}: {out:?}");
    assert!(fx.wt.join("thoughts").is_symlink());
    assert!(!fx.wt.join("thoughts").exists(), "link dangles");
}

/// Only the entries being swapped out matter: the worktree borrowing the guarded
/// `secret.txt` does not block swapping out `thoughts`.
#[test]
fn given_worktree_borrows_other_path_when_swap_out_then_allowed() {
    // Arrange
    let fx = fixture();
    symlink(fx.main.join("secret.txt"), fx.wt.join("secret.txt")).unwrap();

    // Act
    let out = fx.rsenv(&fx.main, &["swap", "out", "thoughts"]);

    // Assert
    assert!(out.status.success(), "{out:?}");
}

#[test]
fn given_worktree_with_own_copy_when_swap_out_then_allowed() {
    // Arrange
    let fx = fixture();
    fs::create_dir_all(fx.wt.join("thoughts")).unwrap();

    // Act
    let out = fx.rsenv(&fx.main, &["swap", "out", "thoughts"]);

    // Assert
    assert!(out.status.success(), "{out:?}");
}

/// `--global` runs unattended (e.g. before backing up vaults): a refusal would
/// silently skip a vault, which is worse than dangling links that revive on the
/// next swap-in. It warns instead.
#[test]
fn given_worktree_borrows_entry_when_swap_out_global_then_warns_and_proceeds() {
    // Arrange
    let fx = fixture();
    fx.attach_by_hand();

    // Act
    let out = fx.rsenv(&fx.main, &["swap", "out", "--global"]);

    // Assert
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{out:?}");
    assert!(
        stderr.contains(&fx.wt.display().to_string()) && stderr.contains("thoughts"),
        "warns about the borrower: {stderr}"
    );
    assert!(!fx.main.join("thoughts").exists(), "main is swapped out");
}

/// `info` counts guarded files, not files in `guarded/`: a ciphertext next to its
/// plaintext is the same guarded file.
#[test]
fn given_guarded_file_with_ciphertext_when_info_then_counts_one() {
    // Arrange
    let fx = fixture();
    fs::write(
        fx.vault.join("guarded/secret.txt.a1b2c3d4.enc"),
        "ciphertext",
    )
    .unwrap();

    // Act
    let out = fx.rsenv_ok(&fx.main, &["info"]);

    // Assert
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Guarded: 1 file"), "{stdout}");
}

// ============================================================
// Slice 3: rsenv worktree attach | detach
// ============================================================

fn git_out(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("run git");
    assert!(out.status.success(), "git {args:?}: {out:?}");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// The daily flow: inside the fresh worktree, no arguments.
#[test]
fn given_fresh_worktree_when_attach_then_envrc_and_borrowed_paths_linked_and_git_clean() {
    // Arrange
    let fx = fixture();

    // Act
    let out = fx.rsenv(&fx.wt, &["worktree", "attach"]);

    // Assert
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{out:?}");
    assert_eq!(
        fs::read_link(fx.wt.join("thoughts")).unwrap(),
        fx.main.join("thoughts")
    );
    assert_eq!(
        fs::read_link(fx.wt.join("secret.txt")).unwrap(),
        fx.main.join("secret.txt"),
        "guarded file borrowed through main's link"
    );
    assert_eq!(
        fs::read_to_string(fx.wt.join("secret.txt")).unwrap(),
        "s3cret\n"
    );
    assert_eq!(git_out(&fx.wt, &["status", "--porcelain"]), "");
    assert!(
        stdout.contains(&format!("direnv allow {}", fx.wt.display())),
        "{stdout}"
    );
}

/// `vault init` makes a RELATIVE .envrc link; copied into a worktree one level
/// deeper it would point nowhere. attach writes an absolute one.
#[test]
fn given_main_with_relative_envrc_when_attach_then_worktree_envrc_absolute_to_same_vault() {
    // Arrange
    let fx = fixture();
    assert!(fs::read_link(fx.main.join(".envrc")).unwrap().is_relative());

    // Act
    fx.rsenv_ok(&fx.main, &["worktree", "attach", fx.wt.to_str().unwrap()]);

    // Assert
    let link = fs::read_link(fx.wt.join(".envrc")).unwrap();
    assert!(link.is_absolute(), "{link:?}");
    assert_eq!(link, fx.vault.join("dot.envrc"));
}

#[rstest]
#[case::main_checkout(true)]
#[case::plain_dir(false)]
fn given_not_a_linked_worktree_when_attach_then_refused_nothing_written(#[case] in_main: bool) {
    // Arrange
    let fx = fixture();
    let dir = if in_main {
        fx.main.clone()
    } else {
        let d = fx.base_dir.join("plain");
        fs::create_dir_all(&d).unwrap();
        d
    };
    let before = snapshot(&dir);

    // Act
    let out = fx.rsenv(&dir, &["worktree", "attach"]);

    // Assert
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(77), "{out:?}");
    assert!(stderr.contains("not a linked git worktree"), "{stderr}");
    assert_eq!(snapshot(&dir), before);
}

#[test]
fn given_main_without_vault_when_attach_then_refused_nothing_written() {
    // Arrange
    let fx = fixture();
    fs::remove_file(fx.main.join(".envrc")).unwrap();
    let before = snapshot(&fx.wt);

    // Act
    let out = fx.rsenv(&fx.wt, &["worktree", "attach"]);

    // Assert
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(77), "{out:?}");
    assert!(stderr.contains("has no vault"), "{stderr}");
    assert_eq!(snapshot(&fx.wt), before);
}

/// The old `wm()` rule: borrowing a swapped-out entry would hand the worktree the
/// committed original, or nothing.
#[test]
fn given_entry_swapped_out_in_main_when_attach_then_refused_names_entry_and_remedy() {
    // Arrange
    let fx = fixture();
    fx.rsenv_ok(&fx.main, &["swap", "out", "thoughts"]);
    let before = snapshot(&fx.wt);

    // Act
    let out = fx.rsenv(&fx.wt, &["worktree", "attach"]);

    // Assert
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(77), "{out:?}");
    // `swap status` reports a swapped-out directory entry by its files.
    assert!(
        stderr.contains("not swapped in: thoughts/note.md"),
        "{stderr}"
    );
    assert!(
        stderr.contains(&format!("rsenv -C {} swap in", fx.main.display())),
        "{stderr}"
    );
    assert_eq!(snapshot(&fx.wt), before);
}

/// Plan first, then write: one conflict means NOTHING is written, not even .envrc.
#[test]
fn given_worktree_has_own_copy_of_borrowed_path_when_attach_then_conflict_nothing_written() {
    // Arrange
    let fx = fixture();
    fs::create_dir_all(fx.wt.join("thoughts")).unwrap();
    fs::write(fx.wt.join("thoughts/mine.md"), "mine\n").unwrap();
    let before = snapshot(&fx.wt);

    // Act
    let out = fx.rsenv(&fx.wt, &["worktree", "attach"]);

    // Assert
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(77), "{out:?}");
    assert!(stderr.contains("thoughts"), "{stderr}");
    assert_eq!(snapshot(&fx.wt), before);
    assert!(!fx.wt.join(".envrc").exists());
}

#[test]
fn given_attached_worktree_when_attach_again_then_nothing_changes() {
    // Arrange
    let fx = fixture();
    fx.rsenv_ok(&fx.wt, &["worktree", "attach"]);
    let before = snapshot(&fx.wt);

    // Act
    let out = fx.rsenv(&fx.wt, &["worktree", "attach"]);

    // Assert
    assert!(out.status.success(), "{out:?}");
    assert_eq!(snapshot(&fx.wt), before);
}

#[test]
fn given_new_swap_entry_in_main_when_attach_again_then_only_new_entry_linked() {
    // Arrange
    let fx = fixture();
    fx.rsenv_ok(&fx.wt, &["worktree", "attach"]);
    fs::write(fx.main.join("AGENTS.md"), "agents\n").unwrap();
    fx.rsenv_ok(&fx.main, &["swap", "init", "AGENTS.md"]);
    fx.rsenv_ok(&fx.main, &["swap", "in", "AGENTS.md"]);

    // Act
    let out = fx.rsenv_ok(&fx.wt, &["worktree", "attach"]);

    // Assert
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        fs::read_link(fx.wt.join("AGENTS.md")).unwrap(),
        fx.main.join("AGENTS.md")
    );
    assert!(stdout.contains("AGENTS.md"), "{stdout}");
}

/// A path tracked in git exists in the worktree as committed. Replacing it by a link
/// would show as a typechange: the worktree keeps the committed version.
#[test]
fn given_borrow_path_tracked_in_git_when_attach_then_skipped_and_worktree_clean() {
    // Arrange
    let fx = fixture();
    fs::write(fx.main.join("CLAUDE.md"), "committed\n").unwrap();
    git(&fx.main, &["add", "CLAUDE.md"]);
    git(&fx.main, &["commit", "-q", "-m", "claude"]);
    git(&fx.wt, &["merge", "-q", "--ff-only", "main"]);
    fx.set_global_borrow(&["CLAUDE.md"]);

    // Act
    let out = fx.rsenv_ok(&fx.wt, &["worktree", "attach"]);

    // Assert
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!fx.wt.join("CLAUDE.md").is_symlink());
    assert!(
        stdout.contains("CLAUDE.md") && stdout.contains("tracked"),
        "{stdout}"
    );
    assert_eq!(git_out(&fx.wt, &["status", "--porcelain"]), "");
}

#[test]
fn given_configured_extra_present_in_main_when_attach_then_linked() {
    // Arrange
    let fx = fixture();
    fs::create_dir_all(fx.main.join(".claude")).unwrap();
    fx.set_global_borrow(&[".claude"]);

    // Act
    fx.rsenv_ok(&fx.wt, &["worktree", "attach"]);

    // Assert
    assert_eq!(
        fs::read_link(fx.wt.join(".claude")).unwrap(),
        fx.main.join(".claude")
    );
}

#[test]
fn given_configured_extra_missing_in_main_when_attach_then_skipped_with_note() {
    // Arrange
    let fx = fixture();
    fx.set_global_borrow(&[".claude"]);

    // Act
    let out = fx.rsenv_ok(&fx.wt, &["worktree", "attach"]);

    // Assert
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!fx.wt.join(".claude").exists());
    assert!(
        stdout.contains(".claude") && stdout.contains("not in main"),
        "{stdout}"
    );
}

/// Vault config lists add to the global ones; `!x` removes x for this project.
#[test]
fn given_vault_config_negating_global_extra_when_attach_then_not_linked() {
    // Arrange
    let fx = fixture();
    fs::create_dir_all(fx.main.join(".claude")).unwrap();
    fx.set_global_borrow(&[".claude"]);
    fs::write(
        fx.vault.join(".rsenv.toml"),
        "[worktree]\nborrow = [\"!.claude\"]\n",
    )
    .unwrap();

    // Act
    fx.rsenv_ok(&fx.wt, &["worktree", "attach"]);

    // Assert
    assert!(!fx.wt.join(".claude").exists());
}

#[test]
fn given_attached_worktree_when_detach_then_links_removed_own_files_kept() {
    // Arrange
    let fx = fixture();
    fx.rsenv_ok(&fx.wt, &["worktree", "attach"]);
    let elsewhere = fx.base_dir.join("elsewhere");
    fs::create_dir_all(&elsewhere).unwrap();
    symlink(&elsewhere, fx.wt.join("ext")).unwrap();
    fs::write(fx.wt.join("own.txt"), "own\n").unwrap();

    // Act
    let out = fx.rsenv(&fx.wt, &["worktree", "detach"]);

    // Assert
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{out:?}");
    for p in [".envrc", "thoughts", "secret.txt"] {
        assert!(!fx.wt.join(p).is_symlink(), "{p} still there");
    }
    assert!(fx.wt.join("ext").is_symlink(), "link elsewhere kept");
    assert!(fx.wt.join("own.txt").exists());
    assert!(fx.main.join("thoughts/note.md").exists(), "main untouched");
    assert!(
        stdout.contains(&format!("git worktree remove {}", fx.wt.display())),
        "{stdout}"
    );
}

/// Untracked links make `git worktree remove` refuse without --force. After detach
/// the plain remove works, so git's dirty-worktree guard stays in force.
#[test]
fn given_detached_worktree_when_git_worktree_remove_then_succeeds_without_force() {
    // Arrange
    let fx = fixture();
    fs::create_dir_all(fx.main.join(".claude")).unwrap();
    fx.set_global_borrow(&[".claude"]);
    fx.rsenv_ok(&fx.wt, &["worktree", "attach"]);
    fx.rsenv_ok(&fx.main, &["worktree", "detach", fx.wt.to_str().unwrap()]);

    // Act
    git(&fx.main, &["worktree", "remove", fx.wt.to_str().unwrap()]);

    // Assert
    assert!(!fx.wt.exists());
}

/// Detach decides by link target, not by vault state: it still cleans up after the
/// main checkout lost its vault link.
#[test]
fn given_main_lost_its_vault_when_detach_then_links_still_removed() {
    // Arrange
    let fx = fixture();
    fx.rsenv_ok(&fx.wt, &["worktree", "attach"]);
    fs::remove_file(fx.main.join(".envrc")).unwrap();

    // Act
    let out = fx.rsenv(&fx.wt, &["worktree", "detach"]);

    // Assert
    assert!(out.status.success(), "{out:?}");
    for p in [".envrc", "thoughts", "secret.txt"] {
        assert!(!fx.wt.join(p).is_symlink(), "{p} still there");
    }
}

#[test]
fn given_main_checkout_when_detach_then_refused() {
    // Arrange
    let fx = fixture();
    let before = snapshot(&fx.main);

    // Act
    let out = fx.rsenv(&fx.main, &["worktree", "detach"]);

    // Assert
    assert_eq!(out.status.code(), Some(77), "{out:?}");
    assert_eq!(snapshot(&fx.main), before);
}

#[test]
fn given_attached_worktree_when_swap_out_in_main_then_refused() {
    // Arrange
    let fx = fixture();
    fx.rsenv_ok(&fx.wt, &["worktree", "attach"]);

    // Act
    let out = fx.rsenv(&fx.main, &["swap", "out"]);

    // Assert
    assert_eq!(out.status.code(), Some(77), "{out:?}");
}

// ============================================================
// Slice 4: info and swap status know about worktrees
// ============================================================

fn stdout_of(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn given_main_with_attached_worktree_when_info_then_lists_worktree_and_its_borrows() {
    // Arrange
    let fx = fixture();
    fx.rsenv_ok(&fx.wt, &["worktree", "attach"]);

    // Act
    let out = fx.rsenv_ok(&fx.main, &["info"]);

    // Assert
    let stdout = stdout_of(&out);
    assert!(
        stdout.contains("Checkout: main checkout, 1 linked worktree"),
        "{stdout}"
    );
    assert!(stdout.contains("Worktrees:"), "{stdout}");
    let line = stdout
        .lines()
        .find(|l| l.contains(&fx.wt.display().to_string()))
        .unwrap_or_else(|| panic!("no worktree line: {stdout}"));
    assert!(line.contains("feat"), "{line}");
    assert!(line.contains("borrows: secret.txt, thoughts"), "{line}");
    assert!(!stdout.contains("Problems:"), "{stdout}");
}

#[test]
fn given_attached_worktree_when_info_then_says_linked_and_borrowed_without_warnings() {
    // Arrange
    let fx = fixture();
    fx.rsenv_ok(&fx.wt, &["worktree", "attach"]);

    // Act
    let out = fx.rsenv_ok(&fx.wt, &["info"]);

    // Assert
    let stdout = stdout_of(&out);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stdout.contains(&format!(
            "Checkout: linked worktree of {}",
            fx.main.display()
        )),
        "{stdout}"
    );
    assert!(stdout.contains("main's state, borrowed here"), "{stdout}");
    assert!(!stdout.contains("Problems:"), "{stdout}");
    assert!(
        !stderr.contains("out of sync"),
        "no reconnect advice: {stderr}"
    );
}

/// An unattached worktree has no vault of its own; the old advice `rsenv vault init`
/// would create a second vault.
#[test]
fn given_unattached_worktree_when_info_then_suggests_attach_not_vault_init() {
    // Arrange
    let fx = fixture();

    // Act
    let out = fx.rsenv_ok(&fx.wt, &["info"]);

    // Assert
    let stdout = stdout_of(&out);
    assert!(stdout.contains("not-attached"), "{stdout}");
    assert!(stdout.contains("rsenv worktree attach"), "{stdout}");
    assert!(!stdout.contains("vault init"), "{stdout}");
}

#[test]
fn given_main_swapped_out_under_borrowing_worktree_when_info_in_worktree_then_reports_it() {
    // Arrange
    let fx = fixture();
    fx.rsenv_ok(&fx.wt, &["worktree", "attach"]);
    fx.rsenv_ok(&fx.main, &["swap", "out", "--allow-borrowed", "thoughts"]);

    // Act
    let out = fx.rsenv_ok(&fx.wt, &["info"]);

    // Assert
    let stdout = stdout_of(&out);
    assert!(stdout.contains("Problems:"), "{stdout}");
    assert!(stdout.contains("borrowed-not-swapped-in"), "{stdout}");
    assert!(
        stdout.contains(&format!("rsenv -C {} swap in thoughts", fx.main.display())),
        "{stdout}"
    );
}

#[test]
fn given_borrowed_link_dangling_when_info_in_worktree_then_reports_it() {
    // Arrange
    let fx = fixture();
    fs::create_dir_all(fx.main.join(".claude")).unwrap();
    fx.set_global_borrow(&[".claude"]);
    fx.rsenv_ok(&fx.wt, &["worktree", "attach"]);
    fs::remove_dir_all(fx.main.join(".claude")).unwrap();

    // Act
    let out = fx.rsenv_ok(&fx.wt, &["info"]);

    // Assert
    let stdout = stdout_of(&out);
    assert!(stdout.contains("borrowed-link-dangling"), "{stdout}");
    assert!(stdout.contains(".claude"), "{stdout}");
}

/// The research doc's original recipe: `ln -s "$(readlink main/.envrc)"`. With the
/// default relative link it points one level off.
#[test]
fn given_worktree_with_relative_envrc_copied_from_main_when_info_then_reports_it() {
    // Arrange
    let fx = fixture();
    symlink(
        fs::read_link(fx.main.join(".envrc")).unwrap(),
        fx.wt.join(".envrc"),
    )
    .unwrap();

    // Act
    let out = fx.rsenv_ok(&fx.wt, &["info"]);

    // Assert
    let stdout = stdout_of(&out);
    assert!(stdout.contains("envrc-relative-in-worktree"), "{stdout}");
}

#[test]
fn given_worktree_linked_to_another_vault_when_info_then_reports_it() {
    // Arrange
    let fx = fixture();
    let other = fx.base_dir.join("vaults/other-12345678");
    fs::create_dir_all(&other).unwrap();
    fs::write(other.join("dot.envrc"), "# other\n").unwrap();
    symlink(other.join("dot.envrc"), fx.wt.join(".envrc")).unwrap();

    // Act
    let out = fx.rsenv_ok(&fx.wt, &["info"]);

    // Assert
    let stdout = stdout_of(&out);
    assert!(stdout.contains("vault-differs-from-main"), "{stdout}");
}

#[test]
fn given_entry_swapped_in_by_other_host_when_info_then_reports_it() {
    // Arrange
    let fx = fixture();
    let sentinel = fs::read_dir(fx.vault.join("swap"))
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .find(|p| p.to_string_lossy().ends_with("@@rsenv_active"))
        .unwrap();
    fs::rename(
        &sentinel,
        fx.vault.join("swap/thoughts@@other-host@@rsenv_active"),
    )
    .unwrap();

    // Act
    let out = fx.rsenv_ok(&fx.main, &["info"]);

    // Assert
    let stdout = stdout_of(&out);
    assert!(stdout.contains("swapped-in-elsewhere"), "{stdout}");
    assert!(stdout.contains("other-host"), "{stdout}");
}

#[test]
fn given_vault_source_dir_points_elsewhere_when_info_in_main_then_reports_it() {
    // Arrange
    let fx = fixture();
    let dot_envrc = fx.vault.join("dot.envrc");
    let content = fs::read_to_string(&dot_envrc).unwrap();
    let start = content.find("# state.sourceDir = '").unwrap();
    let end = start + content[start..].find('\n').unwrap();
    let patched = format!(
        "{}# state.sourceDir = '/somewhere/else'{}",
        &content[..start],
        &content[end..]
    );
    fs::write(&dot_envrc, patched).unwrap();

    // Act
    let out = fx.rsenv_ok(&fx.main, &["info"]);

    // Assert
    let stdout = stdout_of(&out);
    assert!(stdout.contains("source-dir-mismatch"), "{stdout}");
    assert!(stdout.contains("rsenv vault reconnect"), "{stdout}");
}

#[test]
fn given_guarded_link_removed_in_main_when_info_then_reports_it() {
    // Arrange
    let fx = fixture();
    fs::remove_file(fx.main.join("secret.txt")).unwrap();

    // Act
    let out = fx.rsenv_ok(&fx.main, &["info"]);

    // Assert
    let stdout = stdout_of(&out);
    assert!(stdout.contains("guarded-link-broken"), "{stdout}");
    assert!(stdout.contains("secret.txt"), "{stdout}");
}

#[test]
fn given_attached_worktree_when_swap_status_then_header_names_main_and_no_reconnect_warning() {
    // Arrange
    let fx = fixture();
    fx.rsenv_ok(&fx.wt, &["worktree", "attach"]);

    // Act
    let out = fx.rsenv_ok(&fx.wt, &["swap", "status"]);

    // Assert
    let stdout = stdout_of(&out);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stdout.contains(&format!(
            "linked worktree: showing the state of {}",
            fx.main.display()
        )),
        "{stdout}"
    );
    assert!(!stderr.contains("out of sync"), "{stderr}");
}

/// `info --check` keeps its meaning (valid vault yes/no): prompts and scripts use it.
#[rstest]
#[case::main("main", 0)]
#[case::attached_worktree("attached", 0)]
#[case::unattached_worktree("unattached", 1)]
fn given_checkout_kind_when_info_check_then_exit_code_unchanged(
    #[case] which: &str,
    #[case] code: i32,
) {
    // Arrange
    let fx = fixture();
    if which == "attached" {
        fx.rsenv_ok(&fx.wt, &["worktree", "attach"]);
    }
    let dir = if which == "main" { &fx.main } else { &fx.wt };

    // Act
    let out = fx.rsenv(dir, &["info", "--check"]);

    // Assert
    assert_eq!(out.status.code(), Some(code), "{which}: {out:?}");
    assert!(out.stdout.is_empty());
}
