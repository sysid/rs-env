use std::collections::BTreeMap;
use std::{env, fs};
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Result;
use lazy_static::lazy_static;
use regex::Regex;
use rstest::{fixture, rstest};
use tempfile::tempdir;
use fs_extra::{copy_items, dir};
use tracing::debug;
use rsenv::errors::{TreeError, TreeResult};
use rsenv::{build_env, build_env_vars, extract_env, is_dag, link, link_all, print_files, unlink};
use rsenv::util::testing;

#[ctor::ctor]
fn init() {
    testing::init_test_setup();
}

#[fixture]
fn temp_dir() -> PathBuf {
    let tempdir = tempdir().unwrap();
    let options = dir::CopyOptions::new();
    copy_items(
        &[
            "tests/resources/environments/complex/level1.env",
            "tests/resources/environments/complex/level2.env",
            "tests/resources/environments/complex/a",
        ],
        tempdir.path(),
        &options,
    ).expect("Failed to copy test project directory");

    tempdir.into_path()
}

#[rstest]
fn test_extract_env() -> TreeResult<()> {
    let (variables, parent) = extract_env(Path::new("./tests/resources/environments/complex/level4.env"))?;
    debug!("variables: {:?}", variables);
    debug!("parent: {:?}", parent);
    assert_eq!(variables.get("VAR_6"), Some(&"var_64".to_string()));
    Ok(())
}

#[rstest]
fn test_build_env() -> TreeResult<()> {
    let (variables, files, is_dag) = build_env(Path::new("./tests/resources/environments/complex/level4.env"))?;
    let (reference, _) = extract_env(Path::new("./tests/resources/environments/complex/result.env"))?;

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
fn test_build_env_graph() -> TreeResult<()> {
    let (variables, files, is_dag) = build_env(Path::new("./tests/resources/environments/graph/level31.env"))?;
    let (reference, _) = extract_env(Path::new("./tests/resources/environments/graph/result.env"))?;
    println!("variables: {:#?}", variables);
    println!("files: {:#?}", files);
    println!("reference: {:#?}", reference);

    assert_eq!(variables, reference, "The two BTreeMaps are not equal!");
    assert!(is_dag);
    Ok(())
}

#[rstest]
fn test_build_env_graph2() -> TreeResult<()> {
    let (variables, files, is_dag) = build_env(Path::new("./tests/resources/environments/graph2/level21.env"))?;
    let (reference, _) = extract_env(Path::new("./tests/resources/environments/graph2/result1.env"))?;
    println!("variables: {:#?}", variables);
    println!("files: {:#?}", files);
    println!("reference: {:#?}", reference);

    assert_eq!(variables, reference, "The two BTreeMaps are not equal!");
    assert!(is_dag);

    let (variables, _, is_dag) = build_env(Path::new("./tests/resources/environments/graph2/level22.env"))?;
    let (reference, _) = extract_env(Path::new("./tests/resources/environments/graph2/result2.env"))?;
    assert_eq!(variables, reference, "The two BTreeMaps are not equal!");
    assert!(is_dag);
    Ok(())
}

#[rstest]
fn test_build_env_vars() -> TreeResult<()> {
    let env_vars = build_env_vars(Path::new("./tests/resources/environments/parallel/test.env"))?;
    println!("{}", env_vars);
    Ok(())
}

#[rstest]
fn test_build_env_vars_fail_wrong_parent() -> TreeResult<()> {
    let original_dir = env::current_dir()?;
    let result = build_env_vars(Path::new("./tests/resources/environments/graph2/error.env"));
    match result {
        Ok(_) => panic!("Expected an error, but got OK"),
        Err(e) => {
            let re = Regex::new(r"Invalid parent path: .*not-existing.env").expect("Invalid regex pattern");
            assert!(re.is_match(&e.to_string()));
        }
    }
    env::set_current_dir(original_dir)?;  // error occurs after change directory in extract_env
    Ok(())
}

#[rstest]
fn test_build_env_vars_fail() -> TreeResult<()> {
    let result = build_env_vars(Path::new("xxx"));
    match result {
        Ok(_) => panic!("Expected an error, but got OK"),
        Err(e) => {
            assert!(matches!(e, TreeError::FileNotFound(_)));
        }
    }
    Ok(())
}

#[rstest]
fn test_print_files() -> TreeResult<()> {
    print_files(Path::new("./tests/resources/environments/complex/level4.env"))?;
    Ok(())
}

#[rstest]
fn test_link(temp_dir: PathBuf) -> TreeResult<()> {
    let parent = temp_dir.join("a/level3.env");
    let child = temp_dir.join("level1.env");
    link(&parent, &child)?;

    let child_content = fs::read_to_string(&child)?;
    assert!(child_content.contains("# rsenv: a/level3.env"));
    Ok(())
}

#[rstest]
fn test_unlink(temp_dir: PathBuf) -> TreeResult<()> {
    let child = temp_dir.join("a/level3.env");
    unlink(&child)?;

    let child_content = fs::read_to_string(&child)?;
    assert!(child_content.contains("# rsenv:\n"));
    Ok(())
}

#[rstest]
fn test_link_all(temp_dir: PathBuf) -> TreeResult<()> {
    let parent = temp_dir.join("a/level3.env");
    let intermediate = temp_dir.join("level2.env");
    let child = temp_dir.join("level1.env");
    let nodes = vec![parent.clone(), intermediate.clone(), child.clone()];
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
fn test_is_dag_false() -> TreeResult<()> {
    assert!(!is_dag(Path::new("./tests/resources/environments/complex"))?);
    assert!(!is_dag(Path::new("./tests/resources/environments/parallel"))?);
    Ok(())
}

#[rstest]
fn test_is_dag_true() -> TreeResult<()> {
    assert!(is_dag(Path::new("./tests/resources/environments/graph"))?);
    Ok(())
}

#[rstest]
#[ignore = "Only for interactive exploration"]
fn test_extract_env_symlink() -> TreeResult<()> {
    let original_dir = env::current_dir()?;
    env::set_current_dir("./tests/resources/environments/complex")?;

    // 1. Create a symbolic link
    symlink("level4.env", "symlink.env")?;
    // 3. Run extract_env function
    let _ = extract_env(Path::new("./symlink.env"));
    let _ = fs::remove_file("./symlink.env");

    // Reset to the original directory
    env::set_current_dir(original_dir)?;
    Ok(())
}

#[rstest]
fn test_extract_env_symlink2() -> TreeResult<()> {
    let original_dir = env::current_dir()?;
    env::set_current_dir("./tests/resources/environments/complex")?;
    let _ = fs::remove_file("./symlink.env");
    symlink("level4.env", "symlink.env")?;
    env::set_current_dir(original_dir)?;

    // Step 2: Run the Rust binary as a subprocess
    let output = Command::new("cargo")
        .args([
            "run",
            "--",
            "build",
            "./tests/resources/environments/complex/symlink.env",
        ])
        .output()
        .expect("Failed to execute command");

    // Step 3: Check stderr for the symlink warning
    let stderr_output = String::from_utf8(output.stderr).expect("invalid utf8 string");
    println!("stderr_output: {}", stderr_output);
    assert!(stderr_output.contains("Warning: The file"));

    // Step 4: Cleanup by removing the symbolic link
    fs::remove_file("./tests/resources/environments/complex/symlink.env")?;
    Ok(())
}
