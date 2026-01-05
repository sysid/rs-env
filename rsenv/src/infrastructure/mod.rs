//! Infrastructure layer: I/O implementations and DI container
//!
//! This layer implements I/O boundary traits and wires up services.

pub mod di;
pub mod error;
pub mod traits;

pub use error::{InfraError, InfraResult};
