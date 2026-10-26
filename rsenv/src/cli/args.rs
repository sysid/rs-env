//! CLI argument definitions using clap
//!
//! Help text conventions: the first doc paragraph is the one-line summary shown by `-h`
//! and in command lists; the rest is `--help` only. clap is built without `wrap_help`,
//! so multi-line help is hand-wrapped and marked `verbatim_doc_comment`. Doc lines stay
//! below 4-space indentation so rustdoc does not take them for doctests.

use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueHint};

/// Terms used throughout the help, printed after `rsenv --help`.
const GLOSSARY: &str = "\
TERMS
  project        The directory a command works on: -C DIR, else the current directory.
                 Its vault is found only through PROJECT/.envrc being a symlink to the
                 vault's dot.envrc; parent directories are not searched.
  base dir       Root of all rsenv data: `base_dir` in the global config, default
                 ~/.rsenv. Vaults live in <base_dir>/vaults. Usually a git repository
                 of its own (the \"vault repo\"), so vaults can be versioned and shared.
  vault          One project's data, <base_dir>/vaults/<project>-<id>/, containing:
                   dot.envrc    the real .envrc; PROJECT/.envrc is a symlink to it
                   envs/        env files
                   guarded/     guarded files
                   swap/        swap overrides, backups of originals, swap state
                   .rsenv.toml  vault config (optional)
  dot-names      Inside a vault a leading dot becomes \"dot.\": .envrc -> dot.envrc,
                 .pypirc -> dot.pypirc. Commands take project paths and translate.
  env file       A *.env file of `export KEY=value` lines (other lines are ignored).
                 A `# rsenv: a.env b.env` line names its parents, relative to the file,
                 with $VARS expanded. Building an env file merges its parents first, so
                 a child's value overrides its parents'.
  leaf           An env file that no other env file names as parent: the one you build.
  guard          Move a file into <vault>/guarded/ for good and leave a symlink in its
                 place. For secrets the project needs on disk but must never commit.
  swap           Temporarily replace a project file or directory with a variant kept in
                 <vault>/swap/ (e.g. a dev-only config), and later put the original
                 back. Swapped-in state is per host: one host at a time.
  ciphertext     The SOPS-encrypted copy of a file, <name>.<hash8>.enc next to it, where
                 hash8 is the first 8 hex digits of the plaintext's SHA-256. Commit
                 ciphertexts, never plaintext.
  stale          A ciphertext whose plaintext has changed since it was encrypted.
  encryption patterns
                 The config lists `sops.file_extensions_enc` (env -> *.env) and
                 `sops.file_names_enc` (exact file names). They select the files
                 `sops encrypt` encrypts and the rsenv-managed .gitignore files ignore.

CONFIG LAYERS (each overrides the one above)
  defaults       Compiled in.
  global config  rsenv.toml in the platform config dir (`rsenv config path`). Its lists
                 REPLACE the defaults.
  vault config   <vault>/.rsenv.toml. Its lists ADD to the global ones; \"!x\" removes x.
                 Read only from the project's vault, never from the current directory.
  environment    RSENV__<KEY>, e.g. RSENV__EDITOR, RSENV__SOPS__GPG_KEY. Lists are
                 comma-separated; an empty value clears the list.

THE .gitignore FILES rsenv MANAGES
  rsenv only rewrites the block between `# rsenv-managed start` and `# rsenv-managed
  end`. None of these is git's own global ignore file (core.excludesFile).
  base-dir .gitignore   <base_dir>/.gitignore: the global encryption patterns; covers
                        every vault.          Commands: sops gitignore-* --global
  vault .gitignore      <vault>/.gitignore: the vault config's own patterns; git
                        applies it on top of the base-dir one.
                                              Commands: sops gitignore-*
  directory .gitignore  DIR/.gitignore of a repo outside the base dir: all global
                        patterns.             Commands: sops gitignore-* --dir DIR";

/// Keep a project's env vars, secrets and dev overrides in a vault outside the repo
///
/// rsenv keeps what belongs to a project but must not be committed with it - env vars,
/// secret files, local dev overrides - in the project's vault, a directory under the
/// base dir. The project keeps only a `.envrc` symlink into its vault, which direnv
/// loads. The base dir is usually a git repo of its own, with secrets SOPS-encrypted.
///
/// Typical first steps in a project:
///   rsenv vault init                  create the vault, symlink .envrc into it
///   rsenv env envrc $RSENV_VAULT/envs/local.env
///                                     write an env file's merged vars into .envrc
///   rsenv guard add secrets.json      move a secret into the vault, leave a symlink
///   rsenv swap init config/dev.yml    keep a dev-only variant of a file in the vault
///   rsenv sops encrypt                encrypt vault files matching the patterns
///   rsenv vault commit                commit the vault to the vault repo
///
/// `-h` shows short help, `--help` the full text with the terms below.
#[derive(Parser, Debug)]
#[command(name = "rsenv")]
#[command(author, version, verbatim_doc_comment)]
#[command(after_long_help = GLOSSARY)]
#[command(propagate_version = true)]
pub struct Cli {
    /// Log debug output to stderr
    #[arg(short, long, global = true)]
    pub verbose: bool,

    /// Project directory to work on (default: current directory)
    ///
    /// The project's vault is found through DIR/.envrc. File arguments are still
    /// resolved against the current directory, except `swap` paths, which are relative
    /// to the project.
    #[arg(
        short = 'C',
        long,
        global = true,
        value_name = "DIR",
        verbatim_doc_comment
    )]
    pub project_dir: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Build, inspect and edit hierarchical env files
    ///
    /// An env file can name parent env files; building it merges the whole chain, so
    /// shared settings live in one parent and each environment only states what
    /// differs. Default env files live in <vault>/envs/.
    #[command(verbatim_doc_comment)]
    Env {
        #[command(subcommand)]
        command: EnvCommands,
    },

    /// Move secret files into the vault for good, leaving symlinks in the project
    ///
    /// Use for files the project needs on disk but that must never land in its git
    /// history (credentials, private keys, local settings with tokens). The project
    /// keeps a symlink, the vault holds the file; encrypt it with `rsenv sops encrypt`.
    #[command(verbatim_doc_comment)]
    Guard {
        #[command(subcommand)]
        command: GuardCommands,
    },

    /// Temporarily replace project files with variants kept in the vault
    ///
    /// Use for files that are committed to the project but that you want to differ
    /// locally for a while (a dev config, a patched build file). `swap in` puts the
    /// vault variant in place and backs up the original; `swap out` keeps your edits in
    /// the vault and restores the original, so the project's git status is clean.
    #[command(verbatim_doc_comment)]
    Swap {
        #[command(subcommand)]
        command: SwapCommands,
    },

    /// Create, connect and commit the project's vault
    Vault {
        #[command(subcommand)]
        command: VaultCommands,
    },

    /// Encrypt and decrypt files with SOPS; maintain rsenv's .gitignore blocks
    ///
    /// Encryption keys come from `sops.gpg_key` or `sops.age_key` in the config; which
    /// files are encrypted is set by the encryption patterns. Plaintext stays next to
    /// its ciphertext and is ignored by git through the rsenv-managed .gitignore files
    /// (see `rsenv --help`).
    ///
    /// Scope of encrypt/decrypt/clean/status:
    ///   (default)   the project's vault
    ///   --dir DIR   any directory, e.g. a secrets repo outside the base dir
    ///   --global    every vault under <base_dir>/vaults
    #[command(verbatim_doc_comment)]
    Sops {
        #[command(subcommand)]
        command: SopsCommands,
    },

    /// Show, create and edit the global or vault config
    ///
    /// The global config sets your defaults; a vault config adjusts them for one
    /// project. See CONFIG LAYERS in `rsenv --help`.
    #[command(verbatim_doc_comment)]
    Config {
        #[command(subcommand)]
        command: ConfigCommands,
    },

    /// Install a pre-commit hook that blocks commits of unencrypted files
    ///
    /// The hook runs `rsenv sops status --check` and fails the commit if a file
    /// matching the encryption patterns has no up-to-date ciphertext.
    #[command(verbatim_doc_comment)]
    Hook {
        #[command(subcommand)]
        command: HookCommands,
    },

    /// Show the project's vault, guarded and swapped files, and config
    Info {
        /// Print nothing; exit 0 if the project has a valid vault, else 1
        #[arg(long)]
        check: bool,
    },

    /// Print a shell completion script to stdout
    Completion {
        /// Shell type
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
}

