//! Application services
//!
//! Concrete service implementations that orchestrate domain logic.
//! Services depend on I/O boundary traits (FileSystem, CommandRunner, etc.)
//! but are themselves concrete structs, not traits.

mod environment;
mod sops;
mod swap;
mod vault;

pub use environment::{EnvHierarchy, EnvOutput, EnvironmentService};
pub use sops::SopsService;
pub use swap::SwapService;
pub use vault::VaultService;
