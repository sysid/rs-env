use crate::cli::args::{Cli, Commands};
use crate::edit::{
    create_branches, create_vimscript, open_files_in_editor, select_file_with_suffix,
};
use crate::envrc::update_dot_envrc;
use crate::builder::TreeBuilder;
use crate::{build_env_vars, get_files, is_dag, link_all, print_files};
use anyhow::{anyhow, Result};
use std::path::Path;
use std::process;
use std::io::Write;
use crossterm::style::Stylize;
use tracing::{debug, instrument};
use tempfile::NamedTempFile;

pub fn execute_command(cli: &Cli) -> Result<()> {
    match &cli.command {
        Some(Commands::Build { source_path }) => _build(source_path),
        Some(Commands::Envrc {
            source_path,
            envrc_path,
        }) => _envrc(source_path, envrc_path.as_deref()),
        Some(Commands::Files { source_path }) => _files(source_path),
        Some(Commands::EditLeaf { source_path }) => _edit_leaf(source_path),
        Some(Commands::Edit { source_dir }) => _edit(source_dir),
        Some(Commands::SelectLeaf { source_path }) => _select_leaf(source_path),
        Some(Commands::Select { source_dir }) => _select(source_dir),
        Some(Commands::Link { nodes }) => _link(nodes),
        Some(Commands::Branches { source_dir }) => _branches(source_dir),
        Some(Commands::Tree { source_dir }) => _tree(source_dir),
        Some(Commands::TreeEdit { source_dir }) => _tree_edit(source_dir),
        Some(Commands::Leaves { source_dir }) => _leaves(source_dir),
        None => Ok(())
    }
}

#[instrument]
fn _build(source_path: &str) -> Result<()> {
    debug!("source_path: {:?}", source_path);
    let vars = build_env_vars(Path::new(source_path)).unwrap_or_else(|e| {
        eprintln!("{}", format!("Cannot build environment: {}", e).red());
        process::exit(1);
    });
    println!("{}", vars);
    Ok(())
}

#[instrument]
fn _envrc(source_path: &str, envrc_path: Option<&str>) -> Result<()> {
    let envrc_path = envrc_path.unwrap_or(".envrc");
    debug!(
        "source_path: {:?}, envrc_path: {:?}",
        source_path,
        envrc_path
    );
    let vars = build_env_vars(Path::new(source_path)).unwrap_or_else(|e| {
        eprintln!("{}", format!("Cannot build environment: {}", e).red());
        process::exit(1);
    });
    update_dot_envrc(Path::new(envrc_path), vars.as_str())?;
    Ok(())
}

#[instrument]
fn _files(source_path: &str) -> Result<()> {
    debug!("source_path: {:?}", source_path);
    print_files(Path::new(source_path)).unwrap_or_else(|e| {
        eprintln!("{}", format!("Cannot print environment: {}", e).red());
        process::exit(1);
    });
    Ok(())
}

#[instrument]
fn _edit_leaf(source_path: &str) -> Result<()> {
    let path = Path::new(source_path);
    if !path.exists() {
        return Err(anyhow!("File does not exist: {:?}", source_path));
    }
    let files = get_files(path).unwrap_or_else(|e| {
        eprintln!("{}", format!("Cannot get files: {}", e).red());
        process::exit(1);
    });
    open_files_in_editor(files).unwrap_or_else(|e| {
        eprintln!("{}", format!("Cannot open files in editor: {}", e).red());
        process::exit(1);
    });
    Ok(())
}

#[instrument]
fn _edit(source_dir: &str) -> Result<()> {
    let path = Path::new(source_dir);
    if !path.exists() {
        eprintln!("Error: Directory does not exist: {:?}", source_dir);
        process::exit(1);
    }
    let selected_file = select_file_with_suffix(path, ".env").unwrap_or_else(|_| {
        eprintln!("{}", "No .env files found".to_string().red());
        process::exit(1);
    });
    println!("Selected: {}", selected_file.display());
    let files = get_files(&selected_file).unwrap_or_else(|e| {
        eprintln!("{}", format!("Cannot get files: {}", e).red());
        process::exit(1);
    });
    open_files_in_editor(files).unwrap_or_else(|e| {
        eprintln!("{}", format!("Cannot open files in editor: {}", e).red());
        process::exit(1);
    });
    Ok(())
}

#[instrument]
fn _select_leaf(source_path: &str) -> Result<()> {
    let path = Path::new(source_path);
    if !path.exists() {
        eprintln!("Error: File does not exist: {:?}", source_path);
        process::exit(1);
    }
    _envrc(source_path, None)
}