#[derive(Subcommand, Debug)]
pub enum EnvCommands {
    /// Print the merged vars of an env file and its parents as export lines
    ///
    /// Parents are merged first, so the leaf's values win. Use as
    /// `eval "$(rsenv env build FILE)"` or to inspect the result.
    #[command(verbatim_doc_comment)]
    Build {
        /// Leaf env file
        file: PathBuf,
    },

    /// Write the merged vars of an env file into the rsenv section of .envrc
    ///
    /// Replaces only the vars block of the rsenv section that `vault init` added; the
    /// rest of the file is kept. Fails if there is no rsenv section. Run `direnv allow`
    /// afterwards. This is how you switch a project to another environment.
    #[command(verbatim_doc_comment)]
    Envrc {
        /// Leaf env file
        file: PathBuf,
        /// File to update (default: .envrc in the current directory)
        #[arg(short, long)]
        envrc: Option<PathBuf>,
    },

    /// List an env file and all its ancestors, leaf first
    Files {
        /// Leaf env file
        file: PathBuf,
    },

    /// Pick an env file interactively and print its merged vars as export lines
    ///
    /// Offers the *.env files directly in DIR (not in subdirectories); output as for
    /// `env build`.
    #[command(verbatim_doc_comment)]
    Select {
        /// Directory to pick from (default: -C DIR, else the current directory)
        dir: Option<PathBuf>,
    },

