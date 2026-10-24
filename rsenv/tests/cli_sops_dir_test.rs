//! CLI tests for SOPS on a plain directory (`--dir`), e.g. a secrets store that is
//! its own git repo outside the rsenv base dir.
//!
//! Every test runs rsenv with HOME pointing into a temp dir, so the global config
//! (patterns, base_dir) is the one written here and the real ~/.rsenv is never touched.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

struct Env {
    _temp: TempDir,
    home: PathBuf,
    base_dir: PathBuf,
    store: PathBuf,
}

/// Global config: encrypt `*.env` and `restic.pw`; base_dir inside the temp dir.
fn env() -> Env {
    let temp = TempDir::new().unwrap();
    let home = temp.path().join("home");
    let base_dir = temp.path().join("base");
    let store = temp.path().join("store");
    for dir in [&home, &base_dir, &store] {
        fs::create_dir_all(dir).unwrap();
    }
    let config_dir = home.join("Library/Application Support/rsenv");
    fs::create_dir_all(&config_dir).unwrap();
    fs::write(
        config_dir.join("rsenv.toml"),
        format!(
            "base_dir = \"{}\"\n[sops]\nfile_extensions_enc = [\"env\"]\nfile_names_enc = [\"restic.pw\"]\n",
            base_dir.display()
        ),
    )
    .unwrap();
    Env {
        _temp: temp,
        home,
        base_dir,
        store,
    }
}

fn rsenv(env: &Env, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rsenv"))
        .args(args)
        .env("HOME", &env.home)
        .env_remove("XDG_CONFIG_HOME")
        .output()
        .expect("run rsenv")
}

fn path_arg(path: &Path) -> &str {
    path.to_str().unwrap()
}

fn managed_lines(gitignore: &Path) -> Vec<String> {
    let content = fs::read_to_string(gitignore).unwrap();
    let mut inside = false;
    let mut lines = Vec::new();
    for line in content.lines() {
        match line.trim() {
            "# rsenv-managed start" => inside = true,
            "# rsenv-managed end" => inside = false,
            l if inside && !l.starts_with('#') && !l.is_empty() => lines.push(l.to_string()),
            _ => {}
        }
    }
    lines
}

#[test]
fn given_plain_dir_when_gitignore_sync_dir_then_all_global_patterns_written_there() {
    // Arrange
    let env = env();

    // Act
    let output = rsenv(
        &env,
        &[
            "sops",
            "gitignore-sync",
            "--dir",
            path_arg(&env.store),
            "-y",
        ],
    );

    // Assert
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        managed_lines(&env.store.join(".gitignore")),
        vec!["*.env".to_string(), "restic.pw".to_string()]
    );
    assert!(
        !env.base_dir.join(".gitignore").exists(),
        "base-dir .gitignore must not be touched by --dir"
    );
}

#[test]
fn given_stale_section_when_gitignore_status_dir_then_reports_without_writing() {
    // Arrange
    let env = env();
    let stale = "# rsenv-managed start\n*.env\n# rsenv-managed end\n";
    fs::write(env.store.join(".gitignore"), stale).unwrap();

    // Act
    let output = rsenv(
        &env,
        &["sops", "gitignore-status", "--dir", path_arg(&env.store)],
    );

    // Assert
    assert!(output.status.success(), "{output:?}");
    let out = String::from_utf8_lossy(&output.stdout);
    assert!(out.contains("restic.pw"), "{out}");
    assert_eq!(
        fs::read_to_string(env.store.join(".gitignore")).unwrap(),
        stale
    );
}

#[test]
fn given_managed_section_when_gitignore_clean_dir_then_section_removed() {
    // Arrange
    let env = env();
    fs::write(
        env.store.join(".gitignore"),
        "!.run\n# rsenv-managed start\n*.env\n# rsenv-managed end\n",
    )
    .unwrap();

    // Act
    let output = rsenv(
        &env,
        &["sops", "gitignore-clean", "--dir", path_arg(&env.store)],
    );

    // Assert
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        fs::read_to_string(env.store.join(".gitignore")).unwrap(),
        "!.run"
    );
}

#[test]
fn given_dir_and_global_when_gitignore_sync_then_rejected_as_conflicting() {
    // Arrange
    let env = env();

    // Act
    let output = rsenv(
        &env,
        &[
            "sops",
            "gitignore-sync",
            "--dir",
            path_arg(&env.store),
            "--global",
            "-y",
        ],
    );

    // Assert
    assert!(!output.status.success(), "{output:?}");
    assert!(!env.store.join(".gitignore").exists());
}

// ============================================================
// `sops encrypt --dir`: protect new patterns before encrypting (opt-in)
// ============================================================

#[test]
fn given_dir_with_rsenv_section_when_encrypt_dir_then_gitignore_synced() {
    // Arrange: opted in (rsenv section exists) but stale: restic.pw missing
    let env = env();
    fs::write(
        env.store.join(".gitignore"),
        "!.run\n# rsenv-managed start\n*.env\n# rsenv-managed end\n",
    )
    .unwrap();

    // Act: nothing to encrypt, so no key is needed
    let output = rsenv(&env, &["sops", "encrypt", "--dir", path_arg(&env.store)]);

    // Assert
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        managed_lines(&env.store.join(".gitignore")),
        vec!["*.env".to_string(), "restic.pw".to_string()]
    );
}

/// The gitignore update happens BEFORE encryption, so a new secret is protected
/// even when encrypting it fails.
#[test]
fn given_encrypt_fails_when_encrypt_dir_then_gitignore_already_protects_new_secret() {
    // Arrange: plaintext present, no encryption key configured
    let env = env();
    fs::write(
        env.store.join(".gitignore"),
        "# rsenv-managed start\n*.env\n# rsenv-managed end\n",
    )
    .unwrap();
    fs::write(env.store.join("restic.pw"), "secret").unwrap();

    // Act
    let output = rsenv(&env, &["sops", "encrypt", "--dir", path_arg(&env.store)]);

    // Assert
    assert!(
        !output.status.success(),
        "no key -> encrypt fails: {output:?}"
    );
    assert!(managed_lines(&env.store.join(".gitignore")).contains(&"restic.pw".to_string()));
}

/// Without an rsenv section the directory's .gitignore is not rsenv's to manage.
#[test]
fn given_dir_without_rsenv_section_when_encrypt_dir_then_gitignore_untouched() {
    // Arrange
    let env = env();
    let with_foreign = env.store.join("foreign");
    let without_any = env.store.join("bare");
    fs::create_dir_all(&with_foreign).unwrap();
    fs::create_dir_all(&without_any).unwrap();
    fs::write(with_foreign.join(".gitignore"), "*.log\n").unwrap();

    // Act
    let foreign_out = rsenv(&env, &["sops", "encrypt", "--dir", path_arg(&with_foreign)]);
    let bare_out = rsenv(&env, &["sops", "encrypt", "--dir", path_arg(&without_any)]);

    // Assert
    assert!(foreign_out.status.success(), "{foreign_out:?}");
    assert!(bare_out.status.success(), "{bare_out:?}");
    assert_eq!(
        fs::read_to_string(with_foreign.join(".gitignore")).unwrap(),
        "*.log\n"
    );
    assert!(!without_any.join(".gitignore").exists());
}
