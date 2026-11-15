//! Detection of the git checkout kind of a directory: no git, main checkout, or
//! linked worktree. The vault belongs to the main checkout, so every worktree safety
//! rule starts from this answer.

use std::fs;
use std::io;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Output};

use rsenv::application::worktree::{detect, Checkout};
use rsenv::infrastructure::traits::{CommandRunner, RealCommandRunner};
use tempfile::TempDir;

/// git with the host's global/system config switched off, so `init.templateDir`,
/// signing or a default branch on the machine cannot change the outcome.
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

/// A repo with one commit. `--template=` keeps git from creating `.git/hooks`
/// (also denied by some sandboxes).
fn repo(dir: &Path) {
    fs::create_dir_all(dir).unwrap();
    git(dir, &["init", "-q", "--template=", "-b", "main"]);
    fs::write(dir.join("README.md"), "x\n").unwrap();
    git(dir, &["add", "README.md"]);
    git(dir, &["commit", "-q", "-m", "init"]);
}

fn canonical(p: &Path) -> PathBuf {
    fs::canonicalize(p).unwrap()
}

#[test]
fn given_plain_directory_when_detect_then_not_git() {
    let temp = TempDir::new().unwrap();

    let checkout = detect(&RealCommandRunner, temp.path()).unwrap();

    assert_eq!(checkout, Checkout::NotGit);
}

#[test]
fn given_main_checkout_when_detect_then_main_with_toplevel() {
    let temp = TempDir::new().unwrap();
    let proj = temp.path().join("proj");
    repo(&proj);

    let checkout = detect(&RealCommandRunner, &proj).unwrap();

    assert_eq!(
        checkout,
        Checkout::Main {
            dir: canonical(&proj)
        }
    );
}

#[test]
fn given_subdirectory_of_main_checkout_when_detect_then_main_with_toplevel() {
    let temp = TempDir::new().unwrap();
    let proj = temp.path().join("proj");
    repo(&proj);
    let sub = proj.join("a/b");
    fs::create_dir_all(&sub).unwrap();

    let checkout = detect(&RealCommandRunner, &sub).unwrap();

    assert_eq!(
        checkout,
        Checkout::Main {
            dir: canonical(&proj)
        }
    );
}

#[test]
fn given_linked_worktree_when_detect_then_linked_naming_main() {
    let temp = TempDir::new().unwrap();
    let proj = temp.path().join("proj");
    repo(&proj);
    git(
        &proj,
        &[
            "worktree",
            "add",
            "-q",
            "../proj__worktrees/feat",
            "-b",
            "feat",
        ],
    );
    let wt = temp.path().join("proj__worktrees/feat");

    let checkout = detect(&RealCommandRunner, &wt).unwrap();

    assert_eq!(
        checkout,
        Checkout::Linked {
            dir: canonical(&wt),
            main_dir: canonical(&proj),
        }
    );
}

#[test]
fn given_subdirectory_of_linked_worktree_when_detect_then_linked() {
    let temp = TempDir::new().unwrap();
    let proj = temp.path().join("proj");
    repo(&proj);
    git(
        &proj,
        &[
            "worktree",
            "add",
            "-q",
            "../proj__worktrees/feat",
            "-b",
            "feat",
        ],
    );
    let wt = temp.path().join("proj__worktrees/feat");
    let sub = wt.join("deep");
    fs::create_dir_all(&sub).unwrap();

    let checkout = detect(&RealCommandRunner, &sub).unwrap();

    assert!(
        matches!(&checkout, Checkout::Linked { main_dir, .. } if *main_dir == canonical(&proj)),
        "{checkout:?}"
    );
}

