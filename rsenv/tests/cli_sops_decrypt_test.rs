//! CLI tests for `rsenv sops decrypt --dir`: a run never stops at one file and
//! lists what it did not decrypt at the end.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use tempfile::TempDir;

fn rsenv(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rsenv"))
        .args(args)
        .output()
        .expect("run rsenv")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn enc_name(name: &str, content: &str) -> String {
    format!(
        "{name}.{}.enc",
        rsenv::application::hash::content_hash(content.as_bytes())
    )
}

fn dir_arg(dir: &Path) -> &str {
    dir.to_str().unwrap()
}

#[test]
fn given_differing_plaintext_when_decrypt_dir_then_succeeds_and_lists_kept_file() {
    // Arrange: plaintext edited after its last encrypt
    let temp = TempDir::new().unwrap();
    let dir = temp.path();
    fs::write(dir.join("app.env"), "KEY=edited").unwrap();
    let enc = dir.join(enc_name("app.env", "KEY=1"));
    fs::write(&enc, "ciphertext").unwrap();

    // Act
    let output = rsenv(&["sops", "decrypt", "--dir", dir_arg(dir)]);

    // Assert
    assert!(output.status.success(), "{output:?}");
    let out = stdout(&output);
    assert!(
        out.contains("Not decrypted, existing plaintext differs"),
        "{out}"
    );
    assert!(out.contains(enc.to_str().unwrap()), "{out}");
    assert_eq!(
        fs::read_to_string(dir.join("app.env")).unwrap(),
        "KEY=edited"
    );
}

/// Uses the real `sops`: garbage ciphertext fails at parsing, before any key is needed.
#[test]
fn given_sops_failure_when_decrypt_dir_then_run_completes_lists_failure_and_exits_nonzero() {
    // Arrange: one corrupt ciphertext without plaintext, one kept file
    let temp = TempDir::new().unwrap();
    let dir = temp.path();
    let corrupt = dir.join(enc_name("corrupt.env", "X=1"));
    fs::write(&corrupt, "not a sops file").unwrap();
    fs::write(dir.join("app.env"), "KEY=edited").unwrap();
    let kept = dir.join(enc_name("app.env", "KEY=1"));
    fs::write(&kept, "ciphertext").unwrap();

    // Act
    let output = rsenv(&["sops", "decrypt", "--dir", dir_arg(dir)]);

    // Assert: both files reported, failure listed after the kept file, exit 70 (SOFTWARE)
    assert_eq!(output.status.code(), Some(70), "{output:?}");
    let out = stdout(&output);
    let kept_at = out.find(kept.to_str().unwrap()).expect("kept file listed");
    let failed_at = out
        .find(corrupt.to_str().unwrap())
        .expect("failed file listed");
    assert!(kept_at < failed_at, "failures are listed last:\n{out}");
    assert!(!dir.join("corrupt.env").exists());
}

#[test]
fn given_differing_plaintext_when_decrypt_single_file_without_force_then_refused() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let dir = temp.path();
    fs::write(dir.join("app.env"), "KEY=edited").unwrap();
    let enc = dir.join(enc_name("app.env", "KEY=1"));
    fs::write(&enc, "ciphertext").unwrap();

    // Act
    let output = rsenv(&["sops", "decrypt", enc.to_str().unwrap()]);

    // Assert
    assert!(!output.status.success(), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("use --force"),
        "{output:?}"
    );
    assert_eq!(
        fs::read_to_string(dir.join("app.env")).unwrap(),
        "KEY=edited"
    );
}
