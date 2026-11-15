//! Which linked worktrees borrow which of main's paths.
//!
//! A worktree borrows `p` when `<wt>/p` is a symlink into `<main>/p`. Swapping such
//! an entry out of main leaves the worktree's link pointing at the committed original
//! or at nothing, so `swap out` asks this first.

use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::Command;

use rsenv::application::worktree::{borrowers, linked_worktrees, links_into, Borrower};
use rsenv::infrastructure::traits::{RealCommandRunner, RealFileSystem};
use tempfile::TempDir;

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

struct Repo {
    _temp: TempDir,
    root: PathBuf,
    main: PathBuf,
}

impl Repo {
    fn worktree(&self, name: &str) -> PathBuf {
        let rel = format!("../proj__worktrees/{name}");
        git(&self.main, &["worktree", "add", "-q", &rel, "-b", name]);
        self.root.join("proj__worktrees").join(name)
    }
}

/// Main checkout with an untracked `thoughts/` dir and `secret.txt` file.
fn repo() -> Repo {
    let temp = TempDir::new().unwrap();
    // Canonical: macOS temp dirs live behind the /var -> /private/var symlink.
    let root = fs::canonicalize(temp.path()).unwrap();
    let main = root.join("proj");
    fs::create_dir_all(main.join("thoughts")).unwrap();
    git(&main, &["init", "-q", "--template=", "-b", "main"]);
    fs::write(main.join("README.md"), "x\n").unwrap();
    git(&main, &["add", "README.md"]);
    git(&main, &["commit", "-q", "-m", "init"]);
    fs::write(main.join("thoughts/note.md"), "note\n").unwrap();
    fs::write(main.join("secret.txt"), "s\n").unwrap();
    Repo {
        _temp: temp,
        root,
        main,
    }
}

/// Borrowers among all linked worktrees of `main`, as `swap out` asks.
fn borrowers_of(main: &Path, ps: &[PathBuf]) -> Vec<Borrower> {
    let worktrees = linked_worktrees(&RealCommandRunner, main).unwrap();
    borrowers(&RealFileSystem, &worktrees, main, ps)
}

fn paths(ps: &[&str]) -> Vec<PathBuf> {
    ps.iter().map(PathBuf::from).collect()
}

#[test]
fn given_main_with_two_worktrees_when_linked_worktrees_then_both_without_main() {
    let repo = repo();
    let a = repo.worktree("a");
    let b = repo.worktree("b");

    let wts = linked_worktrees(&RealCommandRunner, &repo.main).unwrap();

    let dirs: Vec<_> = wts.iter().map(|w| w.dir.clone()).collect();
    assert_eq!(dirs, vec![a, b]);
    assert_eq!(wts[0].branch.as_deref(), Some("a"));
}

/// A worktree whose directory was deleted without `git worktree remove` is listed
/// as prunable; nothing in it can borrow anything.
#[test]
fn given_worktree_deleted_on_disk_when_linked_worktrees_then_skipped() {
    let repo = repo();
    let a = repo.worktree("a");
    let gone = repo.worktree("gone");
    fs::remove_dir_all(&gone).unwrap();

    let wts = linked_worktrees(&RealCommandRunner, &repo.main).unwrap();

    let dirs: Vec<_> = wts.iter().map(|w| w.dir.clone()).collect();
    assert_eq!(dirs, vec![a]);
}

#[test]
fn given_worktree_linking_into_main_when_borrowers_then_reports_linked_paths_only() {
    let repo = repo();
    let wt = repo.worktree("feat");
    symlink(repo.main.join("thoughts"), wt.join("thoughts")).unwrap();
    // A real file of the same name is the worktree's own, not borrowed.
    fs::write(wt.join("secret.txt"), "own\n").unwrap();

    let found = borrowers_of(&repo.main, &paths(&["thoughts", "secret.txt"]));

    assert_eq!(
        found,
        vec![Borrower {
            dir: wt,
            branch: Some("feat".into()),
            paths: paths(&["thoughts"]),
        }]
    );
}

