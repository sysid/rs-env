#![allow(unused_imports)]

use std::collections::BTreeMap;
use std::fs;
use anyhow::Result;
use camino::Utf8PathBuf;
use camino_tempfile::tempdir;
use fs_extra::{copy_items, dir};
use rstest::{fixture, rstest};
use rsenv::{build_env, dlog, extract_env, build_env_vars, print_files, link, link_all, unlink};
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
            "tests/resources/environments/complex/level1.env",
            "tests/resources/environments/complex/level2.env",
            "tests/resources/environments/complex/a",
        ],
        &tempdir,
        &options,
    )
        .expect("Failed to copy test project directory");

    tempdir.into_path()
}

#[rstest]
fn test_extract_env() -> Result<()> {
    let (variables, parent) = extract_env("./tests/resources/environments/complex/level4.env")?;
    dlog!("variables: {:?}", variables);
    dlog!("parent: {:?}", parent);
    assert_eq!(variables.get("VAR_6"), Some(&"var_64".to_string()));
    // assert_eq!(parent, Some("a/level3.env".to_string()));
    Ok(())
}

#[rstest]
fn test_build_env() -> Result<()> {
    let (variables, files) = build_env("./tests/resources/environments/complex/level4.env")?;
    let reference = extract_env("./tests/resources/environments/complex/result.env")?.0;
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
    // let env_vars = build_env_vars("./tests/resources/environments/complex/level4.env")?;
    let env_vars = build_env_vars("./tests/resources/environments/parallel/test.env")?;
    println!("{}", env_vars);
    Ok(())
}

#[rstest]
fn test_print_files() -> Result<()> {
    print_files("./tests/resources/environments/complex/level4.env")?;
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
#[rstest]
fn test_unlink(temp_dir: Utf8PathBuf) -> Result<()> {
    let child = temp_dir.join("./a/level3.env");
    unlink(child.as_str())?;

    let child_content = fs::read_to_string(&child)?;
    assert!(child_content.contains("# rsenv:\n"));
    Ok(())
}
#[rstest]
fn test_link_all(temp_dir: Utf8PathBuf) -> Result<()> {
    let parent = temp_dir.join("./a/level3.env");
    let intermediate = temp_dir.join("./level2.env");
    let child = temp_dir.join("./level1.env");
    let nodes = vec![parent.as_str().to_string(), intermediate.as_str().to_string(), child.as_str().to_string()];
    link_all(&nodes);

    let child_content = fs::read_to_string(&child)?;
    assert!(child_content.contains("# rsenv: level2.env"));

    let child_content = fs::read_to_string(&intermediate)?;
    assert!(child_content.contains("# rsenv: a/level3.env"));

    let child_content = fs::read_to_string(&parent)?;
    assert!(child_content.contains("# rsenv:\n"));
    Ok(())
}
