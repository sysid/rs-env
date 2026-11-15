//! Git checkout kind of a directory: no git, main checkout, or linked worktree.
//!
//! A vault records one swapped-in copy per host, and that copy lives in the main
//! checkout. A linked worktree reaches the same vault through its `.envrc` but must
//! only borrow main's files, never change the vault. Every worktree rule starts from
//! the answer given here.

use std::path::{Path, PathBuf};

use crate::application::{ApplicationError, ApplicationResult};
use crate::infrastructure::traits::CommandRunner;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Checkout {
    /// Not inside a git working tree, or git is not installed. rsenv works without
    /// git, so this is not an error.
    NotGit,
    /// The main working tree. `dir` is its top level.
    Main { dir: PathBuf },
    /// A linked worktree (`git worktree add`). `dir` is its top level, `main_dir` the
    /// top level of the main working tree that owns the repository.
    Linked { dir: PathBuf, main_dir: PathBuf },
}

/// Detect the checkout kind of `dir` (or of the working tree containing it).
///
/// Asks git rather than reading `.git` files by hand: git knows about `GIT_DIR`,
/// `--separate-git-dir` and submodules, whose `.git` is a pointer file too.
pub fn detect(runner: &dyn CommandRunner, dir: &Path) -> ApplicationResult<Checkout> {
    let dir_str = dir.to_string_lossy();
    let out = match runner.run(
        "git",
        &[
            "-C",
            &dir_str,
            "rev-parse",
            "--path-format=absolute",
            "--git-dir",
            "--git-common-dir",
            "--show-toplevel",
        ],
    ) {
        Ok(out) if out.status.success() => out,
        // git missing, or `dir` not inside a working tree.
        _ => return Ok(Checkout::NotGit),
    };

    let stdout = String::from_utf8_lossy(&out.stdout);
    let mut lines = stdout.lines();
    let (Some(git_dir), Some(common_dir), Some(toplevel)) =
        (lines.next(), lines.next(), lines.next())
    else {
        return Ok(Checkout::NotGit);
    };

    let toplevel = PathBuf::from(toplevel);
    if git_dir == common_dir {
        return Ok(Checkout::Main { dir: toplevel });
    }

    Ok(Checkout::Linked {
        main_dir: main_worktree(runner, &toplevel)?,
        dir: toplevel,
    })
}

/// Top level of the main working tree: the first entry of `git worktree list`.
///
/// The common dir's parent would be a guess: wrong for `--separate-git-dir`, and a
/// bare repository has no main working tree at all.
fn main_worktree(runner: &dyn CommandRunner, linked: &Path) -> ApplicationResult<PathBuf> {
    let linked_str = linked.to_string_lossy();
    let out = runner
        .run(
            "git",
            &["-C", &linked_str, "worktree", "list", "--porcelain"],
        )
        .map_err(|e| ApplicationError::OperationFailed {
            context: format!("git worktree list in {}", linked.display()),
            source: Box::new(e),
        })?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let first = parse_worktree_list(&stdout).into_iter().next();
    match first {
        Some(entry) if !entry.bare => Ok(entry.dir),
        _ => Err(ApplicationError::OperationFailed {
            context: format!(
                "{} is a worktree of a bare repository; rsenv needs a main checkout to own the vault",
                linked.display()
            ),
            source: Box::new(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "bare repository",
            )),
        }),
    }
}

/// One entry of `git worktree list --porcelain`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeEntry {
    pub dir: PathBuf,
    /// Short branch name; `None` for a detached HEAD or a bare entry.
    pub branch: Option<String>,
    pub bare: bool,
    /// Its directory is gone; git will drop it on `git worktree prune`.
    pub prunable: bool,
}

/// Parse `git worktree list --porcelain`: blank-line separated blocks, the first
/// line of each is `worktree <path>`; the first block is the main working tree.
pub fn parse_worktree_list(porcelain: &str) -> Vec<WorktreeEntry> {
    let mut entries = Vec::new();
    let mut current: Option<WorktreeEntry> = None;
    for line in porcelain.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            entries.extend(current.take());
            current = Some(WorktreeEntry {
                dir: PathBuf::from(path),
                branch: None,
                bare: false,
                prunable: false,
            });
        } else if let Some(entry) = current.as_mut() {
            if let Some(branch) = line.strip_prefix("branch ") {
                entry.branch = Some(
                    branch
                        .strip_prefix("refs/heads/")
                        .unwrap_or(branch)
                        .to_string(),
                );
            } else if line == "bare" {
                entry.bare = true;
            } else if line == "prunable" || line.starts_with("prunable ") {
                entry.prunable = true;
            }
        }
    }
    entries.extend(current);
    entries
}

