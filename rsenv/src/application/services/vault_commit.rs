//! Vault commit service — `rsenv vault commit`.
//!
//! Makes ONE commit scoped to this project's vault directory. It does NOT swap anything:
//! it requires the project to be swapped out already and refuses otherwise.
//!
//! # Why this exists
//!
//! `cd ~/.rsenv && git commit` sweeps every vault on the machine into a single commit.
//! The `-- .` pathspec here keeps the commit to `vaults/<name>-<id>/`, so other projects'
//! pending changes stay out of it.
//!
//! The commit message records the project's HEAD hash. That hash is the join key: given
//! a commit in the vault repo, it says which project state the content belongs to.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tracing::{debug, instrument};

use crate::application::error::{ApplicationError, ApplicationResult};
use crate::application::services::{SopsService, SwapService, VaultService};
use crate::config::Settings;
use crate::infrastructure::traits::CommandRunner;

/// Options for a vault commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommitOptions {
    /// Use the generated commit message directly instead of opening the editor.
    pub auto: bool,
    /// Push the vault repo after a successful commit.
    pub push: bool,
    /// Skip re-encryption for this run, whatever `sops.encrypt_on_commit` says.
    pub no_encrypt: bool,
}

/// A single staged change inside the vault.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedChange {
    /// Git status letter (`M`, `A`, `D`, `R100`, …)
    pub status: String,
    /// Path relative to the vault directory
    pub path: PathBuf,
}

/// State of the project repo at the moment the vault content was captured.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectCommit {
    /// Project is a git repo with at least one commit.
    Commit {
        sha: String,
        branch: String,
        dirty: bool,
    },
    /// Project is a git repo but HEAD is unborn.
    Unborn { branch: String },
    /// Project is not a git repo.
    NotARepo,
}

impl ProjectCommit {
    /// Short hash for the commit subject, if there is one.
    fn short(&self) -> Option<&str> {
        match self {
            ProjectCommit::Commit { sha, .. } => Some(&sha[..7.min(sha.len())]),
            _ => None,
        }
    }

    /// The `project-commit:` line body.
    fn describe(&self) -> String {
        match self {
            ProjectCommit::Commit { sha, branch, dirty } => {
                if *dirty {
                    format!("{} ({}, dirty)", sha, branch)
                } else {
                    format!("{} ({})", sha, branch)
                }
            }
            ProjectCommit::Unborn { branch } => format!("(no commits yet on {})", branch),
            ProjectCommit::NotARepo => "(not a git repository)".to_string(),
        }
    }
}

/// Result of a vault commit.
#[derive(Debug, Clone)]
pub struct CommitOutcome {
    /// Changes staged inside the vault
    pub staged: Vec<StagedChange>,
    /// Files re-encrypted before staging; empty when `sops.encrypt_on_commit` is off
    pub encrypted: Vec<PathBuf>,
    /// New commit SHA; `None` when there was nothing to commit or the editor aborted
    pub commit: Option<String>,
    /// Whether the vault repo was pushed
    pub pushed: bool,
    /// Project state recorded in the message
    pub project_commit: ProjectCommit,
}

/// Commits a single project's vault data.
pub struct VaultCommitService {
    swap: Arc<SwapService>,
    vault_service: Arc<VaultService>,
    sops: Arc<SopsService>,
    cmd: Arc<dyn CommandRunner>,
    settings: Arc<Settings>,
}

impl VaultCommitService {
    /// Create a new vault commit service.
    pub fn new(
        swap: Arc<SwapService>,
        vault_service: Arc<VaultService>,
        sops: Arc<SopsService>,
        cmd: Arc<dyn CommandRunner>,
        settings: Arc<Settings>,
    ) -> Self {
        Self {
            swap,
            vault_service,
            sops,
            cmd,
            settings,
        }
    }

