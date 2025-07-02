use std::collections::BTreeMap;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::{env, fs};

use fs_extra::{copy_items, dir};
use regex::Regex;
use rsenv::errors::{TreeError, TreeResult};
use rsenv::util::testing;
use rsenv::{build_env, build_env_vars, extract_env, is_dag, link, link_all, print_files, unlink};
use rstest::{fixture, rstest};
use tempfile::tempdir;
use tracing::debug;

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
    )
    .expect("Failed to copy test project directory");

    tempdir.into_path()
}

#[rstest]
fn given_env_file_when_extracting_env_then_returns_correct_variables_and_parent() -> TreeResult<()>
{
    let (variables, parent) = extract_env(Path::new(
        "./tests/resources/environments/complex/level4.env",
    ))?;
    debug!("variables: {:?}", variables);
    debug!("parent: {:?}", parent);
    assert_eq!(variables.get("VAR_6"), Some(&"var_64".to_string()));
    Ok(())
}

#[rstest]
fn given_env_file_when_building_env_then_returns_correct_variables_and_files() -> TreeResult<()> {
    let (variables, files, is_dag) = build_env(Path::new(
        "./tests/resources/environments/complex/level4.env",
    ))?;
    let (reference, _) = extract_env(Path::new(
        "./tests/resources/environments/complex/result.env",
    ))?;

    let filtered_map: BTreeMap<_, _> = variables
        .iter()
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
fn given_graph_structure_when_building_env_then_returns_correct_dag_variables() -> TreeResult<()> {
    let (variables, files, is_dag) = build_env(Path::new(
        "./tests/resources/environments/graph/level31.env",
    ))?;
    let (reference, _) = extract_env(Path::new("./tests/resources/environments/graph/result.env"))?;
    println!("variables: {:#?}", variables);
    println!("files: {:#?}", files);
    println!("reference: {:#?}", reference);

    assert_eq!(variables, reference, "The two BTreeMaps are not equal!");
    assert!(is_dag);
    Ok(())
}

#[rstest]
fn given_complex_graph_when_building_env_then_returns_correct_dag_variables() -> TreeResult<()> {
    let (variables, files, is_dag) = build_env(Path::new(
        "./tests/resources/environments/graph2/level21.env",
    ))?;
    let (reference, _) = extract_env(Path::new(
        "./tests/resources/environments/graph2/result1.env",
    ))?;
    println!("variables: {:#?}", variables);
    println!("files: {:#?}", files);
    println!("reference: {:#?}", reference);

    assert_eq!(variables, reference, "The two BTreeMaps are not equal!");
    assert!(is_dag);

    let (variables, _, is_dag) = build_env(Path::new(
        "./tests/resources/environments/graph2/level22.env",
    ))?;
    let (reference, _) = extract_env(Path::new(
        "./tests/resources/environments/graph2/result2.env",
    ))?;
    assert_eq!(variables, reference, "The two BTreeMaps are not equal!");
    assert!(is_dag);
    Ok(())
}

#[rstest]
fn given_valid_env_file_when_building_vars_then_returns_correct_env_string() -> TreeResult<()> {
    let env_vars = build_env_vars(Path::new(
        "./tests/resources/environments/parallel/test.env",
    ))?;
    println!("{}", env_vars);
    let expected = "export a_var1=a_test_var1
export a_var2=a_test_var2
export b_var1=b_test_var1
export b_var2=b_test_var2
export var1=test_var1
export var2=test_var2\n";
    assert_eq!(env_vars, expected);
    Ok(())
}

#[rstest]
fn given_invalid_parent_when_building_env_vars_then_returns_error() -> TreeResult<()> {
    let original_dir = env::current_dir()?;
    let result = build_env_vars(Path::new("./tests/resources/environments/graph2/error.env"));
    match result {
        Ok(_) => panic!("Expected an error, but got OK"),
        Err(e) => {
            let re = Regex::new(r"Invalid parent path: .*not-existing.env")
                .expect("Invalid regex pattern");
            assert!(re.is_match(&e.to_string()));
        }
    }
    env::set_current_dir(original_dir)?; // error occurs after change directory in extract_env
    Ok(())
}

#[rstest]
fn given_nonexistent_file_when_building_env_vars_then_returns_error() -> TreeResult<()> {
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
    print_files(Path::new(
        "./tests/resources/environments/complex/level4.env",
    ))?;
    Ok(())
}

#[rstest]
fn given_parent_child_files_when_linking_then_creates_correct_relationship(
    temp_dir: PathBuf,
) -> TreeResult<()> {
    let parent = temp_dir.join("a/level3.env");
    let child = temp_dir.join("level1.env");
    link(&parent, &child)?;

    let child_content = fs::read_to_string(&child)?;
    assert!(child_content.contains("# rsenv: a/level3.env"));
    Ok(())
}

#[rstest]
fn given_linked_file_when_unlinking_then_removes_relationship(temp_dir: PathBuf) -> TreeResult<()> {
    let child = temp_dir.join("a/level3.env");
    unlink(&child)?;

    let child_content = fs::read_to_string(&child)?;
    assert!(child_content.contains("# rsenv:\n"));
    Ok(())
}

#[rstest]
fn given_multiple_files_when_linking_all_then_creates_correct_hierarchy(
    temp_dir: PathBuf,
) -> TreeResult<()> {
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
fn given_tree_structure_when_checking_dag_then_returns_false() -> TreeResult<()> {
    assert!(!is_dag(Path::new(
        "./tests/resources/environments/complex"
    ))?);
    assert!(!is_dag(Path::new(
        "./tests/resources/environments/parallel"
    ))?);
    Ok(())
}

#[rstest]
fn given_graph_structure_when_checking_dag_then_returns_true() -> TreeResult<()> {
    assert!(is_dag(Path::new("./tests/resources/environments/graph"))?);
    Ok(())
}

#[rstest]
#[ignore = "Only for interactive exploration"]
fn given_symlinked_file_when_extracting_env_then_handles_symlink_correctly() -> TreeResult<()> {
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
fn given_symlinked_file_when_extracting_env_then_outputs_warning() -> TreeResult<()> {
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

#[rstest]
fn given_rsenv_comment_with_flexible_spacing_when_extracting_env_then_parses_correctly(
) -> TreeResult<()> {
    let tempdir = tempdir()?;
    let temp_path = tempdir.path();

    // Create parent file
    let parent_file = temp_path.join("parent.env");
    fs::write(&parent_file, "export PARENT_VAR=parent_value\n")?;

    // Test different spacing patterns
    let test_cases = vec![
        ("# rsenv:parent.env", "no space after colon"),
        ("# rsenv: parent.env", "one space after colon"),
        ("# rsenv:  parent.env", "two spaces after colon"),
        ("# rsenv:   parent.env", "three spaces after colon"),
        ("# rsenv:\tparent.env", "tab after colon"),
        ("# rsenv: \tparent.env", "space and tab after colon"),
    ];

    for (rsenv_comment, description) in test_cases {
        let child_file = temp_path.join(format!("child_{}.env", description.replace(" ", "_")));
        let content = format!("{}\nexport CHILD_VAR=child_value\n", rsenv_comment);
        fs::write(&child_file, content)?;

        let (variables, parents) = extract_env(&child_file)?;

        // Verify that the parent was parsed correctly
        assert_eq!(
            parents.len(),
            1,
            "Failed to parse parent for case: {}",
            description
        );
        assert_eq!(
            parents[0].canonicalize()?,
            parent_file.canonicalize()?,
            "Wrong parent path for case: {}",
            description
        );

        // Verify that variables are correct
        assert_eq!(variables.get("CHILD_VAR"), Some(&"child_value".to_string()));

        // Test build_env to ensure full integration works
        let (env_vars, files, _) = build_env(&child_file)?;
        assert_eq!(
            env_vars.get("PARENT_VAR"),
            Some(&"parent_value".to_string())
        );
        assert_eq!(env_vars.get("CHILD_VAR"), Some(&"child_value".to_string()));
        assert_eq!(files.len(), 2);
    }

    Ok(())
}
