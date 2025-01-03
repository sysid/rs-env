use clap::{Parser, Subcommand, ValueHint};
use clap_complete::Shell;

#[derive(Parser, Debug, PartialEq)]
#[command(author, version, about, long_about = None)] // Read from `Cargo.toml`
#[command(arg_required_else_help = true)]
/// A security guard for your config files
pub struct Cli {
    /// Optional name to operate on
    name: Option<String>,

    /// Turn debugging information on (multiple -d flags increase verbosity)
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub debug: u8,

    /// Generate shell completions
    #[arg(long = "generate", value_enum)]
    pub generator: Option<Shell>,

    /// Show configuration information
    #[arg(long = "info")]
    pub info: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug, PartialEq)]
pub enum Commands {
    /// Build the resulting set of environment variables (DAG/Tree)
    Build {
        /// path to environment file (last child in hierarchy, leaf node)
        #[arg(value_hint = ValueHint::FilePath)]
        source_path: String,
    },
    /// Write the resulting set of variables to .envrc (requires direnv, DAG/Tree)
    Envrc {
        /// path to environment file (last child in hierarchy, leaf node)
        #[arg(value_hint = ValueHint::FilePath)]
        source_path: String,
        /// path to .envrc file
        #[arg(value_hint = ValueHint::FilePath)]
        envrc_path: Option<String>,
    },
    /// Show all files involved in resulting set (DAG/Tree)
    Files {
        /// path to environment file (last child in hierarchy)
        #[arg(value_hint = ValueHint::FilePath)]
        source_path: String,
    },
    /// Edit the given environment file and all its parents (DAG/Tree)
    EditLeaf {
        /// path to environment file (Leaf)
        #[arg(value_hint = ValueHint::FilePath)]
        source_path: String,
    },
    /// Edit the FZF selected branch/DAG
    Edit {
        /// path to environment files directory
        #[arg(value_hint = ValueHint::DirPath)]
        source_dir: String,
    },
    /// select environment/branch and update .envrc file (requires direnv, DAG/Tree)
    SelectLeaf {
        /// path to environment file (leaf node))
        #[arg(value_hint = ValueHint::DirPath)]
        source_path: String,
    },
    /// FZF based selection of environment/branch and update of .envrc file (requires direnv, DAG/Tree)
    Select {
        /// path to environment directory
        #[arg(value_hint = ValueHint::DirPath)]
        source_dir: String,
    },
    /// Link files into a linear dependency branch (root -> parent -> child).
    Link {
        /// .env files to link (root -> parent -> child)
        #[arg(value_hint = ValueHint::FilePath, num_args = 1..)]
        nodes: Vec<String>,
    },
    /// Show all branches (linear representation)
    Branches {
        /// path to root directory for environment files
        #[arg(value_hint = ValueHint::DirPath)]
        source_dir: String,
    },
    /// Show all trees (hierarchical representation)
    Tree {
        /// path to root directory for environment files
        #[arg(value_hint = ValueHint::DirPath)]
        source_dir: String,
    },
    /// Edit branches of all trees side-by-side (vim required in path)
    TreeEdit {
        /// path to root directory for environment files
        #[arg(value_hint = ValueHint::DirPath)]
        source_dir: String,
    },
    /// Output leaves as paths (Tree)
    Leaves {
        /// path to root directory for environment files
        #[arg(value_hint = ValueHint::DirPath)]
        source_dir: String,
    },
}