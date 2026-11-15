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
    /// Refuses unless main has a vault and every swap entry it would link is swapped in
    /// on this host (borrowing a swapped-out entry hands the worktree the committed
    /// original). Tracked entries are never linked, so their state does not matter.
    /// `extra` is the `[worktree] borrow` config of main's vault.
    pub fn plan_attach(&self, dir: &Path, extra: &[String]) -> ApplicationResult<AttachPlan> {
        let (wt, main) = self.linked(dir)?;
        let vault = self.vault_service.get(&main)?.ok_or_else(|| {
            refused(format!(
                "main checkout {} has no vault (no .envrc link); run `rsenv vault init` there",
                main.display()
            ))
        })?;

        let borrow = self.borrow_set(&main, &vault, extra)?;
        let tracked = self.tracked(&wt, &borrow);
        let here: BTreeSet<PathBuf> = self
            .swap_service
            .swapped_in_here(&main)?
            .into_iter()
            .map(|f| relative(&main, &f.project_path))
            .collect();
        let not_in: Vec<PathBuf> = self
            .swap_entries(&main)?
            .into_iter()
            .filter(|p| !here.contains(p) && !is_tracked(&tracked, p))
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

        let mut items = vec![self.plan_envrc(&wt, &vault.path.join("dot.envrc"))];
        // Paths whose link reaches main; a path below one is reached through it.
        // BTreeSet order puts a directory before the paths below it.
        let mut reached: Vec<PathBuf> = Vec::new();
        for p in borrow {
            if reached.iter().any(|r| p.starts_with(r)) {
                continue;
            }
            let item = self.plan_borrow(&wt, &main, &p, is_tracked(&tracked, &p));
            if let AttachItem::Link { .. }
            | AttachItem::Skip {
                reason: SkipReason::AlreadyLinked,
                ..
            } = &item
            {
                reached.push(p);
            }
            items.push(item);
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

    /// Remove the worktree's `.envrc` and every link to the same path in main or into
    /// main's vault.
    ///
    /// Decides by link target, not by vault state, so it works after main swapped out
    /// or lost its vault. Candidates: see `link_candidates`.
    /// ponytail: with main's vault gone, a borrowed link nested deeper than the top
    /// level and not in `extra` is not found; walking the whole worktree would.
    pub fn detach(&self, dir: &Path, extra: &[String]) -> ApplicationResult<DetachReport> {
        let (wt, main) = self.linked(dir)?;
        let vault = self.vault_service.get(&main).ok().flatten();
        let candidates = self.link_candidates(&wt, &main, vault.as_ref(), extra);

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
            // The SAME path in main, not anything below main: a worktree nested in main
            // has committed relative links that resolve below main too.
            let borrowed = self.fs.is_symlink(&link)
                && (links_into(self.fs.as_ref(), &link, &main.join(&p))
                    || vault
                        .as_ref()
                        .is_some_and(|v| links_into(self.fs.as_ref(), &link, &v.path)));
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

        // Configured extras are not known here; the top-level scan covers the usual ones.
        let candidates = self.link_candidates(wt, main, main_vault, &[]);
        let out_entries: BTreeSet<PathBuf> = match main_vault {
            Some(_) => self
                .swap_service
                .status_quiet(main)
                .unwrap_or_default()
                .into_iter()
                .filter(|e| e.state == SwapState::Out)
                .map(|e| relative(main, &e.project_path))
                .collect(),
            None => BTreeSet::new(),
        };

        for p in candidates {
            let link = wt.join(&p);
            if !links_into(self.fs.as_ref(), &link, &main.join(&p)) {
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
            let canon = |p: &Path| self.fs.canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
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
            if !(self.fs.is_symlink(&link) && links_into(self.fs.as_ref(), &link, &guarded_dir)) {
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

    /// What a worktree of `main` borrows: the swap entries of main's vault (in or out),
    /// its guarded files, and the configured extras. Project-relative.
    pub fn borrow_set(
        &self,
        main: &Path,
        vault: &Vault,
        extra: &[String],
    ) -> ApplicationResult<BTreeSet<PathBuf>> {
        let mut set: BTreeSet<PathBuf> = self.swap_entries(main)?.into_iter().collect();
        set.extend(self.vault_service.guarded_paths(vault)?);
        set.extend(extra_paths(extra));
        Ok(set)
    }

    /// Project-relative paths of all swap entries of main's vault, in or out.
    ///
    /// Quiet: callers report a sourceDir mismatch themselves (`info`), and the
    /// reconnect advice of the loud variant would be wrong for a worktree.
    fn swap_entries(&self, main: &Path) -> ApplicationResult<Vec<PathBuf>> {
        Ok(self
            .swap_service
            .status_quiet(main)?
            .into_iter()
            .map(|f| relative(main, &f.project_path))
            .collect())
    }

    /// Worktree paths that may be links into main or its vault: the borrow set (when
    /// main's vault is reachable, else just `extra`) plus every top-level symlink,
    /// minus `.envrc` (handled on its own).
    ///
    /// Drops paths reached through a symlinked ancestor: `<wt>/.claude/x` behind a
    /// borrowed `.claude` link IS main's `.claude/x`, and removing it would delete
    /// main's own (guard) link.
    fn link_candidates(
        &self,
        wt: &Path,
        main: &Path,
        vault: Option<&Vault>,
        extra: &[String],
    ) -> BTreeSet<PathBuf> {
        let mut candidates: BTreeSet<PathBuf> = match vault {
            Some(v) => self.borrow_set(main, v, extra).unwrap_or_default(),
            None => extra_paths(extra).collect(),
        };
        // Directory enumeration goes through walkdir: `FileSystem` has no read_dir.
        for entry in walkdir::WalkDir::new(wt)
            .min_depth(1)
            .max_depth(1)
            .into_iter()
            .flatten()
        {
            if entry.path_is_symlink() {
                candidates.insert(PathBuf::from(entry.file_name()));
            }
        }
        candidates.remove(Path::new(".envrc"));
        candidates.retain(|p| !self.behind_link(wt, p));
        candidates
    }

    /// Some proper ancestor of `p` inside `wt` is a symlink.
    fn behind_link(&self, wt: &Path, p: &Path) -> bool {
        p.ancestors()
            .skip(1)
            .filter(|a| !a.as_os_str().is_empty())
            .any(|a| self.fs.is_symlink(&wt.join(a)))
    }

    fn plan_envrc(&self, wt: &Path, dot_envrc: &Path) -> AttachItem {
        let path = PathBuf::from(".envrc");
        let link = wt.join(&path);
        if links_into(self.fs.as_ref(), &link, dot_envrc) {
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

    fn plan_borrow(&self, wt: &Path, main: &Path, p: &Path, tracked: bool) -> AttachItem {
        let path = p.to_path_buf();
        let in_main = main.join(p);
        let link = wt.join(p);
        // Tracked first: it decides, whatever main holds. A tracked swap entry that
        // main swapped out is absent there, yet "not in main" would hide the reason.
        if tracked {
            AttachItem::Skip {
                path,
                reason: SkipReason::Tracked,
            }
        } else if !self.fs.exists_or_is_symlink(&in_main) {
            AttachItem::Skip {
                path,
                reason: SkipReason::NotInMain,
            }
        } else if links_into(self.fs.as_ref(), &link, &in_main) {
            AttachItem::Skip {
                path,
                reason: SkipReason::AlreadyLinked,
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

    /// Files of the worktree's git index at or below any of `paths`, in one git call.
    /// Empty when git fails: nothing counts as tracked, as outside git.
    fn tracked(&self, wt: &Path, paths: &BTreeSet<PathBuf>) -> BTreeSet<PathBuf> {
        if paths.is_empty() {
            return BTreeSet::new();
        }
        let wt_str = wt.to_string_lossy();
        let path_strs: Vec<String> = paths
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        // Literal: a borrowed name like `*.local` or `:x` must not act as a pattern.
        let mut args = vec!["--literal-pathspecs", "-C", &wt_str, "ls-files", "-z", "--"];
        args.extend(path_strs.iter().map(String::as_str));
        match self.runner.run("git", &args) {
            Ok(out) if out.status.success() => out
                .stdout
                .split(|b| *b == 0)
                .filter(|f| !f.is_empty())
                .map(|f| PathBuf::from(String::from_utf8_lossy(f).into_owned()))
                .collect(),
            _ => BTreeSet::new(),
        }
    }
}

/// `p` is a tracked file, or a directory with tracked files below it.
fn is_tracked(tracked: &BTreeSet<PathBuf>, p: &Path) -> bool {
    tracked.iter().any(|f| f.starts_with(p))
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
