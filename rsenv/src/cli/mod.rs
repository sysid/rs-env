//! CLI layer: argument parsing and command dispatch

pub mod args;
pub mod diff_render;
pub mod editor;
pub mod error;
pub mod output;
pub mod pager;

pub use args::{Cli, Commands};
pub use error::{CliError, CliResult};
