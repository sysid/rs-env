#![allow(unused_imports)]

use std::collections::{BTreeMap};
use std::fs::File;
use std::io::{BufRead, BufReader};
use anyhow::{Context, Result};
use log::{debug, info};
use std::env;
use camino::{Utf8Path, Utf8PathBuf};
use rstest::rstest;
use stdext::function_name;
use rsenv::edit::{open_files_in_editor, select_file_with_suffix};
use rsenv::get_files;

#[rstest]
#[ignore = "Interactive via Makefile"]
fn test_select_file_with_suffix() {
    let dir = "./tests/resources/data";
    let suffix = ".env";
    let result = select_file_with_suffix(dir, suffix);
    println!("Selected: {:?}", result);
    assert!(result.is_some());
}

#[rstest]
#[ignore = "Interactive via Makefile"]
fn test_open_files_in_editor() {
    let files = get_files("./tests/resources/data/level4.env").unwrap();
    open_files_in_editor(files).unwrap();
}
