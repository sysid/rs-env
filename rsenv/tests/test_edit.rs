#![allow(unused_imports)]

use std::collections::{BTreeMap};
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use anyhow::{Context, Result};
use log::{debug, info};
use std::env;
use std::process::Command;
use camino::{Utf8Path, Utf8PathBuf};
use rstest::rstest;
use stdext::function_name;
use rsenv::edit::{create_vimscript, open_files_in_editor, select_file_with_suffix};
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

#[rstest]
#[ignore = "Interactive via Makefile"]
fn test_create_vimscript() {
    let files = vec![
        vec!["a_test.env", "b_test.env", "test.env"],
        vec!["a_int.env", "b_int.env", "int.env"],
        // vec!["a_prod.env", "b_prod.env", "prod.env"]
        vec!["a_prod.env"],
    ];

    let script = create_vimscript(files);
    println!("{}", script);

    // If you want to save this to a file:
    let vimscript_filename = "tests/resources/environments/generated.vim";
    let mut file = std::fs::File::create(vimscript_filename).unwrap();
    file.write_all(script.as_bytes()).unwrap();

    // Run vim with the generated script
    let status = Command::new("vim")
        .arg("-S")
        .arg(vimscript_filename)
        .status()
        .expect("failed to run vim");

    println!("Vim exited with status: {:?}", status);
}

#[rstest]
fn test_create_vimscript_non_interactive() {
    let files = vec![
        vec!["a_test.env", "b_test.env", "test.env"],
        vec!["a_int.env", "b_int.env", "int.env"],
        vec!["a_prod.env"],
    ];

    let result = create_vimscript(files);

    let expected = "\
\" Open the first set of files ('a_test.env') in the first column
edit a_test.env
split b_test.env
split test.env
split a_int.env
\" move to right column
wincmd L
split b_int.env
split int.env
split a_prod.env
\" move to right column
wincmd L

\" make distribution equal
wincmd =

\" jump to left top corner
1wincmd w
";

    assert_eq!(result, expected);
}