#[test]
fn given_link_pointing_elsewhere_when_borrowers_then_not_a_borrower() {
    let repo = repo();
    let wt = repo.worktree("feat");
    let elsewhere = repo.root.join("elsewhere");
    fs::create_dir_all(&elsewhere).unwrap();
    symlink(&elsewhere, wt.join("thoughts")).unwrap();

    let found = borrowers_of(&repo.main, &paths(&["thoughts"]));

    assert!(found.is_empty(), "{found:?}");
}

/// After main swapped out, the worktree's link dangles. It still borrows: the next
/// `swap in` brings it back to life, and detach must still find it.
#[test]
fn given_dangling_link_into_main_when_borrowers_then_still_a_borrower() {
    let repo = repo();
    let wt = repo.worktree("feat");
    symlink(repo.main.join("thoughts"), wt.join("thoughts")).unwrap();
    fs::remove_dir_all(repo.main.join("thoughts")).unwrap();

    let found = borrowers_of(&repo.main, &paths(&["thoughts"]));

    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].paths, paths(&["thoughts"]));
}

/// Links are compared after resolving symlinked path components (macOS: /var vs
/// /private/var), so a link written through a non-canonical path still counts.
#[test]
fn given_link_written_through_symlinked_parent_when_borrowers_then_found() {
    let repo = repo();
    let wt = repo.worktree("feat");
    let alias = repo.root.join("alias");
    symlink(&repo.main, &alias).unwrap();
    symlink(alias.join("thoughts"), wt.join("thoughts")).unwrap();

    let found = borrowers_of(&repo.main, &paths(&["thoughts"]));

    assert_eq!(found.len(), 1, "{found:?}");
}

#[test]
fn given_not_a_git_repo_when_borrowers_then_empty() {
    let temp = TempDir::new().unwrap();

    let found = borrowers_of(temp.path(), &paths(&["thoughts"]));

    assert!(found.is_empty());
}

/// A borrowed guarded file is a link to main's link into the vault. It borrows from
/// main: what counts is where the worktree's link points, not the end of the chain.
#[test]
fn given_link_to_mains_own_link_when_borrowers_then_borrows_from_main() {
    let repo = repo();
    let wt = repo.worktree("feat");
    let vault_file = repo.root.join("vault/secret.txt");
    fs::create_dir_all(vault_file.parent().unwrap()).unwrap();
    fs::rename(repo.main.join("secret.txt"), &vault_file).unwrap();
    symlink(&vault_file, repo.main.join("secret.txt")).unwrap();
    symlink(repo.main.join("secret.txt"), wt.join("secret.txt")).unwrap();

    let found = borrowers_of(&repo.main, &paths(&["secret.txt"]));

    assert_eq!(found.len(), 1, "{found:?}");
}

#[test]
fn given_chained_link_when_links_into_main_dir_then_true() {
    let repo = repo();
    let wt = repo.worktree("feat");
    let vault_file = repo.root.join("vault/secret.txt");
    fs::create_dir_all(vault_file.parent().unwrap()).unwrap();
    fs::rename(repo.main.join("secret.txt"), &vault_file).unwrap();
    symlink(&vault_file, repo.main.join("secret.txt")).unwrap();
    symlink(repo.main.join("secret.txt"), wt.join("secret.txt")).unwrap();

    assert!(links_into(
        &RealFileSystem,
        &wt.join("secret.txt"),
        &repo.main
    ));
}

/// In a bare repository each worktree is its own main checkout. Its "linked
/// worktrees" are its siblings, never itself: it cannot borrow from itself.
#[test]
fn given_worktree_of_bare_repository_when_linked_worktrees_then_siblings_without_itself() {
    let repo = repo();
    git(
        &repo.root,
        &[
            "clone",
            "-q",
            "--bare",
            repo.main.to_str().unwrap(),
            "repo.git",
        ],
    );
    let bare = repo.root.join("repo.git");
    git(&bare, &["worktree", "add", "-q", "../a", "-b", "a"]);
    git(&bare, &["worktree", "add", "-q", "../b", "-b", "b"]);

    let wts = linked_worktrees(&RealCommandRunner, &repo.root.join("a")).unwrap();

    let dirs: Vec<_> = wts.iter().map(|w| w.dir.clone()).collect();
    assert_eq!(dirs, vec![repo.root.join("b")]);
}
