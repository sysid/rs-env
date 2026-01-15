//! rsenv: Unified development environment manager
//!
//! Consolidates hierarchical environment variables (rsenv), file guarding (confguard),
//! and file swapping (rplc) into a single tool.

use std::io;
use std::process::ExitCode;
use std::sync::Arc;

use clap::{CommandFactory, Parser};
use tracing_subscriber::EnvFilter;

use colored::Colorize;
use rsenv::application::envrc::update_vars_section;
use rsenv::application::services::{
    EnvironmentService, GitignoreService, SopsService, SwapService, VaultService,
};
use rsenv::cli::args::{
    Cli, Commands, ConfigCommands, EnvCommands, GuardCommands, HookCommands, InitCommands,
    SopsCommands, SwapCommands,
};
use rsenv::cli::output;
use rsenv::config::{global_config_dir, global_config_path, vault_config_path, Settings};
use rsenv::domain::{TreeBuilder, TreeNodeConvert};
use rsenv::exitcode;
use rsenv::infrastructure::traits::RealCommandRunner;
use rsenv::infrastructure::traits::{
    Editor, EnvironmentEditor, RealFileSystem, SelectionItem, Selector, SkimSelector,
};

fn main() -> ExitCode {
    let cli = Cli::parse();

    // Initialize logging - use DEBUG level if verbose flag is set
    let filter = if cli.verbose {
        EnvFilter::new("rsenv=debug")
    } else {
        EnvFilter::from_default_env()
    };
    tracing_subscriber::fmt().with_env_filter(filter).init();

    match run(cli) {
        Ok(()) => ExitCode::from(exitcode::OK as u8),
        Err(e) => {
            output::error(&e);
            ExitCode::from(e.exit_code() as u8)
        }
    }
}

fn run(cli: Cli) -> rsenv::cli::CliResult<()> {
    // Determine project directory
    // Clone for swap commands which need to distinguish "not provided" from "provided"
    let cli_project_dir = cli.project_dir.clone();
    let project_dir = cli.project_dir.or_else(|| std::env::current_dir().ok());

    // Two-phase config loading:
    // 1. Discover vault from project's .envrc symlink (no Settings needed)
    // 2. Load settings with vault path for local config
    let fs = Arc::new(RealFileSystem);
    let vault_path = project_dir
        .as_deref()
        .and_then(|p| VaultService::discover_vault_path(fs.as_ref(), p).ok())
        .flatten();
    // Use vault if found, else project dir for local config lookup
    let config_dir = vault_path.clone().or(project_dir.clone());
    let settings = Settings::load(config_dir.as_deref()).map_err(|e| {
        rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::Application(e))
    })?;

    match cli.command {
        Some(Commands::Init { command }) => handle_init(command, project_dir, &settings),
        Some(Commands::Config { command }) => handle_config(command, &settings, project_dir),
        Some(Commands::Env { command }) => handle_env(command, project_dir),
        Some(Commands::Guard { command }) => handle_guard(command, project_dir, &settings),
        Some(Commands::Hook { command }) => handle_hook(command, vault_path, &settings),
        Some(Commands::Info) => handle_info(project_dir, &settings),
        Some(Commands::Sops { command }) => handle_sops(command, vault_path, &settings),
        Some(Commands::Swap { command }) => {
            // Pass cli_project_dir directly so vault-wide commands can distinguish
            // between "not provided" (use settings.vault_base_dir) vs "provided" (override)
            handle_swap(command, cli_project_dir, &settings)
        }
        Some(Commands::Completion { shell }) => {
            generate_completions(shell);
            Ok(())
        }
        None => {
            // No command: show help
            Cli::command().print_help().ok();
            println!();
            Ok(())
        }
    }
}

