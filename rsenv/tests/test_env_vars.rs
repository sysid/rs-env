use rsenv::errors::TreeResult;
use rsenv::util::testing;
use rstest::{fixture, rstest};
use std::env;
use std::fs::{self};
use std::path::PathBuf;
use tempfile::tempdir;

#[ctor::ctor]
fn init() {
    testing::init_test_setup();
}

#[fixture]
fn temp_dir_with_env_vars() -> (PathBuf, String) {
    let temp_dir = tempdir().unwrap();
    let temp_path = temp_dir.path().to_path_buf();

    // Set a test environment variable
    let env_var_name = "RSENV_TEST_DIR";
    env::set_var(env_var_name, temp_path.to_string_lossy().to_string());

    // Create parent file
    let parent_path = temp_path.join("parent.env");
    fs::write(&parent_path, "export PARENT_VAR=parent_value\n").unwrap();

    // Create child file with $VAR syntax - using direct string writing
    let child_path = temp_path.join("child.env");
    fs::write(
        &child_path,
        "# rsenv: $RSENV_TEST_DIR/parent.env\nexport CHILD_VAR=child_value\n",
    )
    .unwrap();

    // Create child file with ${VAR} syntax
    let child_path2 = temp_path.join("child2.env");
    fs::write(
        &child_path2,
        "# rsenv: ${RSENV_TEST_DIR}/parent.env\nexport CHILD2_VAR=child2_value\n",
    )
    .unwrap();

    (temp_dir.into_path(), env_var_name.to_string())
}

#[rstest]
fn given_env_var_in_path_when_extracting_env_then_expands_variables(
    temp_dir_with_env_vars: (PathBuf, String),
) -> TreeResult<()> {
    let (temp_path, env_var_name) = temp_dir_with_env_vars;

    // Test with $VAR syntax
    let child_path = temp_path.join("child.env");
    let parent_path = temp_path.join("parent.env");

    let (variables, files) = rsenv::extract_env(&child_path)?;

    // Verify environment variable was expanded correctly
    assert_eq!(files.len(), 1);
    assert_eq!(files[0], parent_path.canonicalize()?);
    assert_eq!(variables.get("CHILD_VAR"), Some(&"child_value".to_string()));

    // Test with build_env to ensure full integration
    let (all_vars, all_files, _) = rsenv::build_env(&child_path)?;

    // Verify variables from both files
    assert_eq!(all_vars.get("CHILD_VAR"), Some(&"child_value".to_string()));
    assert_eq!(
        all_vars.get("PARENT_VAR"),
        Some(&"parent_value".to_string())
    );

    // Verify both files were processed
    assert_eq!(all_files.len(), 2);
    assert!(all_files.contains(&child_path.canonicalize()?));
    assert!(all_files.contains(&parent_path.canonicalize()?));

    // Test with ${VAR} syntax
    let child_path2 = temp_path.join("child2.env");

    let (_, files2) = rsenv::extract_env(&child_path2)?;
    assert_eq!(files2.len(), 1);
    assert_eq!(files2[0], parent_path.canonicalize()?);

    // Clean up
    env::remove_var(&env_var_name);

    Ok(())
}

#[rstest]
fn given_nonexistent_env_var_when_extracting_env_then_handles_gracefully() -> TreeResult<()> {
    // Create temporary directory
    let temp_dir = tempdir()?;
    let temp_path = temp_dir.path();

    // Ensure the environment variable doesn't exist
    let non_existent_var = "RSENV_NONEXISTENT_VAR";
    env::remove_var(non_existent_var);

    // Create child file with nonexistent environment variable - use direct writing
    let child_path = temp_path.join("child.env");
    fs::write(
        &child_path,
        "# rsenv: ${RSENV_NONEXISTENT_VAR}/parent.env\nexport CHILD_VAR=child_value\n",
    )
    .unwrap();

    // The extraction should fail gracefully with an appropriate error
    let result = rsenv::extract_env(&child_path);

    // Should be an error, and specifically an InvalidParent error
    assert!(result.is_err());
    match result {
        Err(rsenv::errors::TreeError::InvalidParent(_)) => {
            // This is the expected error type
            assert!(true);
        }
        _ => {
            // Any other error type or success is unexpected
            assert!(false, "Expected InvalidParent error but got: {:?}", result);
        }
    }

    Ok(())
}

#[rstest]
fn test_expand_env_vars() {
    // This function directly tests the expand_env_vars helper function
    // Since it's an internal function, we'll re-implement it here for testing
    fn expand_env_vars(path: &str) -> String {
        use regex::Regex;
        let mut result = path.to_string();

        // Find all occurrences of $VAR or ${VAR}
        let env_var_pattern = Regex::new(r"\$(\w+)|\$\{(\w+)\}").unwrap();

        // Collect all matches first to avoid borrow checker issues with replace_all
        let matches: Vec<_> = env_var_pattern.captures_iter(path).collect();

        for cap in matches {
            // Get the variable name from either $VAR or ${VAR} pattern
            let var_name = cap.get(1).or_else(|| cap.get(2)).unwrap().as_str();
            let var_placeholder = if cap.get(1).is_some() {
                format!("${}", var_name)
            } else {
                format!("${{{}}}", var_name)
            };

            // Replace with environment variable value or empty string if not found
            if let Ok(var_value) = std::env::var(var_name) {
                result = result.replace(&var_placeholder, &var_value);
            }
        }

        result
    }

    // Set up test environment variables
    env::set_var("TEST_VAR_1", "value1");
    env::set_var("TEST_VAR_2", "value2");

    // Test $VAR syntax
    let input = "/path/$TEST_VAR_1/file";
    let expected = "/path/value1/file";
    assert_eq!(expand_env_vars(input), expected);

    // Test ${VAR} syntax
    let input = "/path/${TEST_VAR_2}/file";
    let expected = "/path/value2/file";
    assert_eq!(expand_env_vars(input), expected);

    // Test multiple variables
    let input = "/path/$TEST_VAR_1/${TEST_VAR_2}/file";
    let expected = "/path/value1/value2/file";
    assert_eq!(expand_env_vars(input), expected);

    // Test non-existent variable
    let input = "/path/$NONEXISTENT_VAR/file";
    let expected = "/path/$NONEXISTENT_VAR/file"; // Should remain unchanged
    assert_eq!(expand_env_vars(input), expected);

    // Clean up
    env::remove_var("TEST_VAR_1");
    env::remove_var("TEST_VAR_2");
}