    /// Commit this project's vault data.
    ///
    /// Requires the project to be swapped out already — see `require_swapped_out`.
    #[instrument(skip(self))]
    pub fn commit(
        &self,
        project_dir: &Path,
        opts: &CommitOptions,
    ) -> ApplicationResult<CommitOutcome> {
        let vault = self
            .vault_service
            .get(project_dir)?
            .ok_or_else(|| ApplicationError::VaultNotInitialized(project_dir.to_path_buf()))?;
        let vault_path = vault.path.clone();

        // Swapping out is the user's call, not this command's. All it does is refuse to
        // record a vault that does not yet hold the live bytes.
        self.require_swapped_out(project_dir)?;

        let project_commit = self.probe_project(project_dir);
        debug!("commit: project state {:?}", project_commit);

        // Encrypt before staging, so the `.enc` files committed below match the plaintext
        // they were derived from rather than whatever `.enc` happened to exist.
        let encrypted = if self.settings.sops.encrypt_on_commit && !opts.no_encrypt {
            debug!("commit: encrypting {}", vault_path.display());
            self.sops.encrypt_all(Some(&vault_path))?
        } else {
            Vec::new()
        };

        self.git(&vault_path, &["add", "-A", "."])?;

        let staged = self.staged_changes(&vault_path)?;
        reject_plaintext_secrets(&staged)?;

        if staged.is_empty() {
            debug!("commit: nothing staged in {}", vault_path.display());
            return Ok(CommitOutcome {
                encrypted,
                staged,
                commit: None,
                pushed: false,
                project_commit,
            });
        }

        let message = build_message(&vault.sentinel_id, project_dir, &project_commit, &staged);
        let head_before = self.head(&vault_path);

        // The `-- .` pathspec is what makes this a DEDICATED commit: staged changes in
        // other vaults stay in the index and out of this commit.
        if opts.auto {
            self.git(&vault_path, &["commit", "-m", &message, "--", "."])?;
        } else {
            self.commit_interactive(&vault_path, &message)?;
        }

        let head_after = self.head(&vault_path);
        let commit = match head_after {
            Some(sha) if Some(&sha) != head_before.as_ref() => Some(sha),
            // Interactive commit aborted in the editor: not an error.
            _ => {
                return Ok(CommitOutcome {
                    encrypted,
                    staged,
                    commit: None,
                    pushed: false,
                    project_commit,
                })
            }
        };

        let mut pushed = false;
        if opts.push {
            self.git(&vault_path, &["push"])?;
            pushed = true;
        }

        Ok(CommitOutcome {
            encrypted,
            staged,
            commit,
            pushed,
            project_commit,
        })
    }

    /// Refuse unless everything this host swapped in has been swapped out again.
    ///
    /// While a file is swapped in, the vault holds a frozen sentinel and the project holds
    /// the live bytes, so committing the vault would record stale content. Swapping out is
    /// left to the user precisely so that their `direnv`-aware wrapper stays in the loop.
    ///
    /// Scoped to this host: a foreign-host sentinel is another machine's baseline, which
    /// `swap_out` refuses to touch, so blocking on it would leave no way forward.
    fn require_swapped_out(&self, project_dir: &Path) -> ApplicationResult<()> {
        let still_in: Vec<String> = self
            .swap
            .swapped_in_here(project_dir)?
            .into_iter()
            .map(|f| {
                f.project_path
                    .strip_prefix(project_dir)
                    .unwrap_or(&f.project_path)
                    .display()
                    .to_string()
            })
            .collect();

        if still_in.is_empty() {
            return Ok(());
        }
        debug!("commit: refusing, {} entries swapped in", still_in.len());
        Err(ApplicationError::ProjectSwappedIn { paths: still_in })
    }

    // ============================================================
    // git plumbing
    // ============================================================

    /// Run `git -C <dir> <args>`, failing on a non-zero exit.
    fn git(&self, dir: &Path, args: &[&str]) -> ApplicationResult<String> {
        let (status, stdout, stderr) = self.git_raw(dir, args)?;
        if !status {
            return Err(ApplicationError::GitFailed {
                command: args.join(" "),
                stderr: stderr.trim().to_string(),
            });
        }
        Ok(stdout)
    }

    /// Run `git -C <dir> <args>` and return (success, stdout, stderr) without failing.
    fn git_raw(&self, dir: &Path, args: &[&str]) -> ApplicationResult<(bool, String, String)> {
        let dir = path_arg(dir)?;
        let mut full: Vec<&str> = vec!["-C", &dir];
        full.extend_from_slice(args);

        let out = self
            .cmd
            .run("git", &full)
            .map_err(|e| ApplicationError::OperationFailed {
                context: format!("run git {}", args.join(" ")),
                source: Box::new(e),
            })?;

        Ok((
            out.status.success(),
            String::from_utf8_lossy(&out.stdout).to_string(),
            String::from_utf8_lossy(&out.stderr).to_string(),
        ))
    }

    /// `git commit -e` hands the terminal to the editor, so it cannot use captured output.
    fn commit_interactive(&self, vault_path: &Path, message: &str) -> ApplicationResult<()> {
        let dir = path_arg(vault_path)?;
        let args = vec!["-C", &dir, "commit", "-e", "-m", message, "--", "."];
        self.cmd
            .run_interactive("git", &args)
            .map_err(|e| ApplicationError::OperationFailed {
                context: "run git commit -e".to_string(),
                source: Box::new(e),
            })?;
        // A non-zero exit here means the editor aborted the commit, which is a normal
        // outcome. The caller distinguishes it by comparing HEAD before and after.
        Ok(())
    }

    /// Current HEAD of a repo, or `None` if there is none.
    fn head(&self, dir: &Path) -> Option<String> {
        let (ok, stdout, _) = self.git_raw(dir, &["rev-parse", "HEAD"]).ok()?;
        if ok {
            Some(stdout.trim().to_string())
        } else {
            None
        }
    }

    /// Staged changes under `vault_path`. `--relative` both scopes and shortens the paths.
    fn staged_changes(&self, vault_path: &Path) -> ApplicationResult<Vec<StagedChange>> {
        let out = self.git(
            vault_path,
            &["diff", "--cached", "--name-status", "--relative"],
        )?;
        Ok(out
            .lines()
            .filter(|l| !l.trim().is_empty())
            .filter_map(|line| {
                let mut fields = line.split('\t');
                let status = fields.next()?.to_string();
                // Renames carry two paths; the destination is what now exists.
                let path = fields.next_back()?;
                Some(StagedChange {
                    status,
                    path: PathBuf::from(path),
                })
            })
            .collect())
    }

