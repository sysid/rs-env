//! rsenv: Unified development environment manager
//!
//! This library provides the core functionality for rsenv, consolidating:
//! - Hierarchical environment variable management (rsenv v1)
//! - File guarding with symlinks (confguard)
//! - File swap-in/out (rplc)
//!
//! # Architecture
//!
//! The crate follows clean architecture with layered error types:
//! - `domain`: Core entities and business rules
//! - `application`: Services orchestrating domain logic
//! - `infrastructure`: I/O implementations and DI container
//! - `cli`: Command-line interface
//!
//! # Example
//!
//! ```no_run
//! use rsenv::config::Settings;
//! use rsenv::infrastructure::di::ServiceContainer;
//!
//! let settings = Settings::load(None).unwrap();
//! let container = ServiceContainer::new(settings);
//! ```

pub mod application;
pub mod cli;
pub mod config;
pub mod domain;
pub mod exitcode;
pub mod infrastructure;

// Re-export commonly used types
pub use config::Settings;
pub use domain::{EnvFile, GuardedFile, Project, SwapFile, SwapState, Vault};
pub use infrastructure::di::ServiceContainer;
