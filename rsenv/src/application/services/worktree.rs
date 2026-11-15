//! Attach a linked git worktree to its main checkout's vault, and detach it again.
//!
//! A vault holds one swapped-in copy per host, in the main checkout. A linked worktree
//! borrows it: its `.envrc` links to the vault's `dot.envrc`, and each borrowed path
//! links to the same path in main. rsenv never creates or removes worktrees; git does.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::application::services::{SwapService, VaultService};
use crate::application::worktree::{detect, links_into, Checkout};
use crate::application::{parse_rsenv_metadata, ApplicationError, ApplicationResult};
use crate::domain::{expand_env_vars, SwapState, Vault};
use crate::infrastructure::traits::{CommandRunner, FileSystem};

/// Why a borrow-set path is not linked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    /// Main has nothing at this path (typical for configured extras).
    NotInMain,
    /// Tracked in git: the worktree keeps the committed version.
    Tracked,
    /// Already the planned link.
    AlreadyLinked,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttachItem {
    /// Create `<worktree>/path -> target`.
    Link {
        path: PathBuf,
        target: PathBuf,
    },
    Skip {
        path: PathBuf,
        reason: SkipReason,
    },
    /// Something else is already at `<worktree>/path`; attach never overwrites.
    Conflict {
        path: PathBuf,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachPlan {
    pub worktree: PathBuf,
    pub main_dir: PathBuf,
    /// `.envrc` first, then the borrow set in path order.
    pub items: Vec<AttachItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetachReport {
    pub worktree: PathBuf,
    pub main_dir: PathBuf,
    /// Links removed, worktree-relative.
    pub removed: Vec<PathBuf>,
    /// Borrow-set paths left alone: real files, links pointing elsewhere.
    pub kept: Vec<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Warn,
    Error,
}

/// A health finding of `rsenv info`. Advisory: it never changes an exit code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    /// Stable kebab-case identifier, e.g. `borrowed-link-dangling`.
    pub code: &'static str,
    pub severity: Severity,
    pub message: String,
    /// The command that fixes it, when there is one.
    pub hint: Option<String>,
}

pub struct WorktreeService {
    fs: Arc<dyn FileSystem>,
    vault_service: Arc<VaultService>,
    swap_service: Arc<SwapService>,
    runner: Arc<dyn CommandRunner>,
}

fn refused(message: String) -> ApplicationError {
    ApplicationError::WorktreeRefused { message }
}

impl WorktreeService {
    pub fn new(
        fs: Arc<dyn FileSystem>,
        vault_service: Arc<VaultService>,
        swap_service: Arc<SwapService>,
        runner: Arc<dyn CommandRunner>,
    ) -> Self {
        Self {
            fs,
            vault_service,
            swap_service,
            runner,
        }
    }

    /// Top levels of the linked worktree containing `dir` and of its main checkout.
    fn linked(&self, dir: &Path) -> ApplicationResult<(PathBuf, PathBuf)> {
        match detect(self.runner.as_ref(), dir)? {
            Checkout::Linked { dir, main_dir } => Ok((dir, main_dir)),
            Checkout::Main { dir } => Err(refused(format!(
                "{} is not a linked git worktree (it is a main checkout)",
                dir.display()
            ))),
            Checkout::NotGit => Err(refused(format!(
                "{} is not a linked git worktree (not in a git repository)",
                dir.display()
            ))),
        }
    }

    /// Plan `attach` for the linked worktree containing `dir`; writes nothing.
    ///
    /// Refuses unless main has a vault and every swap entry is swapped in on this host
    /// (borrowing a swapped-out entry hands the worktree the committed original).
    /// `extra` is the `[worktree] borrow` config of main's vault.
    pub fn plan_attach(&self, dir: &Path, extra: &[String]) -> ApplicationResult<AttachPlan> {
        let (wt, main) = self.linked(dir)?;
        let vault = self.vault_service.get(&main)?.ok_or_else(|| {
            refused(format!(
                "main checkout {} has no vault (no .envrc link); run `rsenv vault init` there",
                main.display()
            ))
        })?;

        let entries = self.swap_entries(&main)?;
        let here: BTreeSet<PathBuf> = self
            .swap_service
            .swapped_in_here(&main)?
            .into_iter()
            .map(|f| relative(&main, &f.project_path))
            .collect();
        let not_in: Vec<PathBuf> = entries
            .iter()
            .filter(|p| !here.contains(*p))
            .cloned()
            .collect();
        if !not_in.is_empty() {
            // A swapped-out directory entry is reported as its files (`swap status`),
            // so the hint cannot always name the entry itself.
            return Err(refused(format!(
                "main checkout is not swapped in: {}\n  run: rsenv -C {} swap in <entry>  \
                 (for a directory entry, name the directory, not its files)",
                not_in
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", "),
                main.display(),
            )));
        }

        let mut borrow: BTreeSet<PathBuf> = entries.into_iter().collect();
        borrow.extend(self.vault_service.guarded_paths(&vault)?);
        borrow.extend(extra_paths(extra));

        let mut items = vec![self.plan_envrc(&wt, &vault.path.join("dot.envrc"))];
        for p in borrow {
            items.push(self.plan_borrow(&wt, &main, &p));
        }
        Ok(AttachPlan {
            worktree: wt,
            main_dir: main,
            items,
        })
    }

    /// Create the plan's links. Refuses, writing nothing, if the plan has a conflict.
    ///
    /// ponytail: no rollback. Planning first catches the common failure (a conflict);
    /// an I/O error halfway leaves a partly attached worktree, and re-running attach
    /// completes it because planning is idempotent.
    pub fn apply_attach(&self, plan: &AttachPlan) -> ApplicationResult<Vec<PathBuf>> {
        let conflicts: Vec<String> = plan
            .items
            .iter()
            .filter_map(|i| match i {
                AttachItem::Conflict { path } => Some(format!("    {}", path.display())),
                _ => None,
            })
            .collect();
        if !conflicts.is_empty() {
            return Err(refused(format!(
                "{} already has its own version of these paths; nothing was linked:\n{}\n  \
                 Move or delete them, then attach again.",
                plan.worktree.display(),
                conflicts.join("\n")
            )));
        }

        let mut linked = Vec::new();
        for item in &plan.items {
            if let AttachItem::Link { path, target } = item {
                let link = plan.worktree.join(path);
                if let Some(parent) = link.parent() {
                    self.fs
                        .create_dir_all(parent)
                        .map_err(|e| io_err("create", parent, e))?;
                }
                // Absolute: the worktree sits at another depth than main, so main's
                // relative links would point elsewhere from here.
                self.fs
                    .symlink(target, &link)
                    .map_err(|e| io_err("link", &link, e))?;
                linked.push(path.clone());
            }
        }
        Ok(linked)
    }

    /// Remove the worktree's `.envrc` and every link into main or its vault.
    ///
    /// Decides by link target, not by vault state, so it works after main swapped out
    /// or lost its vault. Candidates: the borrow set (when main's vault is still
    /// reachable), `extra`, and every top-level symlink of the worktree.
    /// ponytail: with main's vault gone, a borrowed link nested deeper than the top
    /// level and not in `extra` is not found; walking the whole worktree would.
    pub fn detach(&self, dir: &Path, extra: &[String]) -> ApplicationResult<DetachReport> {
        let (wt, main) = self.linked(dir)?;
        let vault = self.vault_service.get(&main).ok().flatten();

        let mut candidates: BTreeSet<PathBuf> = extra_paths(extra).collect();
        if let Some(v) = &vault {
            candidates.extend(self.swap_entries(&main).unwrap_or_default());
            candidates.extend(self.vault_service.guarded_paths(v).unwrap_or_default());
        }
        if let Ok(read) = std::fs::read_dir(&wt) {
            for entry in read.flatten() {
                if entry.path().is_symlink() {
                    candidates.insert(PathBuf::from(entry.file_name()));
                }
            }
        }
        candidates.remove(Path::new(".envrc"));

        let mut removed = Vec::new();
        let mut kept = Vec::new();

        let envrc = wt.join(".envrc");
        if self.fs.is_symlink(&envrc) {
            // The same test `VaultService::get` uses to recognise a vault link.
            let to_vault = self
                .fs
                .read_link(&envrc)
                .map(|t| t.ends_with("dot.envrc"))
                .unwrap_or(false);
            if to_vault {
                self.fs
                    .remove_file(&envrc)
                    .map_err(|e| io_err("remove", &envrc, e))?;
                removed.push(PathBuf::from(".envrc"));
            } else {
                kept.push(PathBuf::from(".envrc"));
            }
        }

        for p in candidates {
            let link = wt.join(&p);
            if !self.fs.exists_or_is_symlink(&link) {
                continue;
            }
            let borrowed = self.fs.is_symlink(&link)
                && (links_into(&link, &main)
                    || vault.as_ref().is_some_and(|v| links_into(&link, &v.path)));
            if borrowed {
                self.fs
                    .remove_file(&link)
                    .map_err(|e| io_err("remove", &link, e))?;
                removed.push(p);
            } else {
                kept.push(p);
            }
        }

        Ok(DetachReport {
            worktree: wt,
            main_dir: main,
            removed,
            kept,
        })
    }

    /// Health findings for the checkout containing `project_dir` (design R5).
    ///
    /// In a linked worktree: attachment and borrowed links. In a main checkout:
    /// sourceDir and guarded links. Both: entries swapped in on another host.
    pub fn problems(&self, project_dir: &Path) -> Vec<Problem> {
        let mut found = Vec::new();
        let here = hostname::get()
            .map(|h| h.to_string_lossy().into_owned())
            .unwrap_or_default();
        let checkout = detect(self.runner.as_ref(), project_dir).unwrap_or(Checkout::NotGit);

        let (vault, owner) = match &checkout {
            Checkout::Linked { dir: wt, main_dir } => {
                let main_vault = self.vault_service.get(main_dir).ok().flatten();
                self.worktree_problems(wt, main_dir, main_vault.as_ref(), &mut found);
                (main_vault, main_dir.clone())
            }
            Checkout::Main { .. } | Checkout::NotGit => {
                let vault = self.vault_service.get(project_dir).ok().flatten();
                if let Some(v) = &vault {
                    self.main_problems(project_dir, v, &mut found);
                }
                (vault, project_dir.to_path_buf())
            }
        };

        if vault.is_some() {
            for entry in self.swap_service.status_quiet(&owner).unwrap_or_default() {
                if let SwapState::In { hostname } = &entry.state {
                    if *hostname != here {
                        let rel = relative(&owner, &entry.project_path);
                        found.push(Problem {
                            code: "swapped-in-elsewhere",
                            severity: Severity::Warn,
                            message: format!(
                                "{} is swapped in on host {}",
                                rel.display(),
                                hostname
                            ),
                            hint: Some(format!("on {hostname}: rsenv swap out {}", rel.display())),
                        });
                    }
                }
            }
        }
        found
    }

    fn worktree_problems(
        &self,
        wt: &Path,
        main: &Path,
        main_vault: Option<&Vault>,
        found: &mut Vec<Problem>,
    ) {
        let attach_hint = format!("rsenv worktree attach {}", wt.display());
        let envrc = wt.join(".envrc");
        if !self.fs.is_symlink(&envrc) {
            found.push(Problem {
                code: "not-attached",
                severity: Severity::Warn,
                message: format!(
                    "worktree has no .envrc link to the vault of {}",
                    main.display()
                ),
                hint: Some(attach_hint),
            });
            return;
        }

        let own_vault = self.vault_service.get(wt);
        let relative_link = self
            .fs
            .read_link(&envrc)
            .map(|t| t.is_relative())
            .unwrap_or(false);
        let same_vault = match (&own_vault, main_vault) {
            (Ok(Some(own)), Some(m)) => own.path == m.path,
            _ => false,
        };
        if relative_link && !same_vault {
            found.push(Problem {
                code: "envrc-relative-in-worktree",
                severity: Severity::Error,
                message: ".envrc is a relative link that does not reach main's vault from \
                          this depth (copied from main?)"
                    .to_string(),
                hint: Some(format!("rm {} && {attach_hint}", envrc.display())),
            });
        } else if let (Ok(Some(own)), Some(m)) = (&own_vault, main_vault) {
            if own.path != m.path {
                found.push(Problem {
                    code: "vault-differs-from-main",
                    severity: Severity::Error,
                    message: format!(
                        ".envrc links to vault {}, main uses {}",
                        own.path.display(),
                        m.path.display()
                    ),
                    hint: Some(format!("rm {} && {attach_hint}", envrc.display())),
                });
            }
        }

        // Every link into main: borrow set (when main's vault is reachable) plus
        // top-level links, which covers configured extras.
        let mut candidates: BTreeSet<PathBuf> = BTreeSet::new();
        let mut out_entries: BTreeSet<PathBuf> = BTreeSet::new();
        if let Some(m) = main_vault {
            for entry in self.swap_service.status_quiet(main).unwrap_or_default() {
                let rel = relative(main, &entry.project_path);
                if entry.state == SwapState::Out {
                    out_entries.insert(rel.clone());
                }
                candidates.insert(rel);
            }
            candidates.extend(self.vault_service.guarded_paths(m).unwrap_or_default());
        }
        if let Ok(read) = std::fs::read_dir(wt) {
            for entry in read.flatten() {
                if entry.path().is_symlink() {
                    candidates.insert(PathBuf::from(entry.file_name()));
                }
            }
        }
        candidates.remove(Path::new(".envrc"));

        for p in candidates {
            let link = wt.join(&p);
            if !links_into(&link, &main.join(&p)) {
                continue;
            }
            let swap_in = format!("rsenv -C {} swap in {}", main.display(), p.display());
            // `swap status` reports a swapped-out DIRECTORY entry as its files, so `p`
            // is out when any reported out path lies at or below it.
            if out_entries.iter().any(|o| o.starts_with(&p)) {
                found.push(Problem {
                    code: "borrowed-not-swapped-in",
                    severity: Severity::Warn,
                    message: format!(
                        "{} is swapped out in main: the worktree sees the committed original or nothing",
                        p.display()
                    ),
                    hint: Some(swap_in),
                });
            } else if !self.fs.exists(&link) {
                found.push(Problem {
                    code: "borrowed-link-dangling",
                    severity: Severity::Error,
                    message: format!(
                        "{} links to {}, which does not exist",
                        p.display(),
                        main.join(&p).display()
                    ),
                    hint: None,
                });
            }
        }
    }

    fn main_problems(&self, project_dir: &Path, vault: &Vault, found: &mut Vec<Problem>) {
        let dot_envrc = vault.path.join("dot.envrc");
        if let Some(meta) = self
            .fs
            .read_to_string(&dot_envrc)
            .ok()
            .and_then(|c| parse_rsenv_metadata(&c))
        {
            let recorded = PathBuf::from(expand_env_vars(&meta.source_dir));
            let canon = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
            if canon(&recorded) != canon(project_dir) {
                found.push(Problem {
                    code: "source-dir-mismatch",
                    severity: Severity::Warn,
                    message: format!(
                        "vault records the project at {}, not here",
                        recorded.display()
                    ),
                    hint: Some(format!("rsenv vault reconnect {}", dot_envrc.display())),
                });
            }
        }

        let guarded_dir = vault.path.join("guarded");
        for p in self.vault_service.guarded_paths(vault).unwrap_or_default() {
            let link = project_dir.join(&p);
            if !(self.fs.is_symlink(&link) && links_into(&link, &guarded_dir)) {
                found.push(Problem {
                    code: "guarded-link-broken",
                    severity: Severity::Error,
                    message: format!(
                        "{} is not a link into the vault's guarded files",
                        p.display()
                    ),
                    hint: Some(format!(
                        "recreate it: ln -s {} {}",
                        guarded_dir
                            .join(crate::application::dotfile::neutralize_path(&p))
                            .display(),
                        link.display()
                    )),
                });
            }
        }
    }

    /// Project-relative paths of all swap entries of main's vault, in or out.
    fn swap_entries(&self, main: &Path) -> ApplicationResult<Vec<PathBuf>> {
        Ok(self
            .swap_service
            .status(main)?
            .into_iter()
            .map(|f| relative(main, &f.project_path))
            .collect())
    }

    fn plan_envrc(&self, wt: &Path, dot_envrc: &Path) -> AttachItem {
        let path = PathBuf::from(".envrc");
        let link = wt.join(&path);
        if links_into(&link, dot_envrc) {
            AttachItem::Skip {
                path,
                reason: SkipReason::AlreadyLinked,
            }
        } else if !self.fs.exists_or_is_symlink(&link) {
            AttachItem::Link {
                path,
                target: dot_envrc.to_path_buf(),
            }
        } else {
            AttachItem::Conflict { path }
        }
    }

    fn plan_borrow(&self, wt: &Path, main: &Path, p: &Path) -> AttachItem {
        let path = p.to_path_buf();
        let in_main = main.join(p);
        let link = wt.join(p);
        if !self.fs.exists_or_is_symlink(&in_main) {
            AttachItem::Skip {
                path,
                reason: SkipReason::NotInMain,
            }
        } else if links_into(&link, &in_main) {
            AttachItem::Skip {
                path,
                reason: SkipReason::AlreadyLinked,
            }
        } else if self.tracked(wt, p) {
            AttachItem::Skip {
                path,
                reason: SkipReason::Tracked,
            }
        } else if !self.fs.exists_or_is_symlink(&link) {
            AttachItem::Link {
                path,
                target: in_main,
            }
        } else {
            AttachItem::Conflict { path }
        }
    }

    /// `p` (or something below it) is tracked in the worktree's git index.
    fn tracked(&self, wt: &Path, p: &Path) -> bool {
        let wt_str = wt.to_string_lossy();
        let p_str = p.to_string_lossy();
        self.runner
            .run(
                "git",
                &["-C", &wt_str, "ls-files", "--error-unmatch", "--", &p_str],
            )
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}

/// Configured extras as paths. Negations are consumed by config layering; one left
/// over (a `!x` in the global file) names nothing to link.
fn extra_paths(extra: &[String]) -> impl Iterator<Item = PathBuf> + '_ {
    extra
        .iter()
        .filter(|e| !e.is_empty() && !e.starts_with('!') && e.as_str() != ".envrc")
        .map(PathBuf::from)
}

fn relative(main: &Path, path: &Path) -> PathBuf {
    path.strip_prefix(main).unwrap_or(path).to_path_buf()
}

fn io_err(what: &str, path: &Path, e: std::io::Error) -> ApplicationError {
    ApplicationError::OperationFailed {
        context: format!("{what} {}", path.display()),
        source: Box::new(e),
    }
}
