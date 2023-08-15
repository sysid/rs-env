#![allow(unused_imports)]

use std::collections::{BTreeMap};
use std::fs::File;
use std::io::{BufRead, BufReader};
use anyhow::{Context, Result};
use log::{debug, info};
use std::{env, io};
use camino::{Utf8Path, Utf8PathBuf};
use clap::{Args, Command, CommandFactory, Parser, Subcommand, ValueHint};
use clap_complete::{generate, Generator, Shell};
use stdext::function_name;
use rsenv::{dlog, build_env_vars, print_files, get_files};
use rsenv::edit::{open_files_in_editor, select_file_with_suffix};
use rsenv::envrc::update_dot_envrc;

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
    Build {
        /// path to environment file (last child in hierarchy)
        #[arg(value_hint = ValueHint::FilePath)]
        source_path: String,
    },
    Envrc {
        /// path to environment file (last child in hierarchy)
        #[arg(value_hint = ValueHint::FilePath)]
        source_path: String,
        /// path to .envrc file
        #[arg(value_hint = ValueHint::FilePath)]
        envrc_path: Option<String>,
    },
    Files {
        /// path to environment file (last child in hierarchy)
        #[arg(value_hint = ValueHint::FilePath)]
        source_path: String,
    },
    Edit {
        /// path to environment file (last child in hierarchy)
        #[arg(value_hint = ValueHint::DirPath)]
        source_dir: String,
    },
    Select {
        /// path to environment file (last child in hierarchy)
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
    if ! Utf8Path::new(source_dir).exists() {
        eprintln!("Error: Directory does not exist: {:?}", source_dir);
        return;
    }
    let selected_file = select_file_with_suffix(source_dir, ".env").unwrap();
    println!("Selected: {}", &selected_file);
    let files = get_files(selected_file.as_str()).unwrap();
    open_files_in_editor(files).unwrap();
}

fn _select(source_dir: &str) {
    if ! Utf8Path::new(source_dir).exists() {
        eprintln!("Error: Directory does not exist: {:?}", source_dir);
        return;
    }
    let selected_file = select_file_with_suffix(source_dir, ".env").unwrap();
    println!("Selected: {}", &selected_file);
    _envrc(selected_file.as_str(), None);
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
