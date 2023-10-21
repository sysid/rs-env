#![allow(unused_imports)]

use std::collections::BTreeMap;
use std::{env, fs};
use std::os::unix::fs::symlink;
use std::process::Command;
use anyhow::Result;
use camino::Utf8PathBuf;
use camino_tempfile::tempdir;
use fs_extra::{copy_items, dir};
use lazy_static::lazy_static;
use rstest::{fixture, rstest};
use rsenv::{build_env, dlog, extract_env, build_env_vars, print_files, link, link_all, unlink, is_dag};
use log::{debug, info};
use regex::Regex;
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
    let (variables, files, is_dag) = build_env("./tests/resources/environments/complex/level4.env")?;
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
    assert!(!is_dag);
    Ok(())
}

#[rstest]
fn test_build_env_graph() -> Result<()> {
    let (variables, files, is_dag) = build_env("./tests/resources/environments/graph/level31.env")?;
    let reference = extract_env("./tests/resources/environments/graph/result.env")?.0;
    println!("variables: {:#?}", variables);
    println!("files: {:#?}", files);
    println!("reference: {:#?}", reference);

    assert_eq!(variables, reference, "The two BTreeMaps are not equal!");
    assert!(is_dag);
    Ok(())
}

#[rstest]
fn test_build_env_graph2() -> Result<()> {
    let (variables, files, is_dag) = build_env("./tests/resources/environments/graph2/level21.env")?;
    let reference = extract_env("./tests/resources/environments/graph2/result1.env")?.0;
    println!("variables: {:#?}", variables);
    println!("files: {:#?}", files);
    println!("reference: {:#?}", reference);

    assert_eq!(variables, reference, "The two BTreeMaps are not equal!");
    assert!(is_dag);

    let (variables, _, is_dag) = build_env("./tests/resources/environments/graph2/level22.env")?;
    let reference = extract_env("./tests/resources/environments/graph2/result2.env")?.0;
    assert_eq!(variables, reference, "The two BTreeMaps are not equal!");
    assert!(is_dag);
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
fn test_build_env_vars_fail_wrong_parent() -> Result<()> {
    let original_dir = env::current_dir()?;
    let result = build_env_vars("./tests/resources/environments/graph2/error.env");
    match result {
        Ok(_) => panic!("Expected an error, but got OK"),
        Err(e) => {
            let re = Regex::new(r"\d+: Invalid path: not-existing.env")?;
            assert!(re.is_match(&e.to_string()));
        }
    }
    env::set_current_dir(original_dir)?;  // error occurs after change directory in extract_env
    Ok(())
}

#[rstest]
fn test_build_env_vars_fail() -> Result<()> {
    let result = build_env_vars("xxx");
    match result {
        Ok(_) => panic!("Expected an error, but got OK"),
        Err(e) => {
            let re = Regex::new(r"\d+: File does not exist: xxx")?;
            assert!(re.is_match(&e.to_string()));
        }
    }
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

#[rstest]
fn test_is_dag_false() -> Result<()> {
    assert!(!is_dag("./tests/resources/environments/complex")?);
    assert!(!is_dag("./tests/resources/environments/parallel")?);
    Ok(())
}

#[rstest]
fn test_is_dag_true() -> Result<()> {
    assert!(is_dag("./tests/resources/environments/graph")?);
    Ok(())
}

#[rstest]
#[ignore = "Only for interactive exploration"]
fn test_extract_env_symlink() -> Result<()> {
    let original_dir = env::current_dir()?;
    env::set_current_dir("./tests/resources/environments/complex")?;

    // 1. Create a symbolic link
    symlink("level4.env", "symlink.env")?;

    // 3. Run extract_env function
    let _ = extract_env("./symlink.env");

    // 6. Cleanup: Remove the symlink
    let _ = fs::remove_file("./symlink.env");

    // Reset to the original directory
    env::set_current_dir(original_dir)?;

    Ok(())
}

#[rstest]
fn test_extract_env_symlink2() -> Result<()> {
    // Step 1: Create a symbolic link
    let original_dir = env::current_dir()?;
    env::set_current_dir("./tests/resources/environments/complex")?;
    _ = fs::remove_file("./symlink.env");
    symlink("level4.env", "symlink.env")?;
    env::set_current_dir(original_dir)?;

    // Step 2: Run the Rust binary as a subprocess
    let output = Command::new("cargo")
        .args(&[
            "run",
            "--",
            "build",
            "./tests/resources/environments/complex/symlink.env",
        ])
        .output()
        .expect("Failed to execute command");

    // Step 3: Check stderr for the symlink warning
    let stderr_output = String::from_utf8(output.stderr)?;
    println!("stderr_output: {}", stderr_output);
    assert!(stderr_output.contains("Warning: The file"));

    // Step 4: Cleanup by removing the symbolic link
    fs::remove_file("./tests/resources/environments/complex/symlink.env")?;

    Ok(())
}
