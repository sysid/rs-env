//! CLI wiring and output wording: the three rsenv-managed `.gitignore` files must
//! never be reported as "global gitignore", which reads as git's core.excludesFile.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

use clap::CommandFactory;
use rsenv::cli::Cli;
use tempfile::TempDir;

#[test]
fn given_cli_definition_when_debug_asserted_then_valid() {
    Cli::command().debug_assert();
}

struct Home {
    _temp: TempDir,
    home: PathBuf,
    base_dir: PathBuf,
}

/// HOME in a temp dir with a global config whose base_dir is also in there.
fn home() -> Home {
    let temp = TempDir::new().unwrap();
    let home = temp.path().join("home");
    let base_dir = temp.path().join("base");
    fs::create_dir_all(&base_dir).unwrap();
    let config_dir = home.join("Library/Application Support/rsenv");
    fs::create_dir_all(&config_dir).unwrap();
    fs::write(
        config_dir.join("rsenv.toml"),
        format!(
            "base_dir = \"{}\"\n[sops]\nfile_extensions_enc = [\"env\"]\n",
            base_dir.display()
        ),
    )
    .unwrap();
    Home {
        _temp: temp,
        home,
        base_dir,
    }
}

fn rsenv(home: &Home, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rsenv"))
        .args(args)
        .env("HOME", &home.home)
        .env_remove("XDG_CONFIG_HOME")
        .output()
        .expect("run rsenv")
}

#[test]
fn given_base_dir_gitignore_in_sync_when_gitignore_sync_global_then_message_names_file() {
    // Arrange
    let home = home();
    let first = rsenv(&home, &["sops", "gitignore-sync", "--global", "-y"]);
    assert!(first.status.success(), "{first:?}");

    // Act
    let output = rsenv(&home, &["sops", "gitignore-sync", "--global", "-y"]);

    // Assert
    let stdout = String::from_utf8_lossy(&output.stdout);
    let gitignore = home.base_dir.join(".gitignore");
    assert!(output.status.success(), "{output:?}");
    assert!(
        stdout.contains("Base-dir .gitignore"),
        "must not say 'global gitignore': {stdout}"
    );
    assert!(
        stdout.contains(&gitignore.display().to_string()),
        "must name the file: {stdout}"
    );
    assert!(!stdout.contains("Global gitignore"), "{stdout}");
}

#[test]
fn given_base_dir_gitignore_when_gitignore_status_global_then_header_names_base_dir() {
    // Arrange
    let home = home();

    // Act
    let output = rsenv(&home, &["sops", "gitignore-status", "--global"]);

    // Assert
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{output:?}");
    assert!(stdout.contains("Base-dir .gitignore"), "{stdout}");
    assert!(!stdout.contains("Global Gitignore"), "{stdout}");
}