    /// Read the project's git state. Never fails: a project without git is legitimate.
    fn probe_project(&self, project_dir: &Path) -> ProjectCommit {
        let inside = self
            .git_raw(project_dir, &["rev-parse", "--is-inside-work-tree"])
            .map(|(ok, _, _)| ok)
            .unwrap_or(false);
        if !inside {
            return ProjectCommit::NotARepo;
        }

        let branch = self
            .git_raw(project_dir, &["rev-parse", "--abbrev-ref", "HEAD"])
            .map(|(_, out, _)| out.trim().to_string())
            .unwrap_or_default();

        match self.git_raw(project_dir, &["rev-parse", "HEAD"]) {
            Ok((true, sha, _)) => {
                let dirty = self
                    .git_raw(project_dir, &["status", "--porcelain"])
                    .map(|(_, out, _)| !out.trim().is_empty())
                    .unwrap_or(false);
                ProjectCommit::Commit {
                    sha: sha.trim().to_string(),
                    branch,
                    dirty,
                }
            }
            _ => ProjectCommit::Unborn { branch },
        }
    }
}

// ============================================================
// Free helpers
// ============================================================

fn path_arg(path: &Path) -> ApplicationResult<String> {
    path.to_str()
        .map(|s| s.to_string())
        .ok_or_else(|| ApplicationError::Config {
            message: format!("path is not valid UTF-8: {}", path.display()),
        })
}

/// Strip the `-<8 hex>` suffix from a sentinel id: `rsenv-1f53817c` -> `rsenv`.
fn vault_display_name(sentinel_id: &str) -> &str {
    match sentinel_id.rsplit_once('-') {
        Some((name, suffix))
            if suffix.len() == 8 && suffix.chars().all(|c| c.is_ascii_hexdigit()) =>
        {
            name
        }
        _ => sentinel_id,
    }
}

/// Refuse to commit plaintext that belongs behind SOPS.
///
/// The vault repo's `.gitignore` covers `envs/` via `*.env`, but `guarded/` relies on
/// per-file entries written by `rsenv guard add`. A missing or stale entry would leak
/// plaintext into a pushed repo, so this sits directly on that trust boundary.
fn reject_plaintext_secrets(staged: &[StagedChange]) -> ApplicationResult<()> {
    let leaks: Vec<String> = staged
        .iter()
        .filter(|c| is_plaintext_secret(&c.path))
        .map(|c| c.path.display().to_string())
        .collect();

    if leaks.is_empty() {
        return Ok(());
    }
    Err(ApplicationError::UnencryptedVaultSecrets { paths: leaks })
}

fn is_plaintext_secret(path: &Path) -> bool {
    if path.extension().is_some_and(|e| e == "enc") {
        return false;
    }
    let first = path
        .components()
        .next()
        .and_then(|c| c.as_os_str().to_str());
    matches!(first, Some("envs") | Some("guarded")) || path == Path::new("dot.envrc")
}

fn build_message(
    sentinel_id: &str,
    project_dir: &Path,
    project_commit: &ProjectCommit,
    staged: &[StagedChange],
) -> String {
    let name = vault_display_name(sentinel_id);
    let subject = match project_commit.short() {
        Some(short) => format!("vault({}): checkpoint @ {}", name, short),
        None => format!("vault({}): checkpoint", name),
    };

    let mut msg = format!(
        "{}\n\nproject:        {}\nproject-commit: {}\n\n",
        subject,
        project_dir.display(),
        project_commit.describe()
    );
    for change in staged {
        msg.push_str(&format!(" {} {}\n", change.status, change.path.display()));
    }
    msg
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_sentinel_id_with_hex_suffix_when_display_name_then_suffix_stripped() {
        assert_eq!(vault_display_name("rsenv-1f53817c"), "rsenv");
        assert_eq!(vault_display_name("los-cha-8b5d1b22"), "los-cha");
    }

    #[test]
    fn given_sentinel_id_without_hex_suffix_when_display_name_then_unchanged() {
        assert_eq!(vault_display_name("plain"), "plain");
        assert_eq!(vault_display_name("my-project"), "my-project");
    }

    #[test]
    fn given_plaintext_under_guarded_when_checked_then_flagged() {
        assert!(is_plaintext_secret(Path::new("guarded/id_rsa")));
        assert!(is_plaintext_secret(Path::new("envs/local.env")));
        assert!(is_plaintext_secret(Path::new("dot.envrc")));
    }

    #[test]
    fn given_encrypted_or_swap_paths_when_checked_then_allowed() {
        assert!(!is_plaintext_secret(Path::new(
            "guarded/id_rsa.a1b2c3d4.enc"
        )));
        assert!(!is_plaintext_secret(Path::new(
            "envs/local.env.f1d89926.enc"
        )));
        assert!(!is_plaintext_secret(Path::new("swap/thoughts/notes.md")));
    }
}
