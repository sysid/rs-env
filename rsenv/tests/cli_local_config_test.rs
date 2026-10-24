//! CLI tests for where the local config layer comes from.
//!
//! `.rsenv.toml` is vault config: it is read from the vault discovered through the
//! project's `.envrc` symlink, never from a plain (non-vault) working directory.
//! Otherwise the same `rsenv sops ... --dir X` would behave differently depending
//! on the directory it is run from.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use tempfile::TempDir;

const SENTINEL_EDITOR: &str = "local-config-sentinel-editor";

fn rsenv(project_dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rsenv"))
        .arg("-C")
        .arg(project_dir)
        .args(args)
        .env_remove("RSENV__EDITOR")
        .output()
        .expect("run rsenv")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn given_plain_dir_with_rsenv_toml_when_config_show_then_file_is_ignored() {
    // Arrange: a directory that is NOT a vault project, with a .rsenv.toml in it
    let temp = TempDir::new().unwrap();
    let plain_dir = temp.path().join("plain");
    fs::create_dir_all(&plain_dir).unwrap();
    fs::write(
        plain_dir.join(".rsenv.toml"),
        format!("editor = \"{SENTINEL_EDITOR}\"\n"),
    )
    .unwrap();

    // Act
    let output = rsenv(&plain_dir, &["config", "show"]);

    // Assert
    assert!(output.status.success(), "{output:?}");
    assert!(
        !stdout(&output).contains(SENTINEL_EDITOR),
        "a non-vault .rsenv.toml must not be applied:\n{}",
        stdout(&output)
    );
}

#[test]
fn given_vault_project_with_rsenv_toml_when_config_show_then_vault_file_is_applied() {
    // Arrange: hand-built vault (dot.envrc + .rsenv.toml) linked from the project's .envrc
    let temp = TempDir::new().unwrap();
    let vault_dir = temp.path().join("vaults").join("proj-1234abcd");
    let project_dir = temp.path().join("proj");
    fs::create_dir_all(&vault_dir).unwrap();
    fs::create_dir_all(&project_dir).unwrap();
    fs::write(vault_dir.join("dot.envrc"), "").unwrap();
    fs::write(
        vault_dir.join(".rsenv.toml"),
        format!("editor = \"{SENTINEL_EDITOR}\"\n"),
    )
    .unwrap();
    std::os::unix::fs::symlink(vault_dir.join("dot.envrc"), project_dir.join(".envrc")).unwrap();

    // Act
    let output = rsenv(&project_dir, &["config", "show"]);

    // Assert
    assert!(output.status.success(), "{output:?}");
    assert!(
        stdout(&output).contains(SENTINEL_EDITOR),
        "the vault's .rsenv.toml must be applied:\n{}",
        stdout(&output)
    );
}

#[test]
fn given_plain_dir_when_local_config_init_then_fails_and_writes_nothing() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let plain_dir = temp.path().join("plain");
    fs::create_dir_all(&plain_dir).unwrap();

    // Act
    let output = rsenv(&plain_dir, &["config", "init"]);

    // Assert
    assert!(
        !output.status.success(),
        "config init outside a vault must fail: {output:?}"
    );
    assert!(
        !plain_dir.join(".rsenv.toml").exists(),
        "a .rsenv.toml outside a vault would never be read"
    );
}
