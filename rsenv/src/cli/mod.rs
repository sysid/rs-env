//! CLI layer: argument parsing and command dispatch

pub mod args;
pub mod error;
pub mod output;

pub use args::{Cli, Commands};
pub use error::{CliError, CliResult};
