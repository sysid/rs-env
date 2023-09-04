#![allow(unused_imports)]

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use anyhow::{Context, Result};
use log::{debug, info};
use std::{env, io, process};
use std::cell::RefCell;
use std::rc::Rc;
use camino::{Utf8Path, Utf8PathBuf};
use camino_tempfile::tempfile;
use camino_tempfile::NamedUtf8TempFile;
use clap::{Args, Command, CommandFactory, Parser, Subcommand, ValueHint};
use clap_complete::{generate, Generator, Shell};
use stdext::function_name;
use rsenv::{build_env_vars, dlog, get_files, is_dag, link, link_all, print_files};
use rsenv::edit::{create_branches, create_vimscript, open_files_in_editor, select_file_with_suffix};
use rsenv::envrc::update_dot_envrc;
use rsenv::tree::{build_trees, TreeNode};
use rsenv::tree_stack::transform_tree_recursive;
use colored::Colorize;

// fn main() {
//     println!("Hello, world!");
// }
#[derive(Parser, Debug, PartialEq)]
#[command(author, version, about, long_about = None)] // Read from `Cargo.toml`
#[command(arg_required_else_help = true)]
/// A security guard for your config files
struct Cli {
    /// Optional name to operate on
    name: Option<String>,

    /// Turn debugging information on
    #[arg(short, long, action = clap::ArgAction::Count)]
    debug: u8,

    // If provided, outputs the completion file for given shell
    #[arg(long = "generate", value_enum)]
    generator: Option<Shell>,

    #[arg(long = "info")]
    info: bool,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug, PartialEq)]
