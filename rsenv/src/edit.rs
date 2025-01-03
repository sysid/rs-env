use std::path::{Path, PathBuf};
use std::process::Command;
use std::env;
use std::sync::Arc;

use walkdir::WalkDir;
use skim::prelude::*;
use crossbeam::channel::bounded;
use crossterm::{execute, terminal::{Clear, ClearType}};
use tracing::{debug, instrument};

use crate::errors::{TreeError, TreeResult};
use crate::arena::TreeArena;

#[instrument(level = "debug")]
pub fn select_file_with_suffix(dir: &Path, suffix: &str) -> TreeResult<PathBuf> {
    debug!("Searching for files with suffix {} in {:?}", suffix, dir);

    // List all files with the given suffix
    let files: Vec<PathBuf> = WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| !e.path().is_dir())
        .filter(|e| e.path().to_string_lossy().ends_with(suffix))
        .map(|e| e.path().to_path_buf())
        .collect();

    if files.is_empty() {
        return Err(TreeError::InternalError(format!(
            "No files found with suffix {} in {:?}",
            suffix, dir
        )));
    }

    // Step 2: Create a channel and send items to the skim interface
    // create a channel with a bounded capacity (in this case, 100). The tx (sender) part of the
    // channel is used to send items, and the rx (receiver) part is passed to Skim::run_with().
    let (tx, rx) = bounded(100);

    // Skim::run_with() expects a stream of items that implement the SkimItem trait,
    // which we can achieve by transforming our Vec<String> into a stream of Arc<dyn SkimItem> objects.
    // For each file path String, we convert it to an Arc<String> and then to Arc<dyn SkimItem>,
    // just like before. We then send each of these items through the tx (sender) part of the channel.
    for file in files.iter() {
        let item = Arc::new(file.to_string_lossy().into_owned()) as Arc<dyn SkimItem>;
        tx.send(item).map_err(|e| TreeError::InternalError(
            format!("Failed to send item through channel: {}", e)
        ))?;
    }

    // This step is important because Skim::run_with() needs to know when there are no more items to expect.
    drop(tx); // Close the channel

    let options = SkimOptionsBuilder::default()
        .height(Some("50%"))
        .multi(false)
        .build()
        .map_err(|e| TreeError::InternalError(
            format!("Failed to build skim options: {}", e)
        ))?;

    // Running Skim with the Receiver: Instead of creating and passing a stream of items directly,
    // we just pass the rx (receiver) part of the channel to Skim::run_with().
    let selected_items = Skim::run_with(&options, Some(rx))
        .map(|out| out.selected_items)
        .unwrap_or_default();

    // Clear screen
    let mut stdout = std::io::stdout();
    execute!(stdout, Clear(ClearType::FromCursorDown))
        .map_err(|e| TreeError::InternalError(
            format!("Failed to clear screen: {}", e)
        ))?;

    // Step 3: Save the selection into a variable for later use
    selected_items
        .first()
        .map(|item| PathBuf::from(item.output().to_string()))
        .ok_or_else(|| TreeError::InternalError("No file selected".to_string()))
}

#[instrument(level = "debug")]
pub fn open_files_in_editor(files: Vec<PathBuf>) -> TreeResult<()> {
    debug!("Opening files in editor: {:?}", files);

    let editor = env::var("EDITOR").unwrap_or_else(|_| "vim".to_string());
    if !editor.contains("vim") {
        return Err(TreeError::InternalError("Only vim is supported for now".to_string()));
    }

    let file_paths: Vec<String> = files.iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect();

    // Spawn a new process to run the editor.
    // For Vim and NeoVim, the `-p` option opens files in separate tabs.
    Command::new(&editor)
        .arg("-O")
        .args(&file_paths)
        .status()
        .map_err(|e| TreeError::InternalError(format!("Failed to run editor: {}", e)))?;

    Ok(())
}

#[instrument(level = "debug")]
pub fn create_vimscript(files: Vec<Vec<&Path>>) -> String {
    debug!("Creating vimscript for files: {:?}", files);

    let mut script = String::new();

    for (col_idx, col_files) in files.iter().enumerate() {
        if col_files.is_empty() {
            continue;
        }

        if col_idx == 0 {
            // For the first column, start with 'edit' for the first file
            script.push_str(&format!("\" Open the first set of files ('{}') in the first column\n",
                col_files[0].display()));
            script.push_str(&format!("edit {}\n", col_files[0].display()));
        } else {
            // For subsequent columns, start with a 'split' for the first file in the list
            // and move the cursor to the new (right) column
            script.push_str(&format!("split {}\n", col_files[0].display()));
            script.push_str("\" move to right column\nwincmd L\n");
        }

        // For the rest of the files in the list, add a 'split' command for each
        for file in &col_files[1..] {
            script.push_str(&format!("split {}\n", file.display()));
        }
    }

    // Add the final commands to the script
    script.push_str("\n\" make distribution equal\nwincmd =\n");
    script.push_str("\n\" jump to left top corner\n1wincmd w\n");

    script
}

#[instrument(level = "debug")]
pub fn create_branches(trees: &[TreeArena]) -> Vec<Vec<PathBuf>> {
    debug!("Creating branches for {} trees", trees.len());

    let mut vimscript_files = Vec::new();

    for (tree_idx, tree) in trees.iter().enumerate() {
        debug!("Processing tree {}", tree_idx);

        let leaf_nodes = tree.leaf_nodes();
        debug!("Found {} leaf nodes", leaf_nodes.len());

        for leaf in &leaf_nodes {
            debug!("Processing leaf: {}", leaf.to_string());

            let mut branch = Vec::new();
            if let Ok(files) = crate::get_files(Path::new(leaf)) {
                debug!("Found {} files in branch", files.len());
                branch.extend(files);
                vimscript_files.push(branch);
            }
        }
    }

    debug!("Created {} branches", vimscript_files.len());
    vimscript_files
}