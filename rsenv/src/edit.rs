#![allow(unused_imports)]

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use anyhow::{Context, Result};
use log::{debug, info};
use std::env;
use camino::{Utf8Path, Utf8PathBuf};
use stdext::function_name;

use skim::prelude::*;
use walkdir::WalkDir;
use std::path::Path;
use std::process::Command;
use crossterm::{execute, terminal::{Clear, ClearType}};
use crossbeam::channel::bounded;
use itertools::Itertools;
use crate::dlog;
use crate::tree::TreeNode;


pub fn select_file_with_suffix(dir: &str, suffix: &str) -> Option<String> {
    // Step 1: List all files with the given suffix
    let files: Vec<String> = WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| !e.path().is_dir())
        .filter(|e| e.path().to_string_lossy().ends_with(suffix))
        .map(|e| e.path().to_string_lossy().into_owned())
        .collect();

    // Step 2: Create a channel and send items to the skim interface
    // create a channel with a bounded capacity (in this case, 100). The tx (sender) part of the
    // channel is used to send items, and the rx (receiver) part is passed to Skim::run_with().
    let (tx, rx) = bounded(100);

    // Skim::run_with() expects a stream of items that implement the SkimItem trait,
    // which we can achieve by transforming our Vec<String> into a stream of Arc<dyn SkimItem> objects.
    // For each file path String, we convert it to an Arc<String> and then to Arc<dyn SkimItem>,
    // just like before. We then send each of these items through the tx (sender) part of the channel.
    for file in files.iter() {
        let item = Arc::new(file.clone()) as Arc<dyn SkimItem>;
        tx.send(item).unwrap();
    }

    // This step is important because Skim::run_with() needs to know when there are no more items to expect.
    drop(tx); // Close the channel

    let options = SkimOptionsBuilder::default()
        .height(Some("50%"))
        .multi(false)
        .build()
        .unwrap();

    // Running Skim with the Receiver: Instead of creating and passing a stream of items directly,
    // we just pass the rx (receiver) part of the channel to Skim::run_with().
    let selected_items = Skim::run_with(&options, Some(rx))
        .map(|out| out.selected_items)
        .unwrap_or_else(Vec::new);

    // clear screen
    let mut stdout = std::io::stdout();
    execute!(stdout, Clear(ClearType::FromCursorDown)).unwrap();

    // Step 3: Save the selection into a variable for later use
    if let Some(item) = selected_items.get(0) {
        Some(item.output().to_string())
    } else {
        None
    }
}


pub fn open_files_in_editor(files: Vec<Utf8PathBuf>) -> std::io::Result<()> {
    // Get the editor command from the environment variable `$EDITOR`.
    // If `$EDITOR` is not set, default to "vim".
    let editor = env::var("EDITOR").unwrap_or_else(|_| "vim".to_string());
    if ! editor.contains("vim") {
        todo!("Only vim is supported for now");
    }

    // Prepare the list of file paths as strings.
    let file_paths: Vec<String> = files.iter().map(|path| path.to_string()).collect();

    // Spawn a new process to run the editor.
    // For Vim and NeoVim, the `-p` option opens files in separate tabs.
    Command::new(&editor)
        .arg("-O")
        .args(&file_paths)
        .status()?;

    Ok(())
}

pub fn create_vimscript(files: Vec<Vec<&str>>) -> String {
    let mut script = String::new();

    for (col_idx, col_files) in files.iter().enumerate() {
        if col_idx == 0 {
            // For the first column, start with 'edit' for the first file
            script.push_str(&format!("\" Open the first set of files ('{}') in the first column\n", col_files[0]));
            script.push_str(&format!("edit {}\n", col_files[0]));
        } else {
            // For subsequent columns, start with a 'split' for the first file in the list
            // and move the cursor to the new (right) column
            script.push_str(&format!("split {}\n", col_files[0]));
            script.push_str("\" move to right column\nwincmd L\n");
        }

        // For the rest of the files in the list, add a 'split' command for each
        for file in &col_files[1..] {
            script.push_str(&format!("split {}\n", file));
        }
    }

    // Add the final commands to the script
    script.push_str("\n\" make distribution equal\nwincmd =\n");
    script.push_str("\n\" jump to left top corner\n1wincmd w\n");

    script
}

pub fn create_branches(trees: &Vec<Rc<RefCell<TreeNode>>>) -> Vec<Vec<String>> {
    let mut vimscript_files: Vec<Vec<_>> = vec![];

    dlog!("trees: {:#?}", trees);
    for tree in trees {
        let leaf_nodes = tree.borrow().leaf_nodes();
        dlog!("Leaf nodes: {:#?}", leaf_nodes);

        for leaf in &leaf_nodes {
            println!("Leaf: {}", leaf);
            let mut branch = Vec::new();

            let files = crate::get_files(leaf).unwrap();
            for file in &files {
                println!("{}", file);
                branch.push(file.to_string());
            }
            vimscript_files.push(branch.clone());
        }
        println!();
    }
    dlog!("vimscript_files: {:#?}", vimscript_files);
    vimscript_files
}