enum Commands {
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
    /// Edit the FZF selected branch/DAG
    Edit {
        /// path to environment files directory
        #[arg(value_hint = ValueHint::DirPath)]
        source_dir: String,
    },
    /// FZF based selection of environment/branch and update of .envrc file (requires direnv, DAG/Tree)
    Select {
        /// path to environment file (leaf node))
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
}

fn print_completions<G: Generator>(gen: G, cmd: &mut Command) {
    generate(gen, cmd, cmd.get_name().to_string(), &mut io::stdout());
}

fn main() {
    let cli = Cli::parse();

    if let Some(generator) = cli.generator {
        let mut cmd = Cli::command();
        eprintln!("Generating completion file for {generator:?}...");
        print_completions(generator, &mut cmd);
    }
    if cli.info {
        use clap::CommandFactory; // Trait which returns the current command
        Cli::command()
            .get_author()
            .map(|a| println!("AUTHOR: {}", a));
        Cli::command()
            .get_version()
            .map(|v| println!("VERSION: {}", v));
    }

    set_logger(&cli);

    match &cli.command {
        Some(Commands::Build {
                 source_path,
             }) => _build(source_path),
        Some(Commands::Envrc {
                 source_path,
                 envrc_path,
             }) => _envrc(source_path, envrc_path.as_deref()),
        Some(Commands::Files {
                 source_path,
             }) => _files(source_path),
        Some(Commands::Edit {
                 source_dir,
             }) => _edit(source_dir),
        Some(Commands::Select {
                 source_dir,
             }) => _select(source_dir),
        Some(Commands::Link {
                 nodes,
             }) => _link(nodes),
        Some(Commands::Branches {
                 source_dir,
             }) => _branches(source_dir),
        Some(Commands::Tree {
                 source_dir,
             }) => _tree(source_dir),
        Some(Commands::TreeEdit {
                 source_dir,
             }) => _tree_edit(source_dir),
        None => {
            // println!("{cli:#?}", cli = cli);
            // println!("{cli:#?}");  // prints current CLI attributes
        } // Commands::ValueHint(_) => todo!(),
    }
}

fn _build(source_path: &str) {
    dlog!("source_path: {:?}", source_path);
    let vars = build_env_vars(source_path).unwrap_or_else(|e| {
        eprintln!(
            "{}",
            format!("Cannot build environment: {}", e).red()
        );
        process::exit(1);
    });
    println!("{}", vars);
}

fn _envrc(source_path: &str, envrc_path: Option<&str>) {
    let envrc_path = envrc_path.unwrap_or(".envrc");
    dlog!("source_path: {:?}, envrc_path: {:?}", source_path, envrc_path);
    let vars = build_env_vars(source_path).unwrap_or_else(|e| {
        eprintln!(
            "{}",
            format!("Cannot build environment: {}", e).red()
        );
        process::exit(1);
    });
    update_dot_envrc(Utf8Path::new(envrc_path), vars.as_str()).unwrap();
}

fn _files(source_path: &str) {
    dlog!("source_path: {:?}", source_path);
    print_files(source_path).unwrap_or_else(|e| {
        eprintln!(
            "{}",
            format!("Cannot print environment: {}", e).red()
        );
        process::exit(1);
    });
}

fn _edit(source_dir: &str) {
    if !Utf8Path::new(source_dir).exists() {
        eprintln!("Error: Directory does not exist: {:?}", source_dir);
        return;
    }
    let selected_file = select_file_with_suffix(source_dir, ".env").unwrap_or_else(|| {
        eprintln!(
            "{}",
            format!("No .env files found").red()
        );
        process::exit(1);
    });
    println!("Selected: {}", &selected_file);
    let files = get_files(selected_file.as_str()).unwrap_or_else(|e| {
        eprintln!(
            "{}",
            format!("Cannot get files: {}", e).red()
        );
        process::exit(1);
    });
    open_files_in_editor(files).unwrap_or_else(|e| {
        eprintln!(
            "{}",
            format!("Cannot open files in editor: {}", e).red()
        );
        process::exit(1);
    });
}

fn _select(source_dir: &str) {
    if !Utf8Path::new(source_dir).exists() {
        eprintln!("Error: Directory does not exist: {:?}", source_dir);
        return;
    }
    let selected_file = select_file_with_suffix(source_dir, ".env").unwrap_or_else(|| {
        eprintln!(
            "{}",
            format!("No .env files found.").red()
        );
        process::exit(1);
    });
    println!("Selected: {}", &selected_file);
    _envrc(selected_file.as_str(), None);
}

fn _link(nodes: &[String]) {
    link_all(nodes);
    println!("Linked: {:?}", nodes.join(" <- "));
}

fn _branches(source_path: &str) {
    dlog!("source_path: {:?}", source_path);
    if is_dag(source_path).expect("Failed to determine if DAG") {
        eprintln!("{}", format!("Dependencies form a DAG, you cannot use tree based commands.", ).red());
        process::exit(1);
    }
    let trees = build_trees(Utf8Path::new(source_path)).unwrap_or_else(|e| {
        eprintln!(
            "{}",
            format!("Cannot build trees: {}", e).red()
        );
        process::exit(1);
    });
    println!("Found {} trees:\n", trees.len());
    for tree in &trees {
        let p = &tree.borrow().node_data.file_path;
        let mut path = vec![p.to_string()];
        println!("Tree Root: {}", tree.borrow().node_data.file_path);
        tree.borrow().print_leaf_paths(&mut path);
        println!();
    }
}


fn _tree(source_path: &str) {
    dlog!("source_path: {:?}", source_path);
    if is_dag(source_path).expect("Failed to determine if DAG") {
        eprintln!("{}", format!("Dependencies form a DAG, you cannot use tree based commands.", ).red());
        process::exit(1);
    }
    let trees = build_trees(Utf8Path::new(source_path)).unwrap_or_else(|e| {
        eprintln!(
            "{}",
            format!("Cannot build trees: {}", e).red()
        );
        process::exit(1);
    });
    println!("Found {} trees:\n", trees.len());
    for tree in &trees {
        println!("{}", transform_tree_recursive(tree));
    }
}


fn _tree_edit(source_path: &str) {
    // vim -O3 test.env int.env prod.env -c "wincmd h" -c "sp test.env" -c "wincmd l" -c "sp int.env" -c "wincmd l" -c "sp prod.env"
    dlog!("source_path: {:?}", source_path);
    if is_dag(source_path).expect("Failed to determine if DAG") {
        eprintln!("{}", format!("Dependencies form a DAG, you cannot use tree based commands.", ).red());
        process::exit(1);
    }
    let trees = build_trees(Utf8Path::new(source_path)).unwrap_or_else(|e| {
        eprintln!(
            "{}",
            format!("Cannot build trees: {}", e).red()
        );
        process::exit(1);
    });
    println!("Editing {} trees...", trees.len());

    let vimscript_files: Vec<Vec<_>> = create_branches(&trees);

    let vimscript = create_vimscript(vimscript_files.iter().map(|v| v.iter().map(|s| s.as_str()).collect()).collect());
    // Create a temporary file.
    let mut tmpfile = NamedUtf8TempFile::new().unwrap();
    tmpfile.write_all(vimscript.as_bytes()).unwrap();

    let status = std::process::Command::new("vim")
        .arg("-S")
        .arg(tmpfile.path())
        .status()
        .expect("failed to run vim");

    println!("Vim: {}", status.to_string());
}


fn set_logger(cli: &Cli) {
    // Note, only flags can have multiple occurrences
    match cli.debug {
        0 => {
            let _ = env_logger::builder()
                .filter_level(log::LevelFilter::Warn)
                .try_init();
        }
        1 => {
            let _ = env_logger::builder()
                .filter_level(log::LevelFilter::Info)
                .try_init();
            info!("Debug mode: info");
        }
        2 => {
            let _ = env_logger::builder()
                .filter_level(log::LevelFilter::max())
                .try_init();
            debug!("Debug mode: debug");
        }
        _ => eprintln!("Don't be crazy"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[ctor::ctor]
    fn init() {
        let _ = env_logger::builder()
            .filter_level(log::LevelFilter::max())
            .is_test(true)
            .try_init();
    }

    // https://docs.rs/clap/latest/clap/_derive/_tutorial/index.html#testing
    #[test]
    fn verify_cli() {
        use clap::CommandFactory;
        Cli::command().debug_assert()
    }
}
