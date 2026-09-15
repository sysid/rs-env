//! Application layer: services and use cases
//!
//! This layer orchestrates domain logic and depends on I/O boundary traits.

pub mod dotfile;
pub mod envrc;
pub mod error;
pub mod error_ext;
pub mod hash;
pub mod services;

pub use envrc::{
    delete_section, parse_rsenv_metadata, update_dot_envrc, END_SECTION_DELIMITER,
    START_SECTION_DELIMITER,
};
pub use error::{ApplicationError, ApplicationResult};
pub use error_ext::IoResultExt;
