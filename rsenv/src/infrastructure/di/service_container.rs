//! Service container for dependency injection
//!
//! Wires up all services with their dependencies.

use std::sync::Arc;

use crate::config::Settings;
use crate::infrastructure::traits::{CommandRunner, FileSystem, RealCommandRunner, RealFileSystem};

/// Container holding all application services.
///
/// Services are created lazily and cached.
/// In Phase 0, this is a skeleton that will be populated as services are implemented.
pub struct ServiceContainer {
    /// Application settings
    pub settings: Arc<Settings>,

    /// Filesystem abstraction
    pub fs: Arc<dyn FileSystem>,

    /// Command runner abstraction
    pub cmd: Arc<dyn CommandRunner>,
    // Services will be added in later phases:
    // pub env_service: EnvironmentService,      // Phase 1
    // pub vault_service: Arc<VaultService>,     // Phase 2
    // pub sops_service: SopsService,            // Phase 3
    // pub swap_service: SwapService,            // Phase 4
}

impl ServiceContainer {
    /// Create a new service container with real implementations.
    pub fn new(settings: Settings) -> Self {
        Self::with_deps(
            settings,
            Arc::new(RealFileSystem),
            Arc::new(RealCommandRunner),
        )
    }

    /// Create a service container with custom dependencies (for testing).
    pub fn with_deps(
        settings: Settings,
        fs: Arc<dyn FileSystem>,
        cmd: Arc<dyn CommandRunner>,
    ) -> Self {
        let settings = Arc::new(settings);

        Self { settings, fs, cmd }
    }
}
