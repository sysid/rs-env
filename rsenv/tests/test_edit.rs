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
use rsenv::edit::{create_branches, create_vimscript, open_files_in_editor, select_file_with_suffix};
use rsenv::get_files;
use rsenv::tree::build_trees;

#[ctor::ctor]
fn init() {
    let _ = env_logger::builder()
        .filter_level(log::LevelFilter::max())
        .is_test(true)
        .try_init();
}

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
    let files = get_files("./tests/resources/environments/complex/level4.env").unwrap();
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

#[rstest]
fn test_create_branches_tree() {
    let trees = build_trees(Utf8Path::new("./tests/resources/environments/tree")).unwrap();
    let result = create_branches(&trees);
    println!("{:#?}", result);

    let expected = vec![
        vec![
            "/Users/Q187392/dev/s/public/rs-env/rsenv/tests/resources/environments/tree/level11.env",
            "/Users/Q187392/dev/s/public/rs-env/rsenv/tests/resources/environments/tree/root.env",
        ],
        vec![
            "/Users/Q187392/dev/s/public/rs-env/rsenv/tests/resources/environments/tree/level13.env",
            "/Users/Q187392/dev/s/public/rs-env/rsenv/tests/resources/environments/tree/root.env",
        ],
        vec![
            "/Users/Q187392/dev/s/public/rs-env/rsenv/tests/resources/environments/tree/level32.env",
            "/Users/Q187392/dev/s/public/rs-env/rsenv/tests/resources/environments/tree/level22.env",
            "/Users/Q187392/dev/s/public/rs-env/rsenv/tests/resources/environments/tree/level12.env",
            "/Users/Q187392/dev/s/public/rs-env/rsenv/tests/resources/environments/tree/root.env",
        ],
        vec![
            "/Users/Q187392/dev/s/public/rs-env/rsenv/tests/resources/environments/tree/level21.env",
            "/Users/Q187392/dev/s/public/rs-env/rsenv/tests/resources/environments/tree/level12.env",
            "/Users/Q187392/dev/s/public/rs-env/rsenv/tests/resources/environments/tree/root.env",
        ],
    ];
    assert_eq!(result, expected);
}

#[rstest]
fn test_create_branches_parallel() {
    let trees = build_trees(Utf8Path::new("./tests/resources/environments/parallel")).unwrap();
    let result = create_branches(&trees);
    println!("{:#?}", result);

    let expected = vec![
        vec![
            "/Users/Q187392/dev/s/public/rs-env/rsenv/tests/resources/environments/parallel/int.env",
            "/Users/Q187392/dev/s/public/rs-env/rsenv/tests/resources/environments/parallel/b_int.env",
            "/Users/Q187392/dev/s/public/rs-env/rsenv/tests/resources/environments/parallel/a_int.env",
        ],
        vec![
            "/Users/Q187392/dev/s/public/rs-env/rsenv/tests/resources/environments/parallel/prod.env",
            "/Users/Q187392/dev/s/public/rs-env/rsenv/tests/resources/environments/parallel/b_prod.env",
            "/Users/Q187392/dev/s/public/rs-env/rsenv/tests/resources/environments/parallel/a_prod.env",
        ],
        vec![
            "/Users/Q187392/dev/s/public/rs-env/rsenv/tests/resources/environments/parallel/test.env",
            "/Users/Q187392/dev/s/public/rs-env/rsenv/tests/resources/environments/parallel/b_test.env",
            "/Users/Q187392/dev/s/public/rs-env/rsenv/tests/resources/environments/parallel/a_test.env",
        ],
    ];
    assert_eq!(result, expected);
}

#[rstest]
fn test_create_branches_complex() {
    let trees = build_trees(Utf8Path::new("./tests/resources/environments/complex")).unwrap();
    let result = create_branches(&trees);
    println!("{:#?}", result);

    let expected = vec![
        vec![
            "/Users/Q187392/dev/s/public/rs-env/rsenv/tests/resources/environments/complex/level4.env",
            "/Users/Q187392/dev/s/public/rs-env/rsenv/tests/resources/environments/complex/a/level3.env",
            "/Users/Q187392/dev/s/public/rs-env/rsenv/tests/resources/environments/complex/level2.env",
            "/Users/Q187392/dev/s/public/rs-env/rsenv/tests/resources/environments/complex/level1.env",
            "/Users/Q187392/dev/s/public/rs-env/rsenv/tests/resources/environments/complex/dot.envrc",
        ],
    ];
    assert_eq!(result, expected);
}