/// A linked worktree and the paths it borrows from main.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Borrower {
    pub dir: PathBuf,
    pub branch: Option<String>,
    pub paths: Vec<PathBuf>,
}

/// Linked worktrees of the repository whose main working tree is `main_dir`.
///
/// Excludes main itself, bare entries and prunable ones (directory gone). Empty when
/// `main_dir` is not in a git repository or git is missing.
pub fn linked_worktrees(
    runner: &dyn CommandRunner,
    main_dir: &Path,
) -> ApplicationResult<Vec<WorktreeEntry>> {
    let main_str = main_dir.to_string_lossy();
    let out = match runner.run("git", &["-C", &main_str, "worktree", "list", "--porcelain"]) {
        Ok(out) if out.status.success() => out,
        _ => return Ok(Vec::new()),
    };
    let stdout = String::from_utf8_lossy(&out.stdout);
    Ok(parse_worktree_list(&stdout)
        .into_iter()
        .skip(1)
        .filter(|w| !w.bare && !w.prunable)
        .collect())
}

/// Linked worktrees of `main_dir` that borrow any of `paths` (project-relative):
/// `<wt>/p` is a symlink resolving to `<main>/p` or below it. Dangling links count:
/// they come back to life on the next swap-in.
pub fn borrowers(
    runner: &dyn CommandRunner,
    main_dir: &Path,
    paths: &[PathBuf],
) -> ApplicationResult<Vec<Borrower>> {
    let main = resolve(main_dir);
    let mut found = Vec::new();
    for wt in linked_worktrees(runner, main_dir)? {
        let borrowed: Vec<PathBuf> = paths
            .iter()
            .filter(|p| links_into(&wt.dir.join(p), &main.join(p)))
            .cloned()
            .collect();
        if !borrowed.is_empty() {
            found.push(Borrower {
                dir: wt.dir,
                branch: wt.branch,
                paths: borrowed,
            });
        }
    }
    Ok(found)
}

/// True when `link` is a symlink whose target is `root` or lies below it.
///
/// Compares the link's FIRST hop: the target's directory is resolved, its last
/// component is not. A borrowed guarded file is `<wt>/p -> <main>/p -> <vault>/...`;
/// it borrows from main, wherever main's own link leads. Works for dangling links.
pub fn links_into(link: &Path, root: &Path) -> bool {
    let Ok(target) = std::fs::read_link(link) else {
        return false;
    };
    let target = if target.is_relative() {
        link.parent().map(|p| p.join(&target)).unwrap_or(target)
    } else {
        target
    };
    resolve_parent(&target).starts_with(resolve_parent(root))
}

/// `resolve()` of the parent, joined with the last component as it is.
fn resolve_parent(path: &Path) -> PathBuf {
    match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) => resolve(parent).join(name),
        _ => resolve(path),
    }
}

/// Canonical form of `path` even when it does not exist: canonicalize the longest
/// existing ancestor and append the rest. Needed because a dangling link has no
/// canonical target, and macOS temp paths hide behind `/var -> /private/var`.
fn resolve(path: &Path) -> PathBuf {
    if let Ok(p) = std::fs::canonicalize(path) {
        return p;
    }
    match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) => resolve(parent).join(name),
        _ => path.to_path_buf(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_porcelain_with_main_linked_detached_and_prunable_when_parsed_then_all_fields_read() {
        let porcelain = "\
worktree /r/proj
HEAD 1111
branch refs/heads/main

worktree /r/proj__worktrees/feat
HEAD 2222
branch refs/heads/feat/x

worktree /r/proj__worktrees/det
HEAD 3333
detached

worktree /r/gone
HEAD 4444
branch refs/heads/gone
prunable gitdir file points to non-existent location
";

        let entries = parse_worktree_list(porcelain);

        assert_eq!(
            entries,
            vec![
                WorktreeEntry {
                    dir: "/r/proj".into(),
                    branch: Some("main".into()),
                    bare: false,
                    prunable: false
                },
                WorktreeEntry {
                    dir: "/r/proj__worktrees/feat".into(),
                    branch: Some("feat/x".into()),
                    bare: false,
                    prunable: false
                },
                WorktreeEntry {
                    dir: "/r/proj__worktrees/det".into(),
                    branch: None,
                    bare: false,
                    prunable: false
                },
                WorktreeEntry {
                    dir: "/r/gone".into(),
                    branch: Some("gone".into()),
                    bare: false,
                    prunable: true
                },
            ]
        );
    }

    #[test]
    fn given_bare_main_when_parsed_then_first_entry_is_bare() {
        let entries = parse_worktree_list(
            "worktree /r/repo.git\nbare\n\nworktree /r/wt\nHEAD 1\nbranch refs/heads/x\n",
        );

        assert!(entries[0].bare);
        assert!(!entries[1].bare);
    }
}