    /// Show the env files under DIR as parent-child trees
    ///
    /// Scans DIR recursively. Requires every file to have at most one parent.
    #[command(verbatim_doc_comment)]
    Tree {
        /// Directory to scan (default: -C DIR, else the current directory)
        dir: Option<PathBuf>,
    },

    /// List the root env file of each tree under DIR
    ///
    /// Scans DIR recursively. Requires every file to have at most one parent.
    #[command(verbatim_doc_comment)]
    Branches {
        /// Directory to scan (default: -C DIR, else the current directory)
        dir: Option<PathBuf>,
    },

    /// Pick an env file interactively and open it in the editor
    ///
    /// Offers the *.env files directly in DIR (not in subdirectories).
    #[command(verbatim_doc_comment)]
    Edit {
        /// Directory to pick from (default: -C DIR, else the current directory)
        dir: Option<PathBuf>,
    },

    /// Open an env file and its ancestors side by side in the editor
    ///
    /// Needs a vim-family editor (uses `-O`).
    #[command(verbatim_doc_comment)]
    EditLeaf {
        /// Leaf env file
        file: PathBuf,
    },

    /// Open every tree under DIR in the editor, one column per branch
    ///
    /// Scans DIR recursively. Requires every file to have at most one parent and a
    /// vim-family editor.
    #[command(verbatim_doc_comment)]
    TreeEdit {
        /// Directory to scan (default: -C DIR, else the current directory)
        dir: Option<PathBuf>,
    },

    /// List the leaf env files under DIR
    Leaves {
        /// Directory to scan recursively (default: -C DIR, else the current directory)
        dir: Option<PathBuf>,
    },

    /// Chain env files into a hierarchy: each file becomes the parent of the next
    ///
    /// `rsenv env link base.env int.env int-eu.env` makes base.env a root (its parent
    /// line is cleared), base.env the parent of int.env, and int.env the parent of
    /// int-eu.env. Existing parent lines of the linked files are replaced.
    #[command(verbatim_doc_comment)]
    Link {
        /// Files to link, root first
        #[arg(num_args = 2..)]
        files: Vec<PathBuf>,
    },

    /// Clear an env file's parents (keeps an empty `# rsenv:` line)
    Unlink {
        /// Env file
        file: PathBuf,
    },

