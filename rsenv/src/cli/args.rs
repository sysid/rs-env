//! CLI argument definitions using clap

use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueHint};

/// Unified development environment manager: hierarchical env vars, file guarding, and swap-in/out
#[derive(Parser, Debug)]
#[command(name = "rsenv")]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Cli {
    /// Enable verbose output
    #[arg(short, long, global = true)]
    pub verbose: bool,

    /// Project directory (default: cwd)
    #[arg(short = 'C', long, global = true)]
    pub project_dir: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Initialize vault for project
    Init {
        #[command(subcommand)]
        command: InitCommands,
    },

    /// Manage hierarchical env vars
    Env {
        #[command(subcommand)]
        command: EnvCommands,
    },

    /// Protect files via vault symlinks
    Guard {
        #[command(subcommand)]
        command: GuardCommands,
    },

    /// Swap dev overrides in/out
    Swap {
        #[command(subcommand)]
        command: SwapCommands,
    },

    /// Encrypt/decrypt vault files
    Sops {
        #[command(subcommand)]
        command: SopsCommands,
    },

    /// Manage settings
    Config {
        #[command(subcommand)]
        command: ConfigCommands,
    },

    /// Show status
    Info,

    /// Generate shell completions
    Completion {
        /// Shell type
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
}

/// Init subcommands
#[derive(Subcommand, Debug)]
pub enum InitCommands {
    /// Create vault for project
    Vault {
        /// Project directory
        project: Option<PathBuf>,
        /// Use absolute symlink paths
        #[arg(long)]
        absolute: bool,
    },
    /// Undo init: restore files, remove .envrc symlink (vault kept)
    Reset {
        /// Project directory
        project: Option<PathBuf>,
    },
    /// Reconnect a project to its vault (re-create .envrc symlink)
    Reconnect {
        /// Path to dot.envrc file in vault
        #[arg(value_hint = ValueHint::FilePath)]
        envrc_path: PathBuf,
    },
}

#[derive(Subcommand, Debug)]
pub enum EnvCommands {
    /// Merge env hierarchy to stdout
    Build {
        /// Leaf env file
        file: PathBuf,
    },

    /// Write merged env to .envrc
    Envrc {
        /// Leaf env file
        file: PathBuf,
        /// Target .envrc file
        #[arg(short, long)]
        envrc: Option<PathBuf>,
    },

    /// List files in hierarchy
    Files {
        /// Leaf env file
        file: PathBuf,
    },

    /// Select env interactively (fzf)
    Select {
        /// Directory to search
        dir: Option<PathBuf>,
    },

    /// Show hierarchy as tree
    Tree {
        /// Directory
        dir: Option<PathBuf>,
    },

    /// Show all branches linearly
    Branches {
        /// Directory
        dir: Option<PathBuf>,
    },

    /// Edit file (fzf select)
    Edit {
        /// Directory to search
        dir: Option<PathBuf>,
    },

    /// Edit leaf and its parents
    EditLeaf {
        /// Leaf env file
        file: PathBuf,
    },

    /// Edit all hierarchies side-by-side
    TreeEdit {
        /// Directory
        dir: Option<PathBuf>,
    },

    /// List leaf files
    Leaves {
        /// Directory
        dir: Option<PathBuf>,
    },

    /// Link files: parent <- child
    Link {
        /// Files to link (first=root, rest chain to previous)
        #[arg(num_args = 2..)]
        files: Vec<PathBuf>,
    },

    /// Remove parent link
    Unlink {
        /// Env file
        file: PathBuf,
    },
}

#[derive(Subcommand, Debug)]
pub enum GuardCommands {
    /// Move file to vault, symlink back
    Add {
        /// File to guard
        file: PathBuf,
        /// Use absolute symlink paths
        #[arg(long)]
        absolute: bool,
    },

    /// List guarded files
    List,

    /// Restore file from vault
    Restore {
        /// File to restore
        file: PathBuf,
    },
}

#[derive(Subcommand, Debug)]
pub enum SwapCommands {
    /// Replace with vault versions
    In {
        /// Files to swap in
        files: Vec<PathBuf>,
    },

    /// Restore originals
    Out {
        /// Files to swap out
        files: Vec<PathBuf>,
    },

    /// Move files to vault (first time)
    Init {
        /// Files to initialize
        files: Vec<PathBuf>,
    },

    /// Show swap status
    Status {
        /// Show absolute paths (relative paths are default)
        #[arg(long)]
        absolute: bool,
    },

    /// Restore all projects under dir
    AllOut {
        /// Base directory
        base_dir: Option<PathBuf>,
    },

    /// Show swap status across all vaults
    AllStatus {
        /// Base directory containing vaults (default: vault_base_dir from settings)
        base_dir: Option<PathBuf>,
        /// Silent mode: return exit code only (0=clean, 1=has active swaps)
        #[arg(short, long)]
        silent: bool,
    },

    /// Remove files from swap management
    Delete {
        /// Files to delete
        files: Vec<PathBuf>,
    },
}

#[derive(Subcommand, Debug)]
pub enum SopsCommands {
    /// Encrypt matching files
    Encrypt {
        /// Directory (default: project vault)
        #[arg(short, long, conflicts_with = "global")]
        dir: Option<PathBuf>,
        /// Encrypt all vaults (entire vault_base_dir)
        #[arg(short, long, conflicts_with = "dir")]
        global: bool,
    },

    /// Decrypt .enc files
    Decrypt {
        /// Directory (default: project vault)
        #[arg(short, long, conflicts_with = "global")]
        dir: Option<PathBuf>,
        /// Decrypt all vaults (entire vault_base_dir)
        #[arg(short, long, conflicts_with = "dir")]
        global: bool,
    },

    /// Delete unencrypted originals
    Clean {
        /// Directory (default: project vault)
        #[arg(short, long, conflicts_with = "global")]
        dir: Option<PathBuf>,
        /// Clean all vaults (entire vault_base_dir)
        #[arg(short, long, conflicts_with = "dir")]
        global: bool,
    },

    /// Show encryption status
    Status {
        /// Directory (default: project vault)
        #[arg(short, long, conflicts_with = "global")]
        dir: Option<PathBuf>,
        /// Show status for all vaults (entire vault_base_dir)
        #[arg(short, long, conflicts_with = "dir")]
        global: bool,
    },

    /// Sync .gitignore with config patterns
    #[command(name = "gitignore-sync")]
    GitignoreSync {
        /// Skip confirmation prompt
        #[arg(short, long)]
        yes: bool,
        /// Only sync global gitignore (skip per-vault)
        #[arg(long)]
        global: bool,
    },

    /// Show gitignore sync status
    #[command(name = "gitignore-status")]
    GitignoreStatus {
        /// Only show global gitignore status
        #[arg(long)]
        global: bool,
    },

    /// Remove rsenv-managed section from .gitignore
    #[command(name = "gitignore-clean")]
    GitignoreClean {
        /// Only clean global gitignore
        #[arg(long)]
        global: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum ConfigCommands {
    /// Show merged config
    Show,

    /// Create config template
    Init {
        /// Create global config
        #[arg(short, long)]
        global: bool,
    },

    /// Show config paths
    Path,

    /// Edit config file
    Edit {
        /// Edit global config
        #[arg(short, long)]
        global: bool,
    },
}