#[instrument]
fn _select(source_dir: &str) -> Result<()> {
    let path = Path::new(source_dir);
    if !path.exists() {
        eprintln!("Error: Directory does not exist: {:?}", source_dir);
        process::exit(1);
    }
    let selected_file = select_file_with_suffix(path, ".env").unwrap_or_else(|_| {
        eprintln!("{}", "No .env files found.".to_string().red());
        process::exit(1);
    });
    println!("Selected: {}", selected_file.display());
    _envrc(selected_file.to_str().unwrap(), None)
}

#[instrument]
fn _link(nodes: &[String]) -> Result<()> {
    let paths = nodes.iter()
        .map(|s| Path::new(s).to_path_buf())
        .collect::<Vec<_>>();
    link_all(&paths);
    println!("Linked: {}", nodes.join(" <- "));
    Ok(())
}

#[instrument]
fn _branches(source_path: &str) -> Result<()> {
    debug!("source_path: {:?}", source_path);
    let path = Path::new(source_path);
    if is_dag(path).expect("Failed to determine if DAG") {
        eprintln!(
            "{}",
            "Dependencies form a DAG, you cannot use tree based commands.".to_string().red()
        );
        process::exit(1);
    }
    let mut builder = TreeBuilder::new();
    let trees = builder.build_from_directory(path).unwrap_or_else(|e| {
        eprintln!("{}", format!("Cannot build trees: {}", e).red());
        process::exit(1);
    });
    println!("Found {} trees:\n", trees.len());
    for tree in &trees {
        if let Some(root_idx) = tree.root() {
            if let Some(root_node) = tree.get_node(root_idx) {
                println!("Tree Root: {}", root_node.data.file_path.display());
                let mut path = vec![root_node.data.file_path.to_str().unwrap().to_string()];
                // TODO: Implement print_leaf_paths for arena-based tree
                println!();
            }
        }
    }
    Ok(())
}

#[instrument]
fn _tree(source_path: &str) -> Result<()> {
    debug!("source_path: {:?}", source_path);
    let path = Path::new(source_path);
    if is_dag(path).expect("Failed to determine if DAG") {
        eprintln!(
            "{}",
            "Dependencies form a DAG, you cannot use tree based commands.".to_string().red()
        );
        process::exit(1);
    }
    let mut builder = TreeBuilder::new();
    let trees = builder.build_from_directory(path).unwrap_or_else(|e| {
        eprintln!("{}", format!("Cannot build trees: {}", e).red());
        process::exit(1);
    });
    println!("Found {} trees:\n", trees.len());
    for tree in &trees {
        if let Some(root_idx) = tree.root() {
            if let Some(root_node) = tree.get_node(root_idx) {
                println!("{}", root_node.data.file_path.display());
            }
        }
    }
    Ok(())
}

#[instrument]
fn _tree_edit(source_path: &str) -> Result<()> {
    // vim -O3 test.env int.env prod.env -c "wincmd h" -c "sp test.env" -c "wincmd l" -c "sp int.env" -c "wincmd l" -c "sp prod.env"
    debug!("source_path: {:?}", source_path);
    let path = Path::new(source_path);
    if is_dag(path).expect("Failed to determine if DAG") {
        eprintln!(
            "{}",
            "Dependencies form a DAG, you cannot use tree based commands.".to_string().red()
        );
        process::exit(1);
    }
    let mut builder = TreeBuilder::new();
    let trees = builder.build_from_directory(path).unwrap_or_else(|e| {
        eprintln!("{}", format!("Cannot build trees: {}", e).red());
        process::exit(1);
    });
    println!("Editing {} trees...", trees.len());

    let vimscript_files: Vec<Vec<_>> = create_branches(&trees);

    let vimscript = create_vimscript(
        vimscript_files
            .iter()
            .map(|v| v.iter().map(|s| s.as_path()).collect())
            .collect(),
    );

    let mut tmpfile = NamedTempFile::new()?;
    tmpfile.write_all(vimscript.as_bytes())?;

    let status = process::Command::new("vim")
        .arg("-S")
        .arg(tmpfile.path())
        .status()
        .expect("failed to run vim");

    println!("Vim: {}", status);
    Ok(())
}

#[instrument]
fn _leaves(source_path: &str) -> Result<()> {
    debug!("source_path: {:?}", source_path);
    let path = Path::new(source_path);
    if is_dag(path).expect("Failed to determine if DAG") {
        eprintln!(
            "{}",
            "Dependencies form a DAG, you cannot use tree based commands.".to_string().red()
        );
        process::exit(1);
    }
    let mut builder = TreeBuilder::new();
    let trees = builder.build_from_directory(path).unwrap_or_else(|e| {
        eprintln!("{}", format!("Cannot build trees: {}", e).red());
        process::exit(1);
    });
    debug!("Found {} trees:\n", trees.len());
    for tree in &trees {
        let leaf_nodes = tree.leaf_nodes();
        for leaf in &leaf_nodes {
            println!("{}", leaf);
        }
    }
    Ok(())
}
