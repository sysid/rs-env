//! Tests for EnvFile parsing

use std::path::PathBuf;

use rsenv::domain::{shell_quote, EnvFile};

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

#[test]
fn given_value_with_trailing_comment_when_parsing_then_comment_stripped() {
    // Arrange - quoted value with trailing comment
    let content = "export REDIS_PASSWORD='u7i#G!Z^^zCg75VxfnBxv8u7Mkjg'  # e2e\n";

    // Act
    let env_file = EnvFile::parse(content, PathBuf::from("/project/local.env")).unwrap();

    // Assert - comment should be stripped, quotes should be stripped
    assert_eq!(
        env_file.variables.get("REDIS_PASSWORD"),
        Some(&"u7i#G!Z^^zCg75VxfnBxv8u7Mkjg".to_string())
    );
}

#[test]
fn given_value_with_hash_inside_quotes_when_parsing_then_hash_preserved() {
    // Arrange - hash inside quotes is NOT a comment
    let content = "export PASSWORD='pass#word'\n";

    // Act
    let env_file = EnvFile::parse(content, PathBuf::from("/project/local.env")).unwrap();

    // Assert - hash inside quotes must be preserved
    assert_eq!(
        env_file.variables.get("PASSWORD"),
        Some(&"pass#word".to_string())
    );
}

#[test]
fn given_double_quoted_value_with_trailing_comment_when_parsing_then_comment_stripped() {
    // Arrange - double-quoted value with trailing comment
    let content = r#"export API_KEY="sk-secret-123"  # production key"#;

    // Act
    let env_file = EnvFile::parse(content, PathBuf::from("/project/local.env")).unwrap();

    // Assert
    assert_eq!(
        env_file.variables.get("API_KEY"),
        Some(&"sk-secret-123".to_string())
    );
}

#[test]
fn given_unquoted_value_with_trailing_comment_when_parsing_then_comment_stripped() {
    // Arrange - unquoted value with trailing comment
    let content = "export PORT=8080  # default port\n";

    // Act
    let env_file = EnvFile::parse(content, PathBuf::from("/project/local.env")).unwrap();

    // Assert
    assert_eq!(env_file.variables.get("PORT"), Some(&"8080".to_string()));
}

// --- shell_quote tests (expansion mode: literal = false) ---
// literal = false reproduces the legacy double-quote behavior used for values
// that were double-quoted or unquoted in the source (shell expansion preserved).

#[test]
fn given_value_with_spaces_when_shell_quoting_then_adds_quotes() {
    assert_eq!(
        shell_quote("--reverse --height 100%", false),
        "\"--reverse --height 100%\""
    );
}

#[test]
fn given_simple_value_when_shell_quoting_then_no_quotes() {
    assert_eq!(shell_quote("simple", false), "simple");
}

#[test]
fn given_empty_value_when_shell_quoting_then_adds_quotes() {
    assert_eq!(shell_quote("", false), "\"\"");
}

#[test]
fn given_value_with_dollar_when_shell_quoting_then_preserves_for_expansion() {
    // Non-literal (double-quoted/unquoted source) keeps $ unescaped for expansion.
    assert_eq!(shell_quote("$HOME/bin", false), "\"$HOME/bin\"");
}

#[test]
fn given_value_with_embedded_quote_when_shell_quoting_then_wraps() {
    assert_eq!(shell_quote("say \"hello\"", false), "\"say \"hello\"\"");
}

#[test]
fn given_value_with_backtick_when_shell_quoting_then_wraps() {
    assert_eq!(shell_quote("echo `date`", false), "\"echo `date`\"");
}

#[test]
fn given_value_with_single_quote_when_shell_quoting_then_adds_quotes() {
    assert_eq!(shell_quote("it's", false), "\"it's\"");
}

#[test]
fn given_value_with_backslash_when_shell_quoting_then_adds_quotes() {
    assert_eq!(shell_quote("path\\to", false), "\"path\\to\"");
}

#[test]
fn given_value_with_semicolon_when_shell_quoting_then_adds_quotes() {
    assert_eq!(shell_quote("cmd;cmd2", false), "\"cmd;cmd2\"");
}

#[test]
fn given_value_with_pipe_when_shell_quoting_then_adds_quotes() {
    assert_eq!(shell_quote("a|b", false), "\"a|b\"");
}

#[test]
fn given_value_with_ampersand_when_shell_quoting_then_adds_quotes() {
    assert_eq!(shell_quote("a&b", false), "\"a&b\"");
}

#[test]
fn given_value_with_parentheses_when_shell_quoting_then_adds_quotes() {
    assert_eq!(shell_quote("(group)", false), "\"(group)\"");
}

#[test]
fn given_value_with_angle_brackets_when_shell_quoting_then_adds_quotes() {
    assert_eq!(shell_quote("a<b>c", false), "\"a<b>c\"");
}

#[test]
fn given_value_with_tab_when_shell_quoting_then_adds_quotes() {
    assert_eq!(shell_quote("a\tb", false), "\"a\tb\"");
}

#[test]
fn given_flag_without_spaces_when_shell_quoting_then_no_quotes() {
    assert_eq!(shell_quote("--verbose", false), "--verbose");
}

#[test]
fn given_numeric_value_when_shell_quoting_then_no_quotes() {
    assert_eq!(shell_quote("12345", false), "12345");
}

#[test]
fn given_path_without_special_chars_when_shell_quoting_then_no_quotes() {
    assert_eq!(shell_quote("/usr/local/bin", false), "/usr/local/bin");
}

#[test]
fn given_alphanumeric_with_hyphens_when_shell_quoting_then_no_quotes() {
    assert_eq!(shell_quote("my-app-v2.0", false), "my-app-v2.0");
}

// --- shell_quote tests (literal mode: literal = true) ---
// literal = true is used for values that were single-quoted in the source.
// Output must be a POSIX single-quoted literal so the shell performs NO
// expansion or command substitution when the .envrc is sourced.

#[test]
fn given_literal_value_with_dollar_when_shell_quoting_then_single_quotes_no_expansion() {
    // Regression: a password containing $ must survive sourcing verbatim.
    assert_eq!(
        shell_quote("0$s7-e|ZKi~dMz3eYqH_6UFE(N-.", true),
        "'0$s7-e|ZKi~dMz3eYqH_6UFE(N-.'"
    );
}

#[test]
fn given_literal_value_with_backtick_when_shell_quoting_then_single_quotes_no_substitution() {
    // Backticks inside single quotes are literal - no command substitution.
    assert_eq!(shell_quote("echo `date`", true), "'echo `date`'");
}

#[test]
fn given_literal_value_with_embedded_single_quote_when_shell_quoting_then_escapes() {
    // POSIX idiom: close quote, escaped quote, reopen quote.
    assert_eq!(shell_quote("it's", true), "'it'\\''s'");
}

#[test]
fn given_literal_empty_value_when_shell_quoting_then_single_quotes() {
    assert_eq!(shell_quote("", true), "''");
}

#[test]
fn given_literal_simple_value_when_shell_quoting_then_no_quotes() {
    // No special characters - bare output is already literal.
    assert_eq!(shell_quote("simple", true), "simple");
}

// --- parse quote-style intent tests ---

#[test]
fn given_single_quoted_value_when_parsing_then_marks_key_literal() {
    let content = "export PW='a$b'\n";
    let env_file = EnvFile::parse(content, PathBuf::from("/project/local.env")).unwrap();
    assert_eq!(env_file.variables.get("PW"), Some(&"a$b".to_string()));
    assert!(env_file.literal_keys.contains("PW"));
}

#[test]
fn given_double_quoted_value_when_parsing_then_key_not_literal() {
    let content = "export P=\"$HOME/bin\"\n";
    let env_file = EnvFile::parse(content, PathBuf::from("/project/local.env")).unwrap();
    assert!(!env_file.literal_keys.contains("P"));
}

#[test]
fn given_unquoted_value_when_parsing_then_key_not_literal() {
    let content = "export P=plain\n";
    let env_file = EnvFile::parse(content, PathBuf::from("/project/local.env")).unwrap();
    assert!(!env_file.literal_keys.contains("P"));
}