fn handle_env(
    command: EnvCommands,
    project_dir: Option<std::path::PathBuf>,
) -> rsenv::cli::CliResult<()> {
    let fs = Arc::new(RealFileSystem);
    let service = EnvironmentService::new(fs);

    match command {
        EnvCommands::Build { file } => {
            let result = service.build(&file).map_err(|e| {
                rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::Application(e))
            })?;

            // Output as shell-sourceable format
            for (key, value) in &result.variables {
                println!("export {}={:?}", key, value);
            }
            Ok(())
        }
        EnvCommands::Envrc { file, envrc } => {
            let fs: Arc<dyn rsenv::infrastructure::traits::FileSystem> = Arc::new(RealFileSystem);
            let envrc_path = envrc.unwrap_or_else(|| std::path::PathBuf::from(".envrc"));
            let output = service.build(&file).map_err(|e| {
                rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::Application(e))
            })?;

            let mut exports = String::new();
            for (k, v) in &output.variables {
                exports.push_str(&format!("export {}={}\n", k, v));
            }

            update_vars_section(&fs, &envrc_path, &exports).map_err(|e| {
                rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::Application(e))
            })?;
            output::action("Updated", &envrc_path.display());
            Ok(())
        }
        EnvCommands::Files { file } => {
            let files = service.get_files(&file).map_err(|e| {
                rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::Application(e))
            })?;
            for f in files {
                println!("{}", f.display());
            }
            Ok(())
        }
        EnvCommands::Tree { dir } => {
            let search_dir = dir
                .or(project_dir)
                .unwrap_or_else(|| std::env::current_dir().unwrap());

            if service.is_dag(&search_dir).map_err(|e| {
                rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::Application(e))
            })? {
                return Err(rsenv::cli::CliError::Usage(
                    "Dependencies form a DAG, cannot use tree-based commands.".to_string(),
                ));
            }

            let mut builder = TreeBuilder::new();
            let trees = builder.build_from_directory(&search_dir).map_err(|e| {
                rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::Application(
                    rsenv::application::ApplicationError::Domain(e),
                ))
            })?;

            output::header(&format!("Found {} trees:", trees.len()));
            println!();
            for tree in &trees {
                println!("{}", tree.to_tree_string());
            }
            Ok(())
        }
        EnvCommands::Link { files } => {
            if files.len() < 2 {
                return Err(rsenv::cli::CliError::Usage(
                    "Link requires at least 2 files".to_string(),
                ));
            }
            service.link_chain(&files).map_err(|e| {
                rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::Application(e))
            })?;
            let chain: Vec<_> = files.iter().map(|p| p.display().to_string()).collect();
            output::action("Linked", &chain.join(" <- "));
            Ok(())
        }
        EnvCommands::Unlink { file } => {
            service.unlink(&file).map_err(|e| {
                rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::Application(e))
            })?;
            output::action("Unlinked", &file.display());
            Ok(())
        }
        EnvCommands::Select { dir } => {
            let search_dir = dir
                .or(project_dir)
                .unwrap_or_else(|| std::env::current_dir().unwrap());

            let hierarchy = service.get_hierarchy(&search_dir).map_err(|e| {
                rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::Application(e))
            })?;

            if hierarchy.files.is_empty() {
                output::info(&format!("No .env files found in {}", search_dir.display()));
                return Ok(());
            }

            // Build selection items
            let items: Vec<SelectionItem> = hierarchy
                .files
                .iter()
                .map(|f| SelectionItem {
                    display: f
                        .path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| f.path.to_string_lossy().to_string()),
                    value: f.path.to_string_lossy().to_string(),
                })
                .collect();

            // Present selector
            let selector = SkimSelector;
            let selected = selector
                .select_one(&items, "Select environment: ")
                .map_err(|e| rsenv::cli::CliError::Usage(format!("selection failed: {e}")))?;

            match selected {
                Some(item) => {
                    // Build the selected environment
                    let result = service
                        .build(&std::path::PathBuf::from(&item.value))
                        .map_err(|e| {
                            rsenv::cli::CliError::Infra(
                                rsenv::infrastructure::InfraError::Application(e),
                            )
                        })?;

                    // Output as shell-sourceable format
                    for (key, value) in &result.variables {
                        println!("export {}={:?}", key, value);
                    }
                }
                None => {
                    // User cancelled
                    output::info(&"Selection cancelled");
                }
            }
            Ok(())
        }
        EnvCommands::Edit { dir } => {
            let search_dir = dir
                .or(project_dir)
                .unwrap_or_else(|| std::env::current_dir().unwrap());

            let hierarchy = service.get_hierarchy(&search_dir).map_err(|e| {
                rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::Application(e))
            })?;

            if hierarchy.files.is_empty() {
                output::info(&format!("No .env files found in {}", search_dir.display()));
                return Ok(());
            }

            // Build selection items
            let items: Vec<SelectionItem> = hierarchy
                .files
                .iter()
                .map(|f| SelectionItem {
                    display: f
                        .path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| f.path.to_string_lossy().to_string()),
                    value: f.path.to_string_lossy().to_string(),
                })
                .collect();

            // Present selector
            let selector = SkimSelector;
            let selected = selector
                .select_one(&items, "Edit environment: ")
                .map_err(|e| rsenv::cli::CliError::Usage(format!("selection failed: {e}")))?;

            match selected {
                Some(item) => {
                    // Open in editor
                    let editor = EnvironmentEditor;
                    let path = std::path::PathBuf::from(&item.value);
                    editor.open(&path).map_err(|e| {
                        rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::io(
                            format!("open editor for {}", path.display()),
                            e,
                        ))
                    })?;
                    output::action("Edited", &item.display);
                }
                None => {
                    output::info(&"Selection cancelled");
                }
            }
            Ok(())
        }
        EnvCommands::Branches { dir } => {
            let dir = dir
                .or(project_dir)
                .unwrap_or_else(|| std::env::current_dir().unwrap());

            if service.is_dag(&dir).map_err(|e| {
                rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::Application(e))
            })? {
                return Err(rsenv::cli::CliError::Usage(
                    "Dependencies form a DAG, cannot use tree-based commands.".to_string(),
                ));
            }

            let mut builder = TreeBuilder::new();
            let trees = builder.build_from_directory(&dir).map_err(|e| {
                rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::Application(
                    rsenv::application::ApplicationError::Domain(e),
                ))
            })?;

            output::header(&format!("Found {} trees:", trees.len()));
            for tree in &trees {
                if let Some(root_idx) = tree.root() {
                    if let Some(root_node) = tree.get_node(root_idx) {
                        output::detail(&format!(
                            "Tree Root: {}",
                            root_node.data.file_path.display()
                        ));
                    }
                }
            }
            Ok(())
        }
        EnvCommands::EditLeaf { file } => {
            let files = service.get_files(&file).map_err(|e| {
                rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::Application(e))
            })?;

            if files.is_empty() {
                output::info(&"No files in hierarchy");
                return Ok(());
            }

            // Open all files in editor with -O (vertical split)
            let editor_cmd = std::env::var("EDITOR").unwrap_or_else(|_| "vim".to_string());
            let file_args: Vec<String> = files
                .iter()
                .map(|p| p.to_string_lossy().to_string())
                .collect();

            std::process::Command::new(&editor_cmd)
                .arg("-O")
                .args(&file_args)
                .status()
                .map_err(|e| {
                    rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::io(
                        format!("open editor {}", editor_cmd),
                        e,
                    ))
                })?;

            output::action("Edited", &format!("{} files", files.len()));
            Ok(())
        }
        EnvCommands::TreeEdit { dir } => {
            let dir = dir
                .or(project_dir)
                .unwrap_or_else(|| std::env::current_dir().unwrap());

            if service.is_dag(&dir).map_err(|e| {
                rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::Application(e))
            })? {
                return Err(rsenv::cli::CliError::Usage(
                    "Dependencies form a DAG, cannot use tree-based commands.".to_string(),
                ));
            }

            let mut builder = TreeBuilder::new();
            let trees = builder.build_from_directory(&dir).map_err(|e| {
                rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::Application(
                    rsenv::application::ApplicationError::Domain(e),
                ))
            })?;

            // Collect branches: for each leaf, get the parent chain (leaf → root)
            let mut branches: Vec<Vec<std::path::PathBuf>> = Vec::new();
            for tree in &trees {
                for leaf in tree.leaf_nodes() {
                    let leaf_path = std::path::Path::new(&leaf);
                    if let Ok(files) = service.get_files(leaf_path) {
                        if !files.is_empty() {
                            branches.push(files);
                        }
                    }
                }
            }

            if branches.is_empty() {
                output::info(&"No environment files found");
                return Ok(());
            }

            output::header(&format!("Editing {} branches...", branches.len()));

            // Generate vimscript for grid layout
            let vimscript = create_vimscript(&branches);

            // Write vimscript to temp file
            use std::io::Write;
            let mut tmpfile = tempfile::NamedTempFile::new().map_err(|e| {
                rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::io(
                    "create temp file".to_string(),
                    e,
                ))
            })?;
            tmpfile.write_all(vimscript.as_bytes()).map_err(|e| {
                rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::io(
                    "write vimscript".to_string(),
                    e,
                ))
            })?;

            // Run vim with -S to source the script
            let status = std::process::Command::new("vim")
                .arg("-S")
                .arg(tmpfile.path())
                .status()
                .map_err(|e| {
                    rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::io(
                        "run vim".to_string(),
                        e,
                    ))
                })?;

            output::info(&format!("Vim: {}", status));
            Ok(())
        }
        EnvCommands::Leaves { dir } => {
            let dir = dir
                .or(project_dir)
                .unwrap_or_else(|| std::env::current_dir().unwrap());

            if service.is_dag(&dir).map_err(|e| {
                rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::Application(e))
            })? {
                return Err(rsenv::cli::CliError::Usage(
                    "Dependencies form a DAG, cannot use tree-based commands.".to_string(),
                ));
            }

            let mut builder = TreeBuilder::new();
            let trees = builder.build_from_directory(&dir).map_err(|e| {
                rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::Application(
                    rsenv::application::ApplicationError::Domain(e),
                ))
            })?;

            for tree in &trees {
                for leaf in tree.leaf_nodes() {
                    println!("{}", leaf);
                }
            }
            Ok(())
        }
    }
}

fn handle_config(
    command: ConfigCommands,
    settings: &Settings,
    project_dir: Option<std::path::PathBuf>,
) -> rsenv::cli::CliResult<()> {
    match command {
        ConfigCommands::Show => {
            let toml = settings.to_toml().map_err(|e| {
                rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::Application(e))
            })?;
            println!("{toml}");
            Ok(())
        }
        ConfigCommands::Init { global } => {
            let template = Settings::template();

            if global {
                // Create global config
                if let Some(dir) = global_config_dir() {
                    std::fs::create_dir_all(&dir).map_err(|e| {
                        rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::io(
                            format!("create config dir: {}", dir.display()),
                            e,
                        ))
                    })?;

                    let path = dir.join("rsenv.toml");
                    if path.exists() {
                        return Err(rsenv::cli::CliError::Usage(format!(
                            "config already exists: {}",
                            path.display()
                        )));
                    }

                    std::fs::write(&path, &template).map_err(|e| {
                        rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::io(
                            format!("write config: {}", path.display()),
                            e,
                        ))
                    })?;

                    output::action("Created", &path.display());
                } else {
                    return Err(rsenv::cli::CliError::Usage(
                        "cannot determine config directory".into(),
                    ));
                }
            } else {
                // Smart fallback: try vault first, then project directory
                let current_dir = project_dir.unwrap_or_else(|| std::env::current_dir().unwrap());

                let fs = Arc::new(RealFileSystem);
                let vault_path = VaultService::discover_vault_path(fs.as_ref(), &current_dir)
                    .map_err(|e| {
                        rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::Application(
                            e,
                        ))
                    })?;

                // Use vault if initialized, otherwise use project directory directly
                let target_dir = vault_path.unwrap_or_else(|| current_dir.clone());
                let path = vault_config_path(&target_dir);

                if path.exists() {
                    return Err(rsenv::cli::CliError::Usage(format!(
                        "config already exists: {}",
                        path.display()
                    )));
                }

                std::fs::write(&path, &template).map_err(|e| {
                    rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::io(
                        format!("write config: {}", path.display()),
                        e,
                    ))
                })?;

                output::action("Created", &path.display());
            }
            Ok(())
        }
        ConfigCommands::Path => {
            output::info(&format!("Global config: {:?}", global_config_path()));

            let current_dir = project_dir.unwrap_or_else(|| std::env::current_dir().unwrap());
            let fs = Arc::new(RealFileSystem);

            match VaultService::discover_vault_path(fs.as_ref(), &current_dir) {
                Ok(Some(vault_dir)) => {
                    output::info(&format!(
                        "Local config:  {}",
                        vault_config_path(&vault_dir).display()
                    ));
                }
                _ => {
                    output::info(&"Local config:  (project not initialized)");
                }
            }
            Ok(())
        }
        ConfigCommands::Edit { global } => {
            let fs = Arc::new(RealFileSystem);
            let template = Settings::template();

            // 1. Determine config path
            let (config_path, vault_dir) = if global {
                let path = global_config_path().ok_or_else(|| {
                    rsenv::cli::CliError::Usage("cannot determine global config directory".into())
                })?;
                (path, None)
            } else {
                // Vault-local config requires vault context
                let current_dir = project_dir
                    .clone()
                    .unwrap_or_else(|| std::env::current_dir().unwrap());
                let vault_path = VaultService::discover_vault_path(fs.as_ref(), &current_dir)
                    .map_err(|e| {
                        rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::Application(
                            e,
                        ))
                    })?
                    .ok_or_else(|| {
                        rsenv::cli::CliError::Usage(
                            "not in a vault-initialized project (use --global for global config)"
                                .into(),
                        )
                    })?;
                let path = vault_config_path(&vault_path);
                (path, Some(vault_path))
            };

            // 2. Track if config existed before
            let existed_before = config_path.exists();

            // 3. Create with template if doesn't exist
            if !existed_before {
                if let Some(parent) = config_path.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| {
                        rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::io(
                            format!("create config dir: {}", parent.display()),
                            e,
                        ))
                    })?;
                }
                std::fs::write(&config_path, &template).map_err(|e| {
                    rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::io(
                        format!("write config template: {}", config_path.display()),
                        e,
                    ))
                })?;
            }

            // 4. Open in editor
            let editor = EnvironmentEditor;
            editor.open(&config_path).map_err(|e| {
                rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::io(
                    format!("open editor for {}", config_path.display()),
                    e,
                ))
            })?;

            // 5. Check if user actually made changes
            let config_changed = if existed_before {
                true // Assume changes if file existed (can't easily detect)
            } else {
                // Config didn't exist before - check if it was saved with changes
                if config_path.exists() {
                    let content = std::fs::read_to_string(&config_path).unwrap_or_default();
                    content.trim() != template.trim()
                } else {
                    false // File deleted or never saved
                }
            };

            // 6. Clean up if no changes (only for newly created files)
            if !existed_before && !config_changed {
                let _ = std::fs::remove_file(&config_path);
                output::info(&"No changes made");
                return Ok(());
            }

            // 7. Sync gitignore after edit (match edited scope)
            let global_settings = Settings::load_global_only().map_err(|e| {
                rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::Application(e))
            })?;
            let gitignore_service = GitignoreService::new(fs, global_settings);
            let sync_result: Result<(), _> = if global {
                gitignore_service.sync_global().map(|_| ())
            } else {
                gitignore_service
                    .sync_vault(vault_dir.as_ref().unwrap())
                    .map(|_| ())
            };
            if let Err(e) = sync_result {
                output::warning(&format!("gitignore sync failed: {}", e));
            }

            output::action("Edited", &config_path.display());
            Ok(())
        }
    }
}

fn handle_init(
    command: InitCommands,
    project_dir: Option<std::path::PathBuf>,
    settings: &Settings,
) -> rsenv::cli::CliResult<()> {
    match command {
        InitCommands::Vault { project, absolute } => {
            let project_dir = project
                .or(project_dir)
                .unwrap_or_else(|| std::env::current_dir().unwrap());
            handle_init_create(project_dir, absolute, settings)
        }
        InitCommands::Reset { project } => {
            let project_dir = project
                .or(project_dir)
                .unwrap_or_else(|| std::env::current_dir().unwrap());
            handle_init_reset(project_dir, settings)
        }
        InitCommands::Reconnect { envrc_path } => {
            let project_dir = project_dir.unwrap_or_else(|| std::env::current_dir().unwrap());
            handle_init_reconnect(envrc_path, project_dir, settings)
        }
    }
}

fn handle_init_create(
    project_dir: std::path::PathBuf,
    absolute: bool,
    settings: &Settings,
) -> rsenv::cli::CliResult<()> {
    let fs = Arc::new(RealFileSystem);
    let settings = Arc::new(settings.clone());
    let service = VaultService::new(fs.clone(), settings.clone());

    let vault = service.init(&project_dir, absolute).map_err(|e| {
        rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::Application(e))
    })?;

    // Auto-sync gitignore for new vault (silent on success)
    // GitignoreService needs global-only settings to know what goes in global .gitignore
    let global_settings = Settings::load_global_only().map_err(|e| {
        rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::Application(e))
    })?;
    let gitignore_service = GitignoreService::new(fs, global_settings);
    let sync_result = gitignore_service.sync_all(Some(&vault.path));
    if let Err(e) = sync_result {
        output::warning(&format!("Failed to sync .gitignore: {}", e));
    }

    output::action("Initialized vault for", &project_dir.display());
    output::detail(&format!("Vault:       {}", vault.path.display()));
    output::detail(&format!("Sentinel ID: {}", vault.sentinel_id));
    Ok(())
}

fn handle_init_reset(
    project_dir: std::path::PathBuf,
    settings: &Settings,
) -> rsenv::cli::CliResult<()> {
    let fs = Arc::new(RealFileSystem);
    let settings_arc = Arc::new(settings.clone());
    let service = VaultService::new(fs.clone(), settings_arc);

    // Get vault info before reset for display
    let vault = service.get(&project_dir).map_err(|e| {
        rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::Application(e))
    })?;

    let vault_path = vault.as_ref().map(|v| v.path.clone());

    // Confirm before reset
    output::header("This will:");
    output::detail(&"- Restore all guarded files to project");
    output::detail(&"- Remove .envrc symlink");
    if let Some(ref path) = vault_path {
        output::detail(&format!("- Leave vault directory at {}", path.display()));
    }
    output::prompt(&"Continue? [y/N]");
    std::io::Write::flush(&mut std::io::stdout()).ok();

    let mut input = String::new();
    std::io::stdin().read_line(&mut input).map_err(|e| {
        rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::Io {
            context: "read confirmation".to_string(),
            source: e,
        })
    })?;

    if !input.trim().eq_ignore_ascii_case("y") {
        output::info(&"Aborted.");
        return Ok(());
    }

    let restored_count = service.reset(&project_dir).map_err(|e| {
        rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::Application(e))
    })?;

    output::action("Reset vault for", &project_dir.display());
    output::detail(&format!("Restored {} guarded file(s)", restored_count));
    output::detail(&"Removed .envrc symlink");
    if let Some(path) = vault_path {
        println!();
        output::info(&format!(
            "Note: Vault directory remains at {}",
            path.display()
        ));
        output::info(&"      Delete manually if no longer needed.");
    }
    Ok(())
}

fn handle_init_reconnect(
    envrc_path: std::path::PathBuf,
    project_dir: std::path::PathBuf,
    settings: &Settings,
) -> rsenv::cli::CliResult<()> {
    let fs = Arc::new(RealFileSystem);
    let settings_arc = Arc::new(settings.clone());
    let service = VaultService::new(fs, settings_arc);

    let vault = service.reconnect(&envrc_path, &project_dir).map_err(|e| {
        rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::Application(e))
    })?;

    output::action(
        "Reconnected",
        &format!("{} to vault", project_dir.display()),
    );
    output::detail(&format!("Vault:       {}", vault.path.display()));
    output::detail(&format!("Sentinel ID: {}", vault.sentinel_id));
    Ok(())
}

fn handle_guard(
    command: GuardCommands,
    project_dir: Option<std::path::PathBuf>,
    settings: &Settings,
) -> rsenv::cli::CliResult<()> {
    let fs = Arc::new(RealFileSystem);
    let settings = Arc::new(settings.clone());
    let service = VaultService::new(fs.clone(), settings);

    match command {
        GuardCommands::Add { file, absolute } => {
            let guarded = service.guard(&file, absolute).map_err(|e| {
                rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::Application(e))
            })?;
            output::action("Guarded", &file.display());
            output::detail(&format!("Vault: {}", guarded.vault_path.display()));
            Ok(())
        }
        GuardCommands::List => {
            let project_dir = project_dir.unwrap_or_else(|| std::env::current_dir().unwrap());

            let vault = service.get(&project_dir).map_err(|e| {
                rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::Application(e))
            })?;

            match vault {
                Some(v) => {
                    let guarded_dir = v.path.join("guarded");
                    if !guarded_dir.exists() {
                        output::info(&"No guarded files");
                        return Ok(());
                    }

                    output::header(&format!("Guarded files in {}:", project_dir.display()));
                    for entry in walkdir::WalkDir::new(&guarded_dir)
                        .into_iter()
                        .filter_map(|e| e.ok())
                        .filter(|e| e.file_type().is_file())
                    {
                        if let Ok(rel) = entry.path().strip_prefix(&guarded_dir) {
                            output::detail(&rel.display());
                        }
                    }
                    Ok(())
                }
                None => {
                    output::info(&format!(
                        "Vault not initialized for {}",
                        project_dir.display()
                    ));
                    Ok(())
                }
            }
        }
        GuardCommands::Restore { file } => {
            service.unguard(&file).map_err(|e| {
                rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::Application(e))
            })?;
            output::action("Restored", &file.display());
            Ok(())
        }
    }
}

fn handle_info(
    project_dir: Option<std::path::PathBuf>,
    settings: &Settings,
) -> rsenv::cli::CliResult<()> {
    let project_dir = project_dir.unwrap_or_else(|| std::env::current_dir().unwrap());

    let fs = Arc::new(RealFileSystem);
    let settings_arc = Arc::new(settings.clone());
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings_arc.clone()));

    output::header(&format!("Project: {}", project_dir.display()));

    let vault = vault_service.get(&project_dir).map_err(|e| {
        rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::Application(e))
    })?;

    match vault {
        Some(v) => {
            output::info(&format!("Vault:   {}", v.path.display()));
            output::info(&format!("ID:      {}", v.sentinel_id));

            // Count guarded files
            let guarded_dir = v.path.join("guarded");
            let guarded_count = if guarded_dir.exists() {
                walkdir::WalkDir::new(&guarded_dir)
                    .into_iter()
                    .filter_map(|e| e.ok())
                    .filter(|e| e.file_type().is_file())
                    .count()
            } else {
                0
            };
            output::info(&format!("Guarded: {} files", guarded_count));

            // Swap status summary
            let swap_service = SwapService::new(fs.clone(), vault_service.clone(), settings_arc);
            if let Ok(swap_files) = swap_service.status(&project_dir) {
                if !swap_files.is_empty() {
                    let in_count = swap_files
                        .iter()
                        .filter(|f| matches!(f.state, rsenv::domain::SwapState::In { .. }))
                        .count();
                    let out_count = swap_files.len() - in_count;
                    output::info(&format!(
                        "Swap:    {} files ({} in, {} out)",
                        swap_files.len(),
                        in_count,
                        out_count
                    ));
                } else {
                    output::info("Swap:    0 files");
                }
            }

            // Gitignore status
            if let Ok(global_settings) = Settings::load_global_only() {
                let gitignore_service = GitignoreService::new(fs.clone(), global_settings);
                if let Ok(status) = gitignore_service.status(Some(&v.path)) {
                    println!();
                    output::info("Gitignore:");
                    let global_status = if status.global_diff.in_sync {
                        "in sync".to_string()
                    } else {
                        format!(
                            "{} to add, {} to remove",
                            status.global_diff.to_add.len(),
                            status.global_diff.to_remove.len()
                        )
                    };
                    output::detail(&format!("Global: {}", global_status));

                    if let Some(vault_status) = &status.vault {
                        let vault_sync = if vault_status.diff.in_sync {
                            "in sync".to_string()
                        } else {
                            format!(
                                "{} to add, {} to remove",
                                vault_status.diff.to_add.len(),
                                vault_status.diff.to_remove.len()
                            )
                        };
                        output::detail(&format!("Vault:  {}", vault_sync));
                    }
                }
            }

            // Configuration summary
            println!();
            output::info("Config:");
            output::detail(&format!(
                "vault_base_dir: {}",
                settings.vault_base_dir.display()
            ));
            output::detail(&format!("editor: {}", settings.editor));
            if let Some(ref gpg_key) = settings.sops.gpg_key {
                let truncated = if gpg_key.len() > 16 {
                    format!("{}...", &gpg_key[..16])
                } else {
                    gpg_key.clone()
                };
                output::detail(&format!("sops.gpg_key: {}", truncated));
            }
            if let Some(ref age_key) = settings.sops.age_key {
                let truncated = if age_key.len() > 20 {
                    format!("{}...", &age_key[..20])
                } else {
                    age_key.clone()
                };
                output::detail(&format!("sops.age_key: {}", truncated));
            }
        }
        None => {
            output::info(&"Vault:   (not initialized)");
            println!();
            output::info(&"Run 'rsenv init vault' to create a vault for this project.");
        }
    }

    Ok(())
}

fn resolve_sops_dir(
    dir: Option<std::path::PathBuf>,
    global: bool,
    vault_path: Option<&std::path::Path>,
    vault_base_dir: &std::path::Path,
) -> rsenv::cli::CliResult<std::path::PathBuf> {
    if global {
        Ok(vault_base_dir.to_path_buf())
    } else if let Some(d) = dir {
        Ok(d)
    } else if let Some(v) = vault_path {
        Ok(v.to_path_buf())
    } else {
        Err(rsenv::cli::CliError::Usage(
            "No vault found. Run 'rsenv init' first, or use --dir or --global.".into(),
        ))
    }
}

fn handle_sops(
    command: SopsCommands,
    vault_path: Option<std::path::PathBuf>,
    settings: &Settings,
) -> rsenv::cli::CliResult<()> {
    let fs = Arc::new(RealFileSystem);
    let cmd = Arc::new(RealCommandRunner);
    let settings_arc = Arc::new(settings.clone());
    let service = SopsService::new(fs, cmd, settings_arc);

    match command {
        SopsCommands::Encrypt {
            file,
            dir,
            global,
            vault_base,
        } => {
            if let Some(file_path) = file {
                // File-level: encrypt single file
                output::header(&format!("Encrypting: {}", file_path.display()));
                let result = service.encrypt_file(&file_path).map_err(|e| {
                    rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::Application(e))
                })?;
                output::success(&format!("Created: {}", result.display()));
            } else {
                // Directory/vault/global level
                let vault_base_dir = vault_base.unwrap_or_else(|| settings.vault_base_dir.clone());
                let base_dir =
                    resolve_sops_dir(dir, global, vault_path.as_deref(), &vault_base_dir)?;
                output::header(&format!("Encrypting in: {}", base_dir.display()));
                let encrypted = service.encrypt_all(Some(&base_dir)).map_err(|e| {
                    rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::Application(e))
                })?;

                if encrypted.is_empty() {
                    output::info(&"No files to encrypt");
                } else {
                    output::info(&format!("Encrypted {} files:", encrypted.len()));
                    for path in &encrypted {
                        output::detail(&path.display());
                    }
                }
            }
            Ok(())
        }
        SopsCommands::Decrypt {
            file,
            dir,
            global,
            vault_base,
        } => {
            if let Some(file_path) = file {
                // File-level: decrypt single file
                output::header(&format!("Decrypting: {}", file_path.display()));
                let result = service.decrypt_file(&file_path).map_err(|e| {
                    rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::Application(e))
                })?;
                output::success(&format!("Created: {}", result.display()));
            } else {
                // Directory/vault/global level
                let vault_base_dir = vault_base.unwrap_or_else(|| settings.vault_base_dir.clone());
                let base_dir =
                    resolve_sops_dir(dir, global, vault_path.as_deref(), &vault_base_dir)?;
                output::header(&format!("Decrypting in: {}", base_dir.display()));
                let decrypted = service.decrypt_all(Some(&base_dir)).map_err(|e| {
                    rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::Application(e))
                })?;

                if decrypted.is_empty() {
                    output::info(&"No files to decrypt");
                } else {
                    output::info(&format!("Decrypted {} files:", decrypted.len()));
                    for path in &decrypted {
                        output::detail(&path.display());
                    }
                }
            }
            Ok(())
        }
        SopsCommands::Clean {
            dir,
            global,
            vault_base,
        } => {
            let vault_base_dir = vault_base.unwrap_or_else(|| settings.vault_base_dir.clone());
            let base_dir = resolve_sops_dir(dir, global, vault_path.as_deref(), &vault_base_dir)?;
            output::header(&format!("Cleaning in: {}", base_dir.display()));
            let deleted = service.clean(Some(&base_dir)).map_err(|e| {
                rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::Application(e))
            })?;

            if deleted.is_empty() {
                output::info(&"No plaintext files to clean");
            } else {
                output::info(&format!("Deleted {} plaintext files:", deleted.len()));
                for path in &deleted {
                    output::detail(&path.display());
                }
            }
            Ok(())
        }
        SopsCommands::Status {
            dir,
            global,
            vault_base,
            check,
        } => {
            let vault_base_dir = vault_base.unwrap_or_else(|| settings.vault_base_dir.clone());
            let base_dir = resolve_sops_dir(dir, global, vault_path.as_deref(), &vault_base_dir)?;
            let status = service.status(Some(&base_dir)).map_err(|e| {
                rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::Application(e))
            })?;

            // --check mode: silent, just return exit code
            if check {
                if status.needs_encryption() {
                    std::process::exit(1);
                } else {
                    return Ok(());
                }
            }

            output::header(&format!("SOPS Status for: {}", base_dir.display()));
            println!();

            if !status.pending_encrypt.is_empty() {
                output::warning(&format!(
                    "Pending encryption ({}):",
                    status.pending_encrypt.len()
                ));
                for path in &status.pending_encrypt {
                    output::detail(&path.display());
                }
                println!();
            }

            if !status.stale.is_empty() {
                output::warning(&format!(
                    "Stale (needs re-encryption) ({}):",
                    status.stale.len()
                ));
                for stale_file in &status.stale {
                    output::detail(&format!(
                        "{} (hash: {} → {})",
                        stale_file.plaintext.display(),
                        &stale_file.old_hash,
                        &stale_file.new_hash
                    ));
                }
                println!();
            }

            if !status.current.is_empty() {
                output::success(&format!("Current (up-to-date) ({}):", status.current.len()));
                for path in &status.current {
                    output::detail(&path.display());
                }
                println!();
            }

            if !status.orphaned.is_empty() {
                output::info(&format!("Orphaned .enc files ({}):", status.orphaned.len()));
                for path in &status.orphaned {
                    output::detail(&path.display());
                }
                println!();
            }

            if status.pending_encrypt.is_empty()
                && status.stale.is_empty()
                && status.current.is_empty()
                && status.orphaned.is_empty()
            {
                output::detail(&"No matching files found");
            }

            Ok(())
        }
        SopsCommands::GitignoreSync { yes, global } => {
            let global_settings = Settings::load_global_only().map_err(|e| {
                rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::Application(e))
            })?;
            let fs = Arc::new(RealFileSystem);
            let gitignore_service = GitignoreService::new(fs, global_settings);

            if global {
                // Global only: sync global gitignore
                let status = gitignore_service.status(None).map_err(|e| {
                    rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::Application(e))
                })?;

                if status.global_diff.in_sync {
                    output::success("Global gitignore is in sync with config");
                    return Ok(());
                }

                output::info(&format!(
                    "Global gitignore ({}):",
                    status.global_path.display()
                ));
                for pattern in &status.global_diff.to_add {
                    output::diff_add(pattern);
                }
                for pattern in &status.global_diff.to_remove {
                    output::diff_remove(pattern);
                }

                if !yes {
                    println!();
                    output::prompt(&"Update global gitignore? [Y/n]");
                    use std::io::Write;
                    std::io::stdout().flush().unwrap();
                    let mut input = String::new();
                    std::io::stdin().read_line(&mut input).unwrap();
                    let input = input.trim().to_lowercase();
                    if !input.is_empty() && input != "y" && input != "yes" {
                        output::info(&"Aborted");
                        return Ok(());
                    }
                }

                gitignore_service.sync_global().map_err(|e| {
                    rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::Application(e))
                })?;
                output::success(&format!(
                    "Global gitignore updated: {}",
                    status.global_path.display()
                ));
            } else {
                // Vault only: sync per-vault gitignore
                let vault_dir = vault_path.clone().ok_or_else(|| {
                    rsenv::cli::CliError::Usage(
                        "No vault found. Run 'rsenv init' first, or use --global.".into(),
                    )
                })?;

                let status = gitignore_service.status(Some(&vault_dir)).map_err(|e| {
                    rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::Application(e))
                })?;

                let vault_status = status.vault.as_ref().ok_or_else(|| {
                    rsenv::cli::CliError::Usage("No vault-local gitignore config found.".into())
                })?;

                if vault_status.diff.in_sync {
                    output::success("Per-vault gitignore is in sync with config");
                    return Ok(());
                }

                output::info(&format!(
                    "Per-vault gitignore ({}):",
                    vault_status.path.display()
                ));
                for pattern in &vault_status.diff.to_add {
                    output::diff_add(pattern);
                }
                for pattern in &vault_status.diff.to_remove {
                    output::diff_remove(pattern);
                }

                if !yes {
                    println!();
                    output::prompt(&"Update per-vault gitignore? [Y/n]");
                    use std::io::Write;
                    std::io::stdout().flush().unwrap();
                    let mut input = String::new();
                    std::io::stdin().read_line(&mut input).unwrap();
                    let input = input.trim().to_lowercase();
                    if !input.is_empty() && input != "y" && input != "yes" {
                        output::info(&"Aborted");
                        return Ok(());
                    }
                }

                gitignore_service.sync_vault(&vault_dir).map_err(|e| {
                    rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::Application(e))
                })?;
                output::success(&format!(
                    "Per-vault gitignore updated: {}",
                    vault_status.path.display()
                ));
            }

            Ok(())
        }
        SopsCommands::GitignoreStatus { global } => {
            let global_settings = Settings::load_global_only().map_err(|e| {
                rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::Application(e))
            })?;
            let fs = Arc::new(RealFileSystem);
            let gitignore_service = GitignoreService::new(fs, global_settings);

            if global {
                // Global only: show global gitignore status
                let status = gitignore_service.status(None).map_err(|e| {
                    rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::Application(e))
                })?;

                output::header("Global Gitignore Status:");
                println!();
                output::info(&format!("Path: {}", status.global_path.display()));
                if status.global_diff.in_sync {
                    output::success_detail("In sync with config");
                } else {
                    output::failure("Out of sync:");
                    for pattern in &status.global_diff.to_add {
                        println!("    {} {} (missing)", "+".green(), pattern);
                    }
                    for pattern in &status.global_diff.to_remove {
                        println!("    {} {} (extra)", "-".red(), pattern);
                    }
                }
            } else {
                // Vault only: show per-vault gitignore status
                let vault_dir = vault_path.clone().ok_or_else(|| {
                    rsenv::cli::CliError::Usage(
                        "No vault found. Run 'rsenv init' first, or use --global.".into(),
                    )
                })?;

                let status = gitignore_service.status(Some(&vault_dir)).map_err(|e| {
                    rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::Application(e))
                })?;

                output::header("Per-vault Gitignore Status:");
                println!();

                if let Some(vault_status) = &status.vault {
                    output::info(&format!("Path: {}", vault_status.path.display()));
                    if vault_status.diff.in_sync {
                        output::success_detail("In sync with vault-local config");
                    } else {
                        output::failure("Out of sync:");
                        for pattern in &vault_status.diff.to_add {
                            println!("    {} {} (missing)", "+".green(), pattern);
                        }
                        for pattern in &vault_status.diff.to_remove {
                            println!("    {} {} (extra)", "-".red(), pattern);
                        }
                    }
                } else {
                    output::info(&"No vault-local gitignore config found");
                }
            }

            Ok(())
        }
        SopsCommands::GitignoreClean { global } => {
            let global_settings = Settings::load_global_only().map_err(|e| {
                rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::Application(e))
            })?;
            let fs = Arc::new(RealFileSystem);
            let gitignore_service = GitignoreService::new(fs, global_settings);

            if global {
                // Global only: clean global gitignore
                let cleaned = gitignore_service.clean_global().map_err(|e| {
                    rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::Application(e))
                })?;

                if cleaned {
                    output::success(&format!(
                        "Removed rsenv-managed section from global gitignore: {}",
                        gitignore_service.global_gitignore_path().display()
                    ));
                } else {
                    output::info(&"Global gitignore: no managed section to remove");
                }
            } else {
                // Vault only: clean per-vault gitignore
                let vault_dir = vault_path.clone().ok_or_else(|| {
                    rsenv::cli::CliError::Usage(
                        "No vault found. Run 'rsenv init' first, or use --global.".into(),
                    )
                })?;

                let cleaned = gitignore_service.clean_vault(&vault_dir).map_err(|e| {
                    rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::Application(e))
                })?;

                if cleaned {
                    output::success(&format!(
                        "Removed rsenv-managed section from per-vault gitignore: {}",
                        gitignore_service.vault_gitignore_path(&vault_dir).display()
                    ));
                } else {
                    output::info(&"Per-vault gitignore: no managed section to remove");
                }
            }

            Ok(())
        }
        SopsCommands::Migrate {
            dir,
            global,
            vault_base,
            yes,
        } => {
            let vault_base_dir = vault_base.unwrap_or_else(|| settings.vault_base_dir.clone());
            let base_dir = resolve_sops_dir(dir, global, vault_path.as_deref(), &vault_base_dir)?;

            output::header(&format!(
                "Migrating old .enc files to hash-based format in: {}",
                base_dir.display()
            ));

            // Check for old format files first
            let status = service.status(Some(&base_dir)).map_err(|e| {
                rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::Application(e))
            })?;

            let old_format_count = status
                .orphaned
                .iter()
                .filter(|p| rsenv::application::hash::is_old_enc_format(p))
                .count();

            if old_format_count == 0 {
                output::success("No old format .enc files found. Nothing to migrate.");
                return Ok(());
            }

            output::info(&format!(
                "Found {} old format .enc files to migrate",
                old_format_count
            ));

            if !yes {
                output::warning("This will decrypt files temporarily and rename them.");
                output::warning("Make sure you have the decryption key available.");
                println!();
                output::info("Run with --yes to proceed, or press Ctrl+C to abort.");
                return Ok(());
            }

            let migrated = service.migrate(Some(&base_dir)).map_err(|e| {
                rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::Application(e))
            })?;

            if migrated.is_empty() {
                output::info("No files migrated");
            } else {
                output::success(&format!("Migrated {} files:", migrated.len()));
                for (old_path, new_path) in &migrated {
                    output::detail(&format!(
                        "{} → {}",
                        old_path.file_name().unwrap_or_default().to_string_lossy(),
                        new_path.file_name().unwrap_or_default().to_string_lossy()
                    ));
                }
            }

            Ok(())
        }
    }
}

