use clap::{Parser, Subcommand, ValueHint};
use clap_complete::Shell;

#[derive(Parser, Debug, PartialEq)]
#[command(author, version, about, long_about = None)] // Read from `Cargo.toml`
#[command(arg_required_else_help = true)]
/// A hierarchical environment variable manager for configuration files
pub struct Cli {
    /// Name of the configuration to operate on (optional)
    name: Option<String>,

    /// Enable debug logging. Multiple flags (-d, -dd, -ddd) increase verbosity
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub debug: u8,

    /// Generate shell completion scripts
    #[arg(long = "generate", value_enum)]
    pub generator: Option<Shell>,

    /// Display version and configuration information
    #[arg(long = "info")]
    pub info: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug, PartialEq)]
pub enum Commands {
    /// Build and display the complete set of environment variables
    Build {
        /// Path to the last linked environment file (leaf node in hierarchy)
        #[arg(value_hint = ValueHint::FilePath)]
        source_path: String,
    },
    /// Write environment variables to .envrc file (requires direnv)
    Envrc {
        /// Path to the last linked environment file (leaf node in hierarchy)
        #[arg(value_hint = ValueHint::FilePath)]
        source_path: String,
        /// path to .envrc file
        #[arg(value_hint = ValueHint::FilePath)]
        envrc_path: Option<String>,
    },
    /// List all files in the environment hierarchy
    Files {
        /// Path to the last linked environment file (leaf node in hierarchy)
        #[arg(value_hint = ValueHint::FilePath)]
        source_path: String,
    },
    /// Edit an environment file and all its parent files
    EditLeaf {
        /// Path to the last linked environment file (leaf node in hierarchy)
        #[arg(value_hint = ValueHint::FilePath)]
        source_path: String,
    },
    /// Interactively select and edit an environment hierarchy
    Edit {
        /// Directory containing environment files
        #[arg(value_hint = ValueHint::DirPath)]
        source_dir: String,
    },
    /// Update .envrc with selected environment (requires direnv)
    SelectLeaf {
        /// Path to the leaf environment file
        #[arg(value_hint = ValueHint::DirPath)]
        source_path: String,
    },
    /// Interactively select environment and update .envrc (requires direnv)
    Select {
        /// Directory containing environment files
        #[arg(value_hint = ValueHint::DirPath)]
        source_dir: String,
    },
    /// Create parent-child relationships between environment files
    Link {
        /// Environment files to link (root -> parent -> child)
        #[arg(value_hint = ValueHint::FilePath, num_args = 1..)]
        nodes: Vec<String>,
    },
    /// Show all branches (linear representation)
    Branches {
        /// Root directory containing environment files
        #[arg(value_hint = ValueHint::DirPath)]
        source_dir: String,
    },
    /// Show all trees (hierarchical representation)
    Tree {
        /// Root directory containing environment files
        #[arg(value_hint = ValueHint::DirPath)]
        source_dir: String,
    },
    /// Edit all environment hierarchies side-by-side (requires vim)
    TreeEdit {
        /// Root directory containing environment files
        #[arg(value_hint = ValueHint::DirPath)]
        source_dir: String,
    },
    /// List all leaf environment files
    Leaves {
        /// Root directory containing environment files
        #[arg(value_hint = ValueHint::DirPath)]
        source_dir: String,
    },
}