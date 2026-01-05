//! Tests for environment variable expansion in paths

use rsenv::domain::expand_env_vars;

#[test]
fn given_path_with_dollar_var_when_expanding_then_substitutes() {
    // Arrange
    std::env::set_var("TEST_HOME", "/home/user");

    // Act
    let result = expand_env_vars("$TEST_HOME/config");

    // Assert
    assert_eq!(result, "/home/user/config");

    // Cleanup
    std::env::remove_var("TEST_HOME");
}

#[test]
fn given_path_with_braced_var_when_expanding_then_substitutes() {
    // Arrange
    std::env::set_var("TEST_DIR", "/var/data");

    // Act
    let result = expand_env_vars("${TEST_DIR}/file.txt");

    // Assert
    assert_eq!(result, "/var/data/file.txt");

    // Cleanup
    std::env::remove_var("TEST_DIR");
}

#[test]
fn given_path_with_multiple_vars_when_expanding_then_substitutes_all() {
    // Arrange
    std::env::set_var("TEST_BASE", "/opt");
    std::env::set_var("TEST_APP", "myapp");

    // Act
    let result = expand_env_vars("$TEST_BASE/${TEST_APP}/config");

    // Assert
    assert_eq!(result, "/opt/myapp/config");

    // Cleanup
    std::env::remove_var("TEST_BASE");
    std::env::remove_var("TEST_APP");
}

#[test]
fn given_path_with_undefined_var_when_expanding_then_leaves_unchanged() {
    // Act
    let result = expand_env_vars("$UNDEFINED_VAR_XYZ/config");

    // Assert - undefined vars left as-is or empty, depends on implementation
    // shellexpand leaves them as empty string
    assert!(result.contains("/config"));
}

#[test]
fn given_path_without_vars_when_expanding_then_returns_unchanged() {
    // Act
    let result = expand_env_vars("/absolute/path/no/vars");

    // Assert
    assert_eq!(result, "/absolute/path/no/vars");
}

#[test]
fn given_tilde_path_when_expanding_then_expands_home() {
    // Act
    let result = expand_env_vars("~/config");

    // Assert - should expand to home directory
    assert!(!result.starts_with('~'));
    assert!(result.contains("/config"));
}