/// Pre-commit hook template for vault git repos.
const PRE_COMMIT_HOOK: &str = r#"#!/bin/bash
# rsenv pre-commit hook - prevents committing with stale/unencrypted files
# Installed by: rsenv hook install

VAULT_DIR="$(git rev-parse --show-toplevel)"

if ! command -v rsenv &> /dev/null; then
    echo "Warning: rsenv not found in PATH, skipping encryption check"
    exit 0
fi

if ! rsenv sops status --check --dir "$VAULT_DIR" 2>/dev/null; then
    echo ""
    echo "ERROR: Unencrypted or stale files in vault."
    echo "       Run 'rsenv sops encrypt' to update encryption."
    echo "       Use 'rsenv sops status' to see details."
    echo ""
    exit 1
fi
"#;

fn handle_hook(
    command: HookCommands,
    vault_path: Option<std::path::PathBuf>,
    _settings: &Settings,
) -> rsenv::cli::CliResult<()> {
    let vault_dir = vault_path.ok_or_else(|| {
        rsenv::cli::CliError::Usage("No vault found. Run 'rsenv init' first.".into())
    })?;

    // Check if vault is a git repo
    let git_dir = vault_dir.join(".git");
    let hooks_dir = git_dir.join("hooks");
    let hook_path = hooks_dir.join("pre-commit");

    match command {
        HookCommands::Install { force } => {
            if !git_dir.exists() {
                return Err(rsenv::cli::CliError::Usage(format!(
                    "Vault is not a git repository: {}",
                    vault_dir.display()
                )));
            }

            if hook_path.exists() && !force {
                return Err(rsenv::cli::CliError::Usage(format!(
                    "Pre-commit hook already exists: {}\nUse --force to overwrite.",
                    hook_path.display()
                )));
            }

            // Create hooks directory if needed
            std::fs::create_dir_all(&hooks_dir).map_err(|e| {
                rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::Application(
                    rsenv::application::ApplicationError::OperationFailed {
                        context: format!("create hooks dir: {}", hooks_dir.display()),
                        source: Box::new(e),
                    },
                ))
            })?;

            // Write hook file
            std::fs::write(&hook_path, PRE_COMMIT_HOOK).map_err(|e| {
                rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::Application(
                    rsenv::application::ApplicationError::OperationFailed {
                        context: format!("write hook: {}", hook_path.display()),
                        source: Box::new(e),
                    },
                ))
            })?;

            // Make executable (Unix)
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = std::fs::metadata(&hook_path)
                    .map_err(|e| {
                        rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::Application(
                            rsenv::application::ApplicationError::OperationFailed {
                                context: "get hook permissions".into(),
                                source: Box::new(e),
                            },
                        ))
                    })?
                    .permissions();
                perms.set_mode(0o755);
                std::fs::set_permissions(&hook_path, perms).map_err(|e| {
                    rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::Application(
                        rsenv::application::ApplicationError::OperationFailed {
                            context: "set hook permissions".into(),
                            source: Box::new(e),
                        },
                    ))
                })?;
            }

            output::success(&format!(
                "Installed pre-commit hook: {}",
                hook_path.display()
            ));
            output::info("Hook will block commits when unencrypted files exist in vault.");
        }
        HookCommands::Remove => {
            if !hook_path.exists() {
                output::info("No pre-commit hook found");
                return Ok(());
            }

            // Check if it's our hook
            let content = std::fs::read_to_string(&hook_path).unwrap_or_default();
            if !content.contains("rsenv pre-commit hook") {
                return Err(rsenv::cli::CliError::Usage(format!(
                    "Pre-commit hook exists but was not installed by rsenv: {}",
                    hook_path.display()
                )));
            }

            std::fs::remove_file(&hook_path).map_err(|e| {
                rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::Application(
                    rsenv::application::ApplicationError::OperationFailed {
                        context: format!("remove hook: {}", hook_path.display()),
                        source: Box::new(e),
                    },
                ))
            })?;

            output::success(&format!("Removed pre-commit hook: {}", hook_path.display()));
        }
        HookCommands::Status => {
            if !git_dir.exists() {
                output::warning(&format!(
                    "Vault is not a git repository: {}",
                    vault_dir.display()
                ));
                return Ok(());
            }

            if !hook_path.exists() {
                output::info("No pre-commit hook installed");
                output::detail("Run 'rsenv hook install' to install encryption check hook");
            } else {
                let content = std::fs::read_to_string(&hook_path).unwrap_or_default();
                if content.contains("rsenv pre-commit hook") {
                    output::success(&format!(
                        "rsenv pre-commit hook installed: {}",
                        hook_path.display()
                    ));
                } else {
                    output::warning(&format!(
                        "Pre-commit hook exists but not managed by rsenv: {}",
                        hook_path.display()
                    ));
                }
            }
        }
    }

    Ok(())
}

