#![allow(unused_imports)]

use anyhow::{Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use camino_tempfile::tempfile;
use camino_tempfile::NamedUtf8TempFile;
use clap::{Args, Command, CommandFactory, Parser, Subcommand, ValueHint};
use clap_complete::{generate, Generator, Shell};
use colored::Colorize;
use rsenv::cli::args::{Cli, Commands};
use rsenv::cli::commands::execute_command;
use rsenv::edit::{
    create_branches, create_vimscript, open_files_in_editor, select_file_with_suffix,
};
use rsenv::envrc::update_dot_envrc;
use rsenv::tree::transform_tree_recursive;
use rsenv::tree::{build_trees, TreeNode};
use rsenv::{build_env_vars, get_files, is_dag, link, link_all, print_files};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::rc::Rc;
use std::{env, io, process};
use tracing::level_filters::LevelFilter;
use tracing_subscriber::filter::filter_fn;
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{fmt, Layer};

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
        if let Some(a) = Cli::command().get_author() {
            println!("AUTHOR: {}", a)
        }
        if let Some(v) = Cli::command().get_version() {
            println!("VERSION: {}", v)
        }
    }

    setup_logging(cli.debug);

    if let Err(e) = execute_command(&cli) {
        eprintln!("{}", format!("Error: {}", e).red());
        std::process::exit(1);
    }
}

fn setup_logging(verbosity: u8) {
    tracing::debug!("INIT: Attempting logger init from main.rs");

    let filter = match verbosity {
        0 => LevelFilter::WARN,
        1 => LevelFilter::INFO,
        2 => LevelFilter::DEBUG,
        3 => LevelFilter::TRACE,
        _ => {
            eprintln!("Don't be crazy, max is -d -d -d");
            LevelFilter::TRACE
        }
    };

    // Create a noisy module filter
    let noisy_modules = [""];
    let module_filter = filter_fn(move |metadata| {
        !noisy_modules
            .iter()
            .any(|name| metadata.target().starts_with(name))
    });

    // Create a subscriber with formatted output directed to stderr
    let fmt_layer = fmt::layer()
        .with_writer(std::io::stderr) // Set writer first
        .with_target(true)
        .with_thread_names(false)
        .with_span_events(FmtSpan::ENTER)
        .with_span_events(FmtSpan::CLOSE);

    // Apply filters to the layer
    let filtered_layer = fmt_layer.with_filter(filter).with_filter(module_filter);

    tracing_subscriber::registry().with(filtered_layer).init();

    // Log initial debug level
    match filter {
        LevelFilter::INFO => tracing::info!("Debug mode: info"),
        LevelFilter::DEBUG => tracing::debug!("Debug mode: debug"),
        LevelFilter::TRACE => tracing::debug!("Debug mode: trace"),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsenv::util::testing;
    use tracing::info;

    #[ctor::ctor]
    fn init() {
        testing::init_test_setup();
    }

    // https://docs.rs/clap/latest/clap/_derive/_tutorial/index.html#testing
    #[test]
    fn verify_cli() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
        info!("Debug mode: info");
    }
}
