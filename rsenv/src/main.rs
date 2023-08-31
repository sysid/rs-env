#![allow(unused_imports)]

use std::collections::{BTreeMap};
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use anyhow::{Context, Result};
use log::{debug, info};
use std::{env, io};
use camino::{Utf8Path, Utf8PathBuf};
use camino_tempfile::tempfile;
use camino_tempfile::NamedUtf8TempFile;
use clap::{Args, Command, CommandFactory, Parser, Subcommand, ValueHint};
use clap_complete::{generate, Generator, Shell};
use stdext::function_name;
use rsenv::{dlog, build_env_vars, print_files, get_files, link, link_all};
use rsenv::edit::{create_vimscript, open_files_in_editor, select_file_with_suffix};
use rsenv::envrc::update_dot_envrc;
use rsenv::tree::build_trees;

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
    /// Build the resulting set of environment variables
    Build {
        /// path to environment file (last child in hierarchy)
        #[arg(value_hint = ValueHint::FilePath)]
        source_path: String,
    },
    /// Write the resulting set of environment variables to .envrc (requires direnv)
    Envrc {
        /// path to environment file (last child in hierarchy)
        #[arg(value_hint = ValueHint::FilePath)]
        source_path: String,
        /// path to .envrc file
        #[arg(value_hint = ValueHint::FilePath)]
        envrc_path: Option<String>,
    },
    /// Show all files involved in building the variable set
    Files {
        /// path to environment file (last child in hierarchy)
        #[arg(value_hint = ValueHint::FilePath)]
        source_path: String,
    },
    /// Edit the FZF selected file and its linked parents (dependency chain)
    Edit {
        /// path to environment files directory
        #[arg(value_hint = ValueHint::DirPath)]
        source_dir: String,
    },
    /// FZF based selection of environment and update of .envrc file (requires direnv)
    Select {
        /// path to environment file (last child in hierarchy)
        #[arg(value_hint = ValueHint::DirPath)]
        source_dir: String,
    },
    /// Link files into a dependency tree
    Link {
        /// .env files to link (root -> parent -> child)
        #[arg(value_hint = ValueHint::FilePath, num_args = 1..)]
        nodes: Vec<String>,
    },
    /// Show all dependency trees
    Tree {
        /// path to root directory for dependency trees
        #[arg(value_hint = ValueHint::DirPath)]
        source_dir: String,
    },
    /// Edit all dependency trees side-by-side (vim required)
    TreeEdit {
        /// path to root directory for dependency trees
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
    let vars = build_env_vars(source_path).unwrap();
    println!("{}", vars);
}

fn _envrc(source_path: &str, envrc_path: Option<&str>) {
    let envrc_path = envrc_path.unwrap_or(".envrc");
    // dlog!("source_path: {:?}, envrc_path: {:?}", source_path, envrc_path);

    let vars = build_env_vars(source_path).unwrap();
    update_dot_envrc(Utf8Path::new(envrc_path), vars.as_str()).unwrap();
}

fn _files(source_path: &str) {
    dlog!("source_path: {:?}", source_path);
    print_files(source_path).unwrap();
}

fn _edit(source_dir: &str) {
    if !Utf8Path::new(source_dir).exists() {
        eprintln!("Error: Directory does not exist: {:?}", source_dir);
        return;
    }
    let selected_file = select_file_with_suffix(source_dir, ".env").unwrap();
    println!("Selected: {}", &selected_file);
    let files = get_files(selected_file.as_str()).unwrap();
    open_files_in_editor(files).unwrap();
}

fn _select(source_dir: &str) {
    if !Utf8Path::new(source_dir).exists() {
        eprintln!("Error: Directory does not exist: {:?}", source_dir);
        return;
    }
    let selected_file = select_file_with_suffix(source_dir, ".env").unwrap();
    println!("Selected: {}", &selected_file);
    _envrc(selected_file.as_str(), None);
}

fn _link(nodes: &[String]) {
    link_all(nodes);
    println!("Linked: {:?}", nodes.join(" <- "));
}

fn _tree(source_path: &str) {
    dlog!("source_path: {:?}", source_path);
    let trees = build_trees(Utf8Path::new(source_path)).unwrap();
    for tree in &trees {
        let p = &tree.borrow().node_data.file_path;
        let mut path = vec![p.to_string()];
        println!("Leaf paths of tree rooted at {}:", tree.borrow().node_data.file_path);
        tree.borrow().print_leaf_paths(&mut path);
    }
}


fn _tree_edit(source_path: &str) {
    // vim -O3 test.env int.env prod.env -c "wincmd h" -c "sp test.env" -c "wincmd l" -c "sp int.env" -c "wincmd l" -c "sp prod.env"
    dlog!("source_path: {:?}", source_path);
    let mut vimscript_files: Vec<Vec<_>> = vec![];
    let trees = build_trees(Utf8Path::new(source_path)).unwrap();

    for tree in &trees {
        let leaf_nodes = tree.borrow().leaf_nodes();
        let mut branch = Vec::new();

        for leaf in &leaf_nodes {
            println!("Leaf: {}", leaf);
            let files = get_files(leaf).unwrap();
            for file in &files {
                println!("{}", file);
                branch.push(file.to_string());
            }
            vimscript_files.push(branch.clone());
        }
    }
    dlog!("vimscript_files: {:#?}", vimscript_files);
    let vimscript = create_vimscript(vimscript_files.iter().map(|v| v.iter().map(|s| s.as_str()).collect()).collect());
    // Create a temporary file.
    let mut tmpfile = NamedUtf8TempFile::new().unwrap();
    tmpfile.write_all(vimscript.as_bytes()).unwrap();
    let status = std::process::Command::new("vim")
        .arg("-S")
        .arg(tmpfile.path())
        .status()
        .expect("failed to run vim");

    println!("Vim exited with status: {:?}", status);
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