fn handle_swap(
    command: SwapCommands,
    project_dir_opt: Option<std::path::PathBuf>,
    settings: &Settings,
) -> rsenv::cli::CliResult<()> {
    // For project commands: resolve to cwd if not provided
    // For vault-wide commands: -C overrides vault_base_dir
    let project_dir = project_dir_opt
        .clone()
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    let fs = Arc::new(RealFileSystem);
    let settings = Arc::new(settings.clone());
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let service = SwapService::new(fs, vault_service, settings.clone());

    match command {
        SwapCommands::In { files } => {
            if files.is_empty() {
                return Err(rsenv::cli::CliError::Usage("no files specified".into()));
            }

            let swapped = service.swap_in(&project_dir, &files).map_err(|e| {
                rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::Application(e))
            })?;

            output::info(&format!("Swapped in {} files:", swapped.len()));
            for file in &swapped {
                output::detail(&format!(
                    "{} <- {}",
                    file.project_path.display(),
                    file.vault_path.display()
                ));
            }
            Ok(())
        }
        SwapCommands::Out {
            files,
            global,
            vault_base,
        } => {
            if global {
                // Global: swap out all vaults
                let vault_base_dir = vault_base.unwrap_or_else(|| settings.vault_base_dir.clone());

                let results = service.swap_out_all_vaults(&vault_base_dir).map_err(|e| {
                    rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::Application(e))
                })?;

                if results.is_empty() {
                    output::info(&"No active swaps across all vaults");
                } else {
                    output::info(&format!("Swapped out files in {} vaults:", results.len()));
                    for status in &results {
                        output::detail(&format!(
                            "{}: {} files",
                            status.vault_id,
                            status.active_swaps.len()
                        ));
                    }
                }
                Ok(())
            } else if files.is_empty() {
                // Vault-level (default): swap out all files in current project's vault
                let swapped = service.swap_out_vault(&project_dir).map_err(|e| {
                    rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::Application(e))
                })?;

                if swapped.is_empty() {
                    output::info(&"No files swapped in");
                } else {
                    output::info(&format!("Swapped out {} files:", swapped.len()));
                    for file in &swapped {
                        output::detail(&file.project_path.display());
                    }
                }
                Ok(())
            } else {
                // File-level: swap out specific files
                let swapped = service.swap_out(&project_dir, &files).map_err(|e| {
                    rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::Application(e))
                })?;

                output::info(&format!("Swapped out {} files:", swapped.len()));
                for file in &swapped {
                    output::detail(&format!(
                        "{} (restored original)",
                        file.project_path.display()
                    ));
                }
                Ok(())
            }
        }
        SwapCommands::Init { files } => {
            if files.is_empty() {
                return Err(rsenv::cli::CliError::Usage("no files specified".into()));
            }

            let initialized = service.swap_init(&project_dir, &files).map_err(|e| {
                rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::Application(e))
            })?;

            output::info(&format!(
                "Initialized {} files in vault:",
                initialized.len()
            ));
            for file in &initialized {
                output::detail(&format!(
                    "{} -> {}",
                    file.project_path.display(),
                    file.vault_path.display()
                ));
            }
            Ok(())
        }
        SwapCommands::Status {
            absolute,
            global,
            silent,
            vault_base,
        } => {
            if global {
                // Global: show status across all vaults
                let vault_base_dir = vault_base.unwrap_or_else(|| settings.vault_base_dir.clone());

                let statuses = service.status_all_vaults(&vault_base_dir).map_err(|e| {
                    rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::Application(e))
                })?;

                let has_active = !statuses.is_empty();

                if silent {
                    if has_active {
                        std::process::exit(1);
                    }
                    return Ok(());
                }

                if statuses.is_empty() {
                    output::info(&"No active swaps across all vaults");
                } else {
                    output::header("Active Swaps:");
                    for status in &statuses {
                        println!();
                        output::info(&format!("{}:", status.vault_id));
                        for file in &status.active_swaps {
                            let display_path = status
                                .project_path
                                .as_ref()
                                .and_then(|p| file.project_path.strip_prefix(p).ok())
                                .map(|p| p.to_path_buf())
                                .unwrap_or_else(|| file.project_path.clone());
                            let hostname = match &file.state {
                                rsenv::domain::SwapState::In { hostname } => hostname,
                                _ => "unknown",
                            };
                            output::detail(&format!(
                                "{} [in ({})]",
                                display_path.display(),
                                hostname
                            ));
                        }
                    }
                }
                Ok(())
            } else {
                // Project-level: show status for current project
                let status = service.status(&project_dir).map_err(|e| {
                    rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::Application(e))
                })?;

                if status.is_empty() {
                    output::info(&"No swappable files found");
                    return Ok(());
                }

                output::header("Swap Status:");
                for file in &status {
                    let display_path = if absolute {
                        file.project_path.display().to_string()
                    } else {
                        file.project_path
                            .strip_prefix(&project_dir)
                            .unwrap_or(&file.project_path)
                            .display()
                            .to_string()
                    };
                    let state_str = match &file.state {
                        rsenv::domain::SwapState::Out => "out".normal(),
                        rsenv::domain::SwapState::In { hostname } => {
                            format!("in ({})", hostname).green()
                        }
                    };
                    println!("  {} [{}]", display_path, state_str);
                }
                Ok(())
            }
        }
        SwapCommands::Delete { files } => {
            if files.is_empty() {
                return Err(rsenv::cli::CliError::Usage("no files specified".into()));
            }

            let deleted = service.delete(&project_dir, &files).map_err(|e| {
                rsenv::cli::CliError::Infra(rsenv::infrastructure::InfraError::Application(e))
            })?;

            output::info(&format!("Deleted {} files from swap:", deleted.len()));
            for file in &deleted {
                output::detail(&file.project_path.display());
            }
            Ok(())
        }
    }
}

fn generate_completions(shell: clap_complete::Shell) {
    let mut cmd = Cli::command();
    clap_complete::generate(shell, &mut cmd, "rsenv", &mut io::stdout());
}

/// Generate vimscript for grid layout of env file branches.
///
/// Each branch (leaf → root chain) becomes a column.
/// Files within each column are stacked vertically.
fn create_vimscript(branches: &[Vec<std::path::PathBuf>]) -> String {
    let mut script = String::new();

    for (col_idx, col_files) in branches.iter().enumerate() {
        if col_files.is_empty() {
            continue;
        }

        if col_idx == 0 {
            // First column: start with 'edit' for the first file
            script.push_str(&format!("edit {}\n", col_files[0].display()));
        } else {
            // Subsequent columns: split and move right
            script.push_str(&format!("split {}\n", col_files[0].display()));
            script.push_str("wincmd L\n");
        }

        // Rest of files in this column: add as vertical splits
        for file in &col_files[1..] {
            script.push_str(&format!("split {}\n", file.display()));
        }
    }

    // Equalize window sizes and jump to top-left
    script.push_str("\nwincmd =\n");
    script.push_str("1wincmd w\n");

    script
}
