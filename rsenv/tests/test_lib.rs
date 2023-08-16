#![allow(unused_imports)]

use std::collections::BTreeMap;
use std::fs;
use anyhow::Result;
use camino::Utf8PathBuf;
use camino_tempfile::tempdir;
use fs_extra::{copy_items, dir};
use rstest::{fixture, rstest};
use rsenv::{build_env, dlog, extract_env, build_env_vars, print_files, link};
use log::{debug, info};
use stdext::function_name;

#[ctor::ctor]
fn init() {
    let _ = env_logger::builder()
        .filter_level(log::LevelFilter::max())
        .is_test(true)
        .try_init();
}

#[fixture]
fn temp_dir() -> Utf8PathBuf {
    let tempdir = tempdir().unwrap();
    let options = dir::CopyOptions::new(); //Initialize default values for CopyOptions
    copy_items(
        &[
            "tests/resources/data/level1.env",
            "tests/resources/data/level2.env",
            "tests/resources/data/a",
        ],
        &tempdir,
        &options,
    )
        .expect("Failed to copy test project directory");

    tempdir.into_path()
}

#[rstest]
fn test_extract_env() -> Result<()> {
    let (variables, parent) = extract_env("./tests/resources/data/level4.env")?;
    dlog!("variables: {:?}", variables);
    dlog!("parent: {:?}", parent);
    assert_eq!(variables.get("VAR_6"), Some(&"var_64".to_string()));
    // assert_eq!(parent, Some("a/level3.env".to_string()));
    Ok(())
}

#[rstest]
fn test_build_env() -> Result<()> {
    let (variables, files) = build_env("./tests/resources/data/level4.env")?;
    let reference = extract_env("./tests/resources/data/result.env")?.0;
    // println!("reference: {:#?}", reference);
    // println!("variables: {:#?}", variables);
    let filtered_map: BTreeMap<_, _> = variables.iter()
        .filter(|(k, _)| k.starts_with("VAR_"))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    println!("variables: {:#?}", filtered_map);
    println!("files: {:#?}", files);

    assert_eq!(filtered_map, reference, "The two BTreeMaps are not equal!");
    Ok(())
}

#[rstest]
fn test_build_env_vars() -> Result<()> {
    let env_vars = build_env_vars("./tests/resources/data/level4.env")?;
    println!("{}", env_vars);
    Ok(())
}

#[rstest]
fn test_print_files() -> Result<()> {
    print_files("./tests/resources/data/level4.env")?;
    Ok(())
}

#[rstest]
fn test_link(temp_dir: Utf8PathBuf) -> Result<()> {
    let parent = temp_dir.join("./a/level3.env");
    let child = temp_dir.join("./level1.env");
    link(parent.as_str(), child.as_str())?;

    let child_content = fs::read_to_string(&child)?;
    assert!(child_content.contains("# rsenv: a/level3.env"));
    Ok(())
}
