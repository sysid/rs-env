//! Application layer: services and use cases
//!
//! This layer orchestrates domain logic and depends on I/O boundary traits.

pub mod dotfile;
pub mod envrc;
pub mod error;
pub mod error_ext;
pub mod services;

pub use envrc::{
    add_swapped_marker, delete_section, remove_swapped_marker, update_dot_envrc,
    END_SECTION_DELIMITER, RSENV_SWAPPED_MARKER, START_SECTION_DELIMITER,
};
pub use error::{ApplicationError, ApplicationResult};
pub use error_ext::IoResultExt;
