//! Tests for EnvFile parsing

use std::path::PathBuf;

use rsenv::domain::EnvFile;

#[test]
fn given_env_file_with_rsenv_directive_when_parsing_then_extracts_parent() {
    // Arrange - v1 format uses export prefix
    let content = r#"# rsenv: base.env
export FOO=bar
export BAZ=qux
"#;

    // Act
    let env_file = EnvFile::parse(content, PathBuf::from("/project/local.env")).unwrap();

    // Assert
    assert_eq!(env_file.parents, vec![PathBuf::from("/project/base.env")]);
    assert_eq!(env_file.variables.get("FOO"), Some(&"bar".to_string()));
    assert_eq!(env_file.variables.get("BAZ"), Some(&"qux".to_string()));
}

#[test]
fn given_env_file_with_multiple_parents_when_parsing_then_extracts_all() {
    // Arrange - v1 format: space-separated, not comma
    let content = r#"# rsenv: base.env common.env
export FOO=bar
"#;

    // Act
    let env_file = EnvFile::parse(content, PathBuf::from("/project/local.env")).unwrap();

    // Assert
    assert_eq!(
        env_file.parents,
        vec![
            PathBuf::from("/project/base.env"),
            PathBuf::from("/project/common.env")
        ]
    );
}

#[test]
fn given_env_file_without_rsenv_directive_when_parsing_then_has_no_parents() {
    // Arrange - v1 format uses export prefix
    let content = r#"export FOO=bar
export BAZ=qux
"#;

    // Act
    let env_file = EnvFile::parse(content, PathBuf::from("/project/local.env")).unwrap();

    // Assert
    assert!(env_file.parents.is_empty());
    assert_eq!(env_file.variables.len(), 2);
}

#[test]
fn given_env_file_with_comments_when_parsing_then_ignores_comments() {
    // Arrange - v1 format uses export prefix
    let content = r#"# This is a comment
export FOO=bar
# Another comment
export BAZ=qux
"#;

    // Act
    let env_file = EnvFile::parse(content, PathBuf::from("/project/local.env")).unwrap();

    // Assert
    assert!(env_file.parents.is_empty());
    assert_eq!(env_file.variables.len(), 2);
}

#[test]
fn given_env_file_with_quoted_values_when_parsing_then_strips_quotes() {
    // Arrange - v1 format uses export prefix
    let content = r#"export FOO="bar baz"
export SINGLE='hello world'
"#;

    // Act
    let env_file = EnvFile::parse(content, PathBuf::from("/project/local.env")).unwrap();

    // Assert
    assert_eq!(env_file.variables.get("FOO"), Some(&"bar baz".to_string()));
    assert_eq!(
        env_file.variables.get("SINGLE"),
        Some(&"hello world".to_string())
    );
}

#[test]
fn given_env_file_with_empty_lines_when_parsing_then_ignores_them() {
    // Arrange - v1 format uses export prefix
    let content = r#"export FOO=bar

export BAZ=qux

"#;

    // Act
    let env_file = EnvFile::parse(content, PathBuf::from("/project/local.env")).unwrap();

    // Assert
    assert_eq!(env_file.variables.len(), 2);
}

#[test]
fn given_env_file_with_absolute_parent_path_when_parsing_then_keeps_absolute() {
    // Arrange - v1 format uses export prefix
    let content = r#"# rsenv: /etc/base.env
export FOO=bar
"#;

    // Act
    let env_file = EnvFile::parse(content, PathBuf::from("/project/local.env")).unwrap();

    // Assert
    assert_eq!(env_file.parents, vec![PathBuf::from("/etc/base.env")]);
}

#[test]
fn given_env_file_with_space_separated_parents_when_parsing_then_extracts_all() {
    // Arrange - v1 format uses spaces, not commas
    let content = r#"# rsenv: base.env common.env
export FOO=bar
"#;

    // Act
    let env_file = EnvFile::parse(content, PathBuf::from("/project/local.env")).unwrap();

    // Assert
    assert_eq!(
        env_file.parents,
        vec![
            PathBuf::from("/project/base.env"),
            PathBuf::from("/project/common.env")
        ]
    );
}

#[test]
fn given_env_file_with_flexible_whitespace_when_parsing_then_handles_all() {
    // Arrange - v1 supports various whitespace after colon
    let temp = tempfile::TempDir::new().unwrap();
    let parent = temp.path().join("parent.env");
    std::fs::write(&parent, "export PARENT=value\n").unwrap();

    let test_cases = vec![
        ("# rsenv:parent.env", "no space"),
        ("# rsenv: parent.env", "one space"),
        ("# rsenv:  parent.env", "two spaces"),
        ("# rsenv:\tparent.env", "tab"),
    ];

    for (directive, desc) in test_cases {
        let content = format!("{}\nexport CHILD=value\n", directive);
        let child_path = temp
            .path()
            .join(format!("child_{}.env", desc.replace(" ", "_")));

        let env_file = EnvFile::parse(&content, child_path).unwrap();

        assert_eq!(env_file.parents.len(), 1, "Failed for case: {}", desc);
    }
}

#[test]
fn given_env_file_with_non_export_variables_when_parsing_then_ignores_them() {
    // Arrange - v1 only parses "export VAR=value", ignores plain "VAR=value"
    let content = r#"# This is a comment
export EXPORTED=should_include
NOT_EXPORTED=should_ignore
ALSO_IGNORED=value
export ANOTHER=also_include
"#;

    // Act
    let env_file = EnvFile::parse(content, PathBuf::from("/project/local.env")).unwrap();

    // Assert - only export lines
    assert_eq!(env_file.variables.len(), 2);
    assert_eq!(
        env_file.variables.get("EXPORTED"),
        Some(&"should_include".to_string())
    );
    assert_eq!(
        env_file.variables.get("ANOTHER"),
        Some(&"also_include".to_string())
    );
    assert!(env_file.variables.get("NOT_EXPORTED").is_none());
    assert!(env_file.variables.get("ALSO_IGNORED").is_none());
}