/// A submodule's `.git` is a file pointing elsewhere too, like a worktree's. It is
/// still its own main checkout: its git dir is its common dir.
#[test]
fn given_submodule_checkout_when_detect_then_main() {
    let temp = TempDir::new().unwrap();
    let lib = temp.path().join("lib");
    repo(&lib);
    let proj = temp.path().join("proj");
    repo(&proj);
    git(
        &proj,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            "-q",
            lib.to_str().unwrap(),
            "lib",
        ],
    );

    let checkout = detect(&RealCommandRunner, &proj.join("lib")).unwrap();

    assert_eq!(
        checkout,
        Checkout::Main {
            dir: canonical(&proj.join("lib"))
        }
    );
}

struct MissingGit;

impl CommandRunner for MissingGit {
    fn run(&self, _cmd: &str, _args: &[&str]) -> io::Result<Output> {
        Err(io::Error::new(io::ErrorKind::NotFound, "git not installed"))
    }
    fn run_with_stdin(&self, _cmd: &str, _args: &[&str], _stdin: &str) -> io::Result<Output> {
        unreachable!()
    }
    fn run_interactive(&self, _cmd: &str, _args: &[&str]) -> io::Result<ExitStatus> {
        unreachable!()
    }
}

/// rsenv works without git; a missing git binary must not break any command.
#[test]
fn given_git_not_installed_when_detect_then_not_git() {
    let temp = TempDir::new().unwrap();

    let checkout = detect(&MissingGit, temp.path()).unwrap();

    assert_eq!(checkout, Checkout::NotGit);
}

/// A bare repository has no main working tree, so each of its worktrees stands alone
/// and owns its vault, as before worktree support. Treating it as linked would refuse
/// every vault-changing command with no main checkout to run it in.
#[test]
fn given_worktree_of_bare_repository_when_detect_then_main_with_its_toplevel() {
    let temp = TempDir::new().unwrap();
    let src = temp.path().join("src");
    repo(&src);
    let bare = temp.path().join("repo.git");
    git(
        temp.path(),
        &["clone", "-q", "--bare", src.to_str().unwrap(), "repo.git"],
    );
    git(&bare, &["worktree", "add", "-q", "../wt", "main"]);
    let wt = temp.path().join("wt");

    let checkout = detect(&RealCommandRunner, &wt).unwrap();

    assert_eq!(
        checkout,
        Checkout::Main {
            dir: canonical(&wt)
        }
    );
}

/// `rev-parse` says linked, then `git worktree list` fails. The error must carry
/// git's own message, not claim a bare repository.
struct WorktreeListFails;

impl CommandRunner for WorktreeListFails {
    fn run(&self, _cmd: &str, args: &[&str]) -> io::Result<Output> {
        let ok = |stdout: &str| Output {
            status: ExitStatus::from_raw(0),
            stdout: stdout.as_bytes().to_vec(),
            stderr: Vec::new(),
        };
        if args.contains(&"rev-parse") {
            Ok(ok("/r/.git/worktrees/wt\n/r/.git\n/r/wt\n"))
        } else {
            Ok(Output {
                status: ExitStatus::from_raw(128 << 8),
                stdout: Vec::new(),
                stderr: b"fatal: simulated failure".to_vec(),
            })
        }
    }
    fn run_with_stdin(&self, _cmd: &str, _args: &[&str], _stdin: &str) -> io::Result<Output> {
        unreachable!()
    }
    fn run_interactive(&self, _cmd: &str, _args: &[&str]) -> io::Result<ExitStatus> {
        unreachable!()
    }
}

#[test]
fn given_git_worktree_list_fails_when_detect_linked_then_error_names_git_failure() {
    let err = detect(&WorktreeListFails, Path::new("/r/wt")).unwrap_err();

    // The whole error chain: context plus source.
    let chain = std::iter::successors(Some(&err as &dyn std::error::Error), |e| e.source())
        .map(|e| e.to_string())
        .collect::<Vec<_>>()
        .join(" | ");
    assert!(chain.contains("simulated failure"), "{chain}");
    assert!(!chain.contains("bare"), "{chain}");
}
