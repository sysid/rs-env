//! Domain layer: entities and business logic
//!
//! This layer is independent of external concerns (no I/O, no CLI, no config loading).

pub mod arena;
pub mod builder;
pub mod entities;
pub mod error;

pub use arena::{create_branches, NodeData, TreeArena, TreeNode};
pub use builder::TreeBuilder;
pub use entities::*;
pub use error::DomainError;