    /// Recreate the default env files in <vault>/envs
    ///
    /// Creates none.env, local.env, test.env, int.env, e2e.env and prod.env. Every file
    /// already in <vault>/envs is first renamed to <name>.bkp.<ext>, or deleted with
    /// --clear.
    #[command(verbatim_doc_comment)]
    Init {
        /// Delete existing files instead of backing them up
        #[arg(long)]
        clear: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum GuardCommands {
    /// Move a file into <vault>/guarded/ and leave a symlink in its place
    ///
    /// The file keeps its path relative to the project, with dot-names
    /// (e.g. config/.secret -> <vault>/guarded/config/dot.secret). The vault is found
    /// by searching upward from the file for a .envrc symlink, so this works from any
    /// subdirectory. No .gitignore entry is written: a guarded file is ignored by the
    /// vault repo only if it matches the encryption patterns, so encrypt it or keep it
    /// out of `git add` yourself.
    #[command(verbatim_doc_comment)]
    Add {
        /// File to guard
        file: PathBuf,
        /// Create an absolute symlink (default: relative)
        #[arg(long)]
        absolute: bool,
    },

    /// List the files in <vault>/guarded/ (vault-side names)
    List,

    /// Move a guarded file back into the project, replacing its symlink
    ///
    /// Its ciphertexts stay in the vault; delete them yourself if no longer needed.
    #[command(verbatim_doc_comment)]
    Restore {
        /// Project path of the guarded file (the symlink)
        file: PathBuf,
    },

    /// Delete guarded files for good: vault copy, ciphertexts and project link
    ///
    /// The vault holds the only copy, so this cannot be undone except from committed
    /// vault history. Use `guard restore` to keep the file instead.
    #[command(verbatim_doc_comment)]
    Delete {
        /// Project paths of guarded files (the links), NOT vault paths.
        /// Dotfiles are matched automatically (e.g. .pypirc → dot.pypirc).
        #[arg(verbatim_doc_comment)]
        files: Vec<PathBuf>,
        /// Do not ask for confirmation
        #[arg(short, long)]
        yes: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum SwapCommands {
    /// Put the vault variants of files in place, backing up the originals
    ///
    /// For each path the original (if any) is moved to <vault>/swap/<path>.rsenv_original
    /// and the variant from <vault>/swap/<path> takes its place. A sentinel records that
    /// this host has it swapped in; a path swapped in on another host is refused.
    /// Repeating on the same host is a no-op. Paths are relative to the project.
    #[command(verbatim_doc_comment)]
    In {
        /// Project paths (files or directories) previously set up with `swap init`
        files: Vec<PathBuf>,
    },

    /// Move edited variants back into the vault and restore the originals
    ///
    /// Without paths: every swapped-in entry of the project. With --global: every vault
    /// under <base_dir>/vaults (or --vault-base). Paths are relative to the project.
    #[command(verbatim_doc_comment)]
    Out {
        /// Project paths to swap out (default: all of the project's entries)
        #[arg(conflicts_with = "global")]
        files: Vec<PathBuf>,
        /// Swap out every vault under <base_dir>/vaults, not just the project's
        #[arg(short, long, conflicts_with = "files")]
        global: bool,
        /// Directory of vaults to use with --global (default: <base_dir>/vaults)
        #[arg(long, requires = "global", value_name = "DIR")]
        vault_base: Option<PathBuf>,
    },

    /// Move project files into <vault>/swap/ as variants (one-time setup)
    ///
    /// The path is removed from the project; `swap in` puts the vault variant back.
    /// Dotfiles inside directories are renamed to dot-names. Paths are relative to the
    /// project.
    #[command(verbatim_doc_comment)]
    Init {
        /// Project paths (files or directories) to move into the vault
        files: Vec<PathBuf>,
    },

    /// Show which swap entries are swapped in, and on which host
    Status {
        /// Show absolute paths (relative paths are default)
        #[arg(long)]
        absolute: bool,
        /// Show every vault under <base_dir>/vaults, not just the project's
        #[arg(short, long)]
        global: bool,
        /// Print nothing; exit 0 = all swapped out, 1 = something swapped in, 2 = no vault
        #[arg(short, long)]
        silent: bool,
        /// Directory of vaults to use with --global (default: <base_dir>/vaults)
        #[arg(long, requires = "global", value_name = "DIR")]
        vault_base: Option<PathBuf>,
    },

    /// Show what changed in swapped-in files since they were swapped in
    #[command(verbatim_doc_comment)]
    Diff {
        /// Paths to diff: a swapped entry, a path inside one, or an ancestor.
        /// Selects everything at or below it, like a git pathspec.
        /// If empty, diffs all swapped-in entries in the current vault.
        #[arg(verbatim_doc_comment)]
        files: Vec<PathBuf>,
        /// Show the patch (default, like `git diff`; accepted for explicitness)
        #[arg(short, long, conflicts_with = "stat")]
        patch: bool,
        /// Show only the summary of changed files, without the patch
        #[arg(long)]
        stat: bool,
        /// Do not pipe the patch through the pager
        #[arg(long)]
        no_pager: bool,
        /// Show absolute paths (relative paths are default)
        #[arg(long)]
        absolute: bool,
        /// Silent mode: exit code only (0=clean, 1=changes, 2=unmanaged)
        #[arg(short, long)]
        silent: bool,
    },

    /// Remove entries from the vault; the project returns to its swapped-out state
    ///
    /// Swapped out: the vault override, its backup and ciphertexts are deleted, the
    /// project file is untouched. Swapped in on this host: the swapped-in content is
    /// discarded and the original restored. Swapped in elsewhere, or unknown paths:
    /// refused, and nothing is deleted.
    #[command(verbatim_doc_comment)]
    Delete {
        /// Project paths (as you'd pass to `swap in`/`out`), NOT vault paths.
        /// Dotfiles are matched automatically (e.g. .github → dot.github).
        #[arg(verbatim_doc_comment)]
        files: Vec<PathBuf>,
        /// Do not ask for confirmation
        #[arg(short, long)]
        yes: bool,
    },
}

/// Vault subcommands
#[derive(Subcommand, Debug)]
pub enum VaultCommands {
    /// Create the project's vault and symlink .envrc into it
    ///
    /// Creates <base_dir>/vaults/<project>-<id>/ with envs/, guarded/ and swap/ and the
    /// default env files. An existing .envrc moves into the vault as dot.envrc and keeps
    /// its content; an rsenv section (RSENV_VAULT and friends) is added to it. Finally
    /// syncs the base-dir .gitignore. If the project already has a vault, prints it.
    #[command(verbatim_doc_comment)]
    Init {
        /// Project directory (default: -C DIR, else the current directory)
        project: Option<PathBuf>,
        /// Create an absolute .envrc symlink (default: relative)
        #[arg(long)]
        absolute: bool,
    },

    /// Undo `vault init`: put guarded files and .envrc back into the project
    ///
    /// Asks first. Restores every guarded file, replaces the .envrc symlink by a plain
    /// .envrc without the rsenv section. The vault directory itself is kept. Swap
    /// entries are not touched: run `rsenv swap out` before.
    #[command(verbatim_doc_comment)]
    Reset {
        /// Project directory (default: -C DIR, else the current directory)
        project: Option<PathBuf>,
    },

    /// Re-create the project's .envrc symlink to an existing vault
    ///
    /// Use after a fresh clone of the project or after moving it. The project is
    /// -C DIR or the current directory; the vault's record of the project path is
    /// updated.
    #[command(verbatim_doc_comment)]
    Reconnect {
        /// The vault's dot.envrc, e.g. ~/.rsenv/vaults/myproj-1a2b3c4d/dot.envrc
        #[arg(value_hint = ValueHint::FilePath)]
        envrc_path: PathBuf,
    },

    /// Commit this project's vault data to the vault repo
    ///
    /// Requires the project to be swapped out (`rsenv swap out`); refuses and names the
    /// swapped-in paths otherwise. Makes one commit scoped to this project's vault
    /// directory, stamped with the project's HEAD as the link back to the project state.
    #[command(verbatim_doc_comment)]
    Commit {
        /// Use a generated commit message instead of opening the editor
        #[arg(short = 'a', long)]
        auto: bool,
        /// Push the vault repo after committing
        #[arg(long)]
        push: bool,
        /// Skip re-encryption, overriding sops.encrypt_on_commit
        #[arg(long)]
        no_encrypt: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum SopsCommands {
    /// Encrypt files matching the encryption patterns, or one given file
    ///
    /// Searches the scope recursively. Each match gets a ciphertext <name>.<hash8>.enc
    /// next to it; files whose ciphertext is current are skipped, and a stale
    /// ciphertext is replaced. A FILE is encrypted whether or not it matches.
    /// With --dir, DIR/.gitignore is synced first if it already has an rsenv block.
    #[command(verbatim_doc_comment)]
    Encrypt {
        /// One file to encrypt, regardless of the patterns
        #[arg(conflicts_with_all = ["dir", "global"])]
        file: Option<PathBuf>,
        /// Encrypt under DIR instead of the project's vault
        #[arg(short, long, conflicts_with_all = ["file", "global"])]
        dir: Option<PathBuf>,
        /// Encrypt in every vault under <base_dir>/vaults
        #[arg(short, long, conflicts_with_all = ["file", "dir"])]
        global: bool,
        /// Directory of vaults to use with --global (default: <base_dir>/vaults)
        #[arg(long, requires = "global", value_name = "DIR")]
        vault_base: Option<PathBuf>,
    },

    /// Decrypt ciphertexts back to plaintext, or one given .enc file
    ///
    /// Use after cloning the vault repo on a new machine. Existing plaintext that
    /// differs from its ciphertext is kept and listed unless --force. Continues past
    /// failures and exits 70 if any file failed.
    #[command(verbatim_doc_comment)]
    Decrypt {
        /// One .enc file to decrypt
        #[arg(conflicts_with_all = ["dir", "global"])]
        file: Option<PathBuf>,
        /// Decrypt under DIR instead of the project's vault
        #[arg(short, long, conflicts_with_all = ["file", "global"])]
        dir: Option<PathBuf>,
        /// Decrypt in every vault under <base_dir>/vaults
        #[arg(short, long, conflicts_with_all = ["file", "dir"])]
        global: bool,
        /// Directory of vaults to use with --global (default: <base_dir>/vaults)
        #[arg(long, requires = "global", value_name = "DIR")]
        vault_base: Option<PathBuf>,
        /// Overwrite existing plaintext that differs from its ciphertext
        /// (by default such files are kept and listed)
        #[arg(long, verbatim_doc_comment)]
        force: bool,
    },

    /// Delete plaintext files that have a current ciphertext
    ///
    /// Only deletes a plaintext whose content still matches its ciphertext, so nothing
    /// unencrypted is lost. Use to leave only ciphertexts on disk.
    #[command(verbatim_doc_comment)]
    Clean {
        /// Clean under DIR instead of the project's vault
        #[arg(short, long, conflicts_with = "global")]
        dir: Option<PathBuf>,
        /// Clean every vault under <base_dir>/vaults
        #[arg(short, long, conflicts_with = "dir")]
        global: bool,
        /// Directory of vaults to use with --global (default: <base_dir>/vaults)
        #[arg(long, requires = "global", value_name = "DIR")]
        vault_base: Option<PathBuf>,
    },

    /// Show which files are encrypted, need encrypting, or have stale ciphertexts
    ///
    /// Categories: current (ciphertext matches), stale (plaintext changed since),
    /// pending (matching file without ciphertext), orphaned (ciphertext without
    /// plaintext, e.g. after `sops clean`).
    #[command(verbatim_doc_comment)]
    Status {
        /// Check DIR instead of the project's vault
        #[arg(short, long, conflicts_with = "global")]
        dir: Option<PathBuf>,
        /// Check every vault under <base_dir>/vaults
        #[arg(short, long, conflicts_with = "dir")]
        global: bool,
        /// Directory of vaults to use with --global (default: <base_dir>/vaults)
        #[arg(long, requires = "global", value_name = "DIR")]
        vault_base: Option<PathBuf>,
        /// Exit 1 if any file is pending or stale (for scripts and hooks)
        #[arg(long)]
        check: bool,
    },

    /// Write the encryption patterns into the rsenv block of a .gitignore
    ///
    /// Keeps plaintext secrets out of git: every file the patterns select for
    /// encryption is ignored, its ciphertext is not. Shows the change and asks first.
    ///
    /// Which .gitignore (none is git's global core.excludesFile):
    ///   (default)   <vault>/.gitignore     the vault config's own patterns; "!x" there
    ///                                      un-ignores x. Needs a <vault>/.rsenv.toml.
    ///   --global    <base_dir>/.gitignore  the global config's patterns; covers every
    ///                                      vault in the vault repo
    ///   --dir DIR   DIR/.gitignore         all global patterns, for a repo outside
    ///                                      the base dir
    ///
    /// Run it after editing a config file by hand. `config edit` syncs the matching
    /// .gitignore itself, `vault init` the base-dir one.
    #[command(name = "gitignore-sync", verbatim_doc_comment)]
    GitignoreSync {
        /// Do not ask for confirmation
        #[arg(short, long)]
        yes: bool,
        /// Sync the base-dir .gitignore, <base_dir>/.gitignore, instead
        #[arg(long, conflicts_with = "dir")]
        global: bool,
        /// Sync DIR/.gitignore instead (a repo outside the base dir)
        #[arg(short, long)]
        dir: Option<PathBuf>,
    },

    /// Show whether a .gitignore's rsenv block matches the config
    ///
    /// Same targets as `gitignore-sync`: <vault>/.gitignore by default,
    /// <base_dir>/.gitignore with --global, DIR/.gitignore with --dir.
    #[command(name = "gitignore-status", verbatim_doc_comment)]
    GitignoreStatus {
        /// Check the base-dir .gitignore, <base_dir>/.gitignore, instead
        #[arg(long, conflicts_with = "dir")]
        global: bool,
        /// Check DIR/.gitignore instead (a repo outside the base dir)
        #[arg(short, long)]
        dir: Option<PathBuf>,
    },

    /// Remove the rsenv block from a .gitignore, keeping the rest of the file
    ///
    /// Same targets as `gitignore-sync`: <vault>/.gitignore by default,
    /// <base_dir>/.gitignore with --global, DIR/.gitignore with --dir.
    #[command(name = "gitignore-clean", verbatim_doc_comment)]
    GitignoreClean {
        /// Clean the base-dir .gitignore, <base_dir>/.gitignore, instead
        #[arg(long, conflicts_with = "dir")]
        global: bool,
        /// Clean DIR/.gitignore instead (a repo outside the base dir)
        #[arg(short, long)]
        dir: Option<PathBuf>,
    },
}

#[derive(Subcommand, Debug)]
pub enum ConfigCommands {
    /// Print the effective config: all layers merged, for the project's vault
    Show,

    /// Create a commented config file: the vault config, or the global one with -g
    ///
    /// Refuses to overwrite an existing file. The vault config is <vault>/.rsenv.toml.
    #[command(verbatim_doc_comment)]
    Init {
        /// Create the global config instead
        #[arg(short, long)]
        global: bool,
    },

    /// Print the paths of the global and vault config files
    Path,

    /// Open the vault config, or the global one with -g, in the editor
    ///
    /// Creates the file from the template if missing. Afterwards syncs the matching
    /// .gitignore: <vault>/.gitignore, or <base_dir>/.gitignore with -g.
    #[command(verbatim_doc_comment)]
    Edit {
        /// Edit the global config instead
        #[arg(short, long)]
        global: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum HookCommands {
    /// Install the pre-commit hook in a git repo (default: the base dir)
    ///
    /// In the base dir the hook checks every vault (`sops status --global --check`);
    /// in any other repo it checks that repo (`sops status --dir <repo> --check`).
    /// If rsenv is not on PATH at commit time, the hook warns and lets the commit pass.
    #[command(verbatim_doc_comment)]
    Install {
        /// Overwrite an existing pre-commit hook
        #[arg(short, long)]
        force: bool,
        /// Git repo to install into (default: the base dir)
        #[arg(long)]
        dir: Option<PathBuf>,
    },

    /// Remove rsenv's pre-commit hook (a foreign hook is left alone)
    Remove {
        /// Git repo to remove it from (default: the base dir)
        #[arg(long)]
        dir: Option<PathBuf>,
    },

    /// Show whether rsenv's pre-commit hook is installed
    Status {
        /// Git repo to check (default: the base dir)
        #[arg(long)]
        dir: Option<PathBuf>,
    },
}
