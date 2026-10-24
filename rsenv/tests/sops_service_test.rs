//! Tests for SopsService

use std::path::PathBuf;
use std::sync::Arc;

use tempfile::TempDir;

use rsenv::application::services::SopsService;
use rsenv::config::{Settings, SopsConfig};
use rsenv::infrastructure::traits::{RealCommandRunner, RealFileSystem};

/// Helper to create test settings with custom SOPS config
fn test_settings(base_dir: PathBuf, sops: SopsConfig) -> Settings {
    Settings {
        base_dir,
        editor: "vim".to_string(),
        sops,
    }
}

/// Helper to create a default SopsConfig for testing
fn test_sops_config() -> SopsConfig {
    SopsConfig {
        gpg_key: Some("test-key".to_string()),
        age_key: None,
        file_extensions_enc: vec!["env".into(), "envrc".into()],
        file_names_enc: vec!["dot_pypirc".into()],
        file_extensions_dec: vec!["enc".into()],
        file_names_dec: vec![],
        encrypt_on_commit: true,
    }
}

// ============================================================
// collect_files() tests
// ============================================================

#[test]
fn given_files_matching_extension_when_collect_then_returns_matches() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let dir = temp.path();

    // Create files with various extensions
    std::fs::write(dir.join("config.env"), "KEY=value").unwrap();
    std::fs::write(dir.join("local.envrc"), "export X=1").unwrap();
    std::fs::write(dir.join("readme.txt"), "ignored").unwrap();
    std::fs::write(dir.join("data.json"), "{}").unwrap();

    let settings = Arc::new(test_settings(dir.to_path_buf(), test_sops_config()));
    let fs = Arc::new(RealFileSystem);
    let cmd = Arc::new(RealCommandRunner);
    let service = SopsService::new(fs, cmd, settings);

    // Act
    let files = service
        .collect_files(dir, &["env".into(), "envrc".into()], &[])
        .unwrap();

    // Assert
    assert_eq!(files.len(), 2);
    let names: Vec<_> = files
        .iter()
        .filter_map(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string())
        .collect();
    assert!(names.contains(&"config.env".to_string()));
    assert!(names.contains(&"local.envrc".to_string()));
}

#[test]
fn given_files_matching_exact_name_when_collect_then_returns_matches() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let dir = temp.path();

    std::fs::write(dir.join("dot_pypirc"), "[pypi]").unwrap();
    std::fs::write(dir.join("dot_pgpass"), "localhost:5432").unwrap();
    std::fs::write(dir.join("other.txt"), "ignored").unwrap();

    let settings = Arc::new(test_settings(dir.to_path_buf(), test_sops_config()));
    let fs = Arc::new(RealFileSystem);
    let cmd = Arc::new(RealCommandRunner);
    let service = SopsService::new(fs, cmd, settings);

    // Act
    let files = service
        .collect_files(dir, &[], &["dot_pypirc".into(), "dot_pgpass".into()])
        .unwrap();

    // Assert
    assert_eq!(files.len(), 2);
}

#[test]
fn given_nested_files_when_collect_then_finds_recursively() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let dir = temp.path();
    let subdir = dir.join("config").join("secrets");
    std::fs::create_dir_all(&subdir).unwrap();

    std::fs::write(dir.join("root.env"), "ROOT=1").unwrap();
    std::fs::write(subdir.join("nested.env"), "NESTED=1").unwrap();

    let settings = Arc::new(test_settings(dir.to_path_buf(), test_sops_config()));
    let fs = Arc::new(RealFileSystem);
    let cmd = Arc::new(RealCommandRunner);
    let service = SopsService::new(fs, cmd, settings);

    // Act
    let files = service.collect_files(dir, &["env".into()], &[]).unwrap();

    // Assert
    assert_eq!(files.len(), 2);
}

#[test]
fn given_empty_patterns_when_collect_then_returns_empty() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let dir = temp.path();

    std::fs::write(dir.join("config.env"), "KEY=value").unwrap();

    let settings = Arc::new(test_settings(dir.to_path_buf(), test_sops_config()));
    let fs = Arc::new(RealFileSystem);
    let cmd = Arc::new(RealCommandRunner);
    let service = SopsService::new(fs, cmd, settings);

    // Act
    let files = service.collect_files(dir, &[], &[]).unwrap();

    // Assert
    assert!(files.is_empty());
}

// ============================================================
// status() tests
// ============================================================

#[test]
fn given_mixed_files_when_status_then_categorizes_correctly() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let dir = temp.path();

    // Pending encryption (matches enc patterns, not encrypted yet)
    std::fs::write(dir.join("config.env"), "KEY=value").unwrap();
    // Orphaned encrypted file (no matching plaintext)
    std::fs::write(dir.join("secret.env.a1b2c3d4.enc"), "encrypted-data").unwrap();
    // Not matching any pattern
    std::fs::write(dir.join("readme.txt"), "ignored").unwrap();

    let settings = Arc::new(test_settings(dir.to_path_buf(), test_sops_config()));
    let fs = Arc::new(RealFileSystem);
    let cmd = Arc::new(RealCommandRunner);
    let service = SopsService::new(fs, cmd, settings);

    // Act
    let status = service.status(Some(dir)).unwrap();

    // Assert
    assert_eq!(status.pending_encrypt.len(), 1); // config.env needs encryption
    assert_eq!(status.orphaned.len(), 1); // secret.env.a1b2c3d4.enc is orphaned
    assert!(status.stale.is_empty());
    assert!(status.current.is_empty());
}

#[test]
fn given_plaintext_with_matching_hash_enc_when_status_then_marks_current() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let dir = temp.path();

    // Both plaintext and encrypted exist with matching hash
    let content = "KEY=plaintext";
    std::fs::write(dir.join("secret.env"), content).unwrap();
    let hash = rsenv::application::hash::content_hash(content.as_bytes());
    std::fs::write(
        dir.join(format!("secret.env.{}.enc", hash)),
        "encrypted-data",
    )
    .unwrap();

    let settings = Arc::new(test_settings(dir.to_path_buf(), test_sops_config()));
    let fs = Arc::new(RealFileSystem);
    let cmd = Arc::new(RealCommandRunner);
    let service = SopsService::new(fs, cmd, settings);

    // Act
    let status = service.status(Some(dir)).unwrap();

    // Assert - secret.env should be current since hash-matching .enc exists
    assert_eq!(status.current.len(), 1);
    assert!(status.current[0].ends_with("secret.env"));
}

// ============================================================
// clean() tests (old tests updated for hash-based behavior)
// ============================================================

#[test]
fn given_plaintext_with_matching_hash_enc_when_clean_then_deletes_plaintext() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let dir = temp.path();

    let content = "KEY=plaintext";
    let plaintext = dir.join("secret.env");
    std::fs::write(&plaintext, content).unwrap();

    // Create encrypted with matching hash
    let hash = rsenv::application::hash::content_hash(content.as_bytes());
    let encrypted = dir.join(format!("secret.env.{}.enc", hash));
    std::fs::write(&encrypted, "encrypted-data").unwrap();

    let settings = Arc::new(test_settings(dir.to_path_buf(), test_sops_config()));
    let fs = Arc::new(RealFileSystem);
    let cmd = Arc::new(RealCommandRunner);
    let service = SopsService::new(fs, cmd, settings);

    // Act
    let deleted = service.clean(Some(dir)).unwrap();

    // Assert
    assert_eq!(deleted.len(), 1);
    assert!(!plaintext.exists()); // Plaintext deleted
    assert!(encrypted.exists()); // Encrypted still exists
}

#[test]
fn given_plaintext_without_enc_when_clean_then_keeps_plaintext() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let dir = temp.path();

    let plaintext = dir.join("config.env");
    std::fs::write(&plaintext, "KEY=value").unwrap();
    // No .enc counterpart

    let settings = Arc::new(test_settings(dir.to_path_buf(), test_sops_config()));
    let fs = Arc::new(RealFileSystem);
    let cmd = Arc::new(RealCommandRunner);
    let service = SopsService::new(fs, cmd, settings);

    // Act
    let deleted = service.clean(Some(dir)).unwrap();

    // Assert
    assert!(deleted.is_empty());
    assert!(plaintext.exists()); // Plaintext preserved
}

// ============================================================
// Hash-based encryption tests (new format: {name}.{hash8}.enc)
// ============================================================

#[test]
fn given_plaintext_without_enc_when_status_then_pending_encrypt() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let dir = temp.path();

    // New file, no encrypted version exists
    std::fs::write(dir.join("secrets.env"), "API_KEY=abc123").unwrap();

    let settings = Arc::new(test_settings(dir.to_path_buf(), test_sops_config()));
    let fs = Arc::new(RealFileSystem);
    let cmd = Arc::new(RealCommandRunner);
    let service = SopsService::new(fs, cmd, settings);

    // Act
    let status = service.status(Some(dir)).unwrap();

    // Assert
    assert_eq!(status.pending_encrypt.len(), 1);
    assert!(status.stale.is_empty());
    assert!(status.current.is_empty());
}

#[test]
fn given_plaintext_with_matching_hash_enc_when_status_then_current() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let dir = temp.path();

    // Create plaintext
    let content = "API_KEY=abc123";
    std::fs::write(dir.join("secrets.env"), content).unwrap();

    // Create encrypted file with matching hash
    let hash = rsenv::application::hash::content_hash(content.as_bytes());
    let enc_name = format!("secrets.env.{}.enc", hash);
    std::fs::write(dir.join(&enc_name), "encrypted-data").unwrap();

    let settings = Arc::new(test_settings(dir.to_path_buf(), test_sops_config()));
    let fs = Arc::new(RealFileSystem);
    let cmd = Arc::new(RealCommandRunner);
    let service = SopsService::new(fs, cmd, settings);

    // Act
    let status = service.status(Some(dir)).unwrap();

    // Assert
    assert!(status.pending_encrypt.is_empty());
    assert!(status.stale.is_empty());
    assert_eq!(status.current.len(), 1);
}

#[test]
fn given_modified_plaintext_when_status_then_stale() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let dir = temp.path();

    // Create plaintext with NEW content
    let new_content = "API_KEY=new_value";
    std::fs::write(dir.join("secrets.env"), new_content).unwrap();

    // Create encrypted file with OLD hash (different content)
    let old_content = "API_KEY=old_value";
    let old_hash = rsenv::application::hash::content_hash(old_content.as_bytes());
    let enc_name = format!("secrets.env.{}.enc", old_hash);
    std::fs::write(dir.join(&enc_name), "encrypted-old-data").unwrap();

    let settings = Arc::new(test_settings(dir.to_path_buf(), test_sops_config()));
    let fs = Arc::new(RealFileSystem);
    let cmd = Arc::new(RealCommandRunner);
    let service = SopsService::new(fs, cmd, settings);

    // Act
    let status = service.status(Some(dir)).unwrap();

    // Assert
    assert!(status.pending_encrypt.is_empty());
    assert_eq!(status.stale.len(), 1);
    assert!(status.current.is_empty());

    // Verify stale file details
    let stale = &status.stale[0];
    assert!(stale.plaintext.ends_with("secrets.env"));
    assert_eq!(stale.old_hash, old_hash);
    assert_ne!(stale.new_hash, old_hash);
}

#[test]
fn given_orphaned_enc_when_status_then_orphaned() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let dir = temp.path();

    // Create encrypted file without matching plaintext
    std::fs::write(dir.join("deleted.env.a1b2c3d4.enc"), "orphaned-data").unwrap();

    let settings = Arc::new(test_settings(dir.to_path_buf(), test_sops_config()));
    let fs = Arc::new(RealFileSystem);
    let cmd = Arc::new(RealCommandRunner);
    let service = SopsService::new(fs, cmd, settings);

    // Act
    let status = service.status(Some(dir)).unwrap();

    // Assert
    assert_eq!(status.orphaned.len(), 1);
    assert!(status.orphaned[0].ends_with("deleted.env.a1b2c3d4.enc"));
}

#[test]
fn given_stale_plaintext_when_clean_then_keeps_plaintext() {
    // Arrange: plaintext changed since encryption - DO NOT delete!
    let temp = TempDir::new().unwrap();
    let dir = temp.path();

    // Create plaintext with NEW content
    let new_content = "API_KEY=new_value";
    let plaintext = dir.join("secrets.env");
    std::fs::write(&plaintext, new_content).unwrap();

    // Create encrypted file with OLD hash
    let old_content = "API_KEY=old_value";
    let old_hash = rsenv::application::hash::content_hash(old_content.as_bytes());
    let enc_name = format!("secrets.env.{}.enc", old_hash);
    std::fs::write(dir.join(&enc_name), "encrypted-old-data").unwrap();

    let settings = Arc::new(test_settings(dir.to_path_buf(), test_sops_config()));
    let fs = Arc::new(RealFileSystem);
    let cmd = Arc::new(RealCommandRunner);
    let service = SopsService::new(fs, cmd, settings);

    // Act
    let deleted = service.clean(Some(dir)).unwrap();

    // Assert - plaintext should NOT be deleted because hash doesn't match
    assert!(deleted.is_empty());
    assert!(plaintext.exists());
}

#[test]
fn given_current_plaintext_when_clean_then_deletes_plaintext() {
    // Arrange: plaintext matches encryption - safe to delete
    let temp = TempDir::new().unwrap();
    let dir = temp.path();

    // Create plaintext
    let content = "API_KEY=abc123";
    let plaintext = dir.join("secrets.env");
    std::fs::write(&plaintext, content).unwrap();

    // Create encrypted file with MATCHING hash
    let hash = rsenv::application::hash::content_hash(content.as_bytes());
    let enc_name = format!("secrets.env.{}.enc", hash);
    let encrypted = dir.join(&enc_name);
    std::fs::write(&encrypted, "encrypted-data").unwrap();

    let settings = Arc::new(test_settings(dir.to_path_buf(), test_sops_config()));
    let fs = Arc::new(RealFileSystem);
    let cmd = Arc::new(RealCommandRunner);
    let service = SopsService::new(fs, cmd, settings);

    // Act
    let deleted = service.clean(Some(dir)).unwrap();

    // Assert - plaintext IS deleted because hash matches
    assert_eq!(deleted.len(), 1);
    assert!(!plaintext.exists());
    assert!(encrypted.exists());
}

#[test]
fn given_needs_encryption_when_check_then_returns_true() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let dir = temp.path();

    std::fs::write(dir.join("secrets.env"), "API_KEY=abc123").unwrap();

    let settings = Arc::new(test_settings(dir.to_path_buf(), test_sops_config()));
    let fs = Arc::new(RealFileSystem);
    let cmd = Arc::new(RealCommandRunner);
    let service = SopsService::new(fs, cmd, settings);

    // Act
    let status = service.status(Some(dir)).unwrap();

    // Assert
    assert!(status.needs_encryption());
    assert_eq!(status.pending_count(), 1);
}

#[test]
fn given_all_current_when_check_then_returns_false() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let dir = temp.path();

    // Create plaintext
    let content = "API_KEY=abc123";
    std::fs::write(dir.join("secrets.env"), content).unwrap();

    // Create encrypted file with matching hash
    let hash = rsenv::application::hash::content_hash(content.as_bytes());
    let enc_name = format!("secrets.env.{}.enc", hash);
    std::fs::write(dir.join(&enc_name), "encrypted-data").unwrap();

    let settings = Arc::new(test_settings(dir.to_path_buf(), test_sops_config()));
    let fs = Arc::new(RealFileSystem);
    let cmd = Arc::new(RealCommandRunner);
    let service = SopsService::new(fs, cmd, settings);

    // Act
    let status = service.status(Some(dir)).unwrap();

    // Assert
    assert!(!status.needs_encryption());
    assert_eq!(status.pending_count(), 0);
}

// ============================================================
// Old format detection tests (for migration)
// ============================================================

#[test]
fn given_old_format_enc_with_plaintext_when_status_then_pending_encrypt() {
    // Arrange: old format file with its plaintext existing
    let temp = TempDir::new().unwrap();
    let dir = temp.path();

    // Old format: secrets.env.enc (no hash)
    std::fs::write(dir.join("secrets.env.enc"), "old-encrypted-data").unwrap();
    // Plaintext exists
    std::fs::write(dir.join("secrets.env"), "API_KEY=value").unwrap();

    let settings = Arc::new(test_settings(dir.to_path_buf(), test_sops_config()));
    let fs = Arc::new(RealFileSystem);
    let cmd = Arc::new(RealCommandRunner);
    let service = SopsService::new(fs, cmd, settings);

    // Act
    let status = service.status(Some(dir)).unwrap();

    // Assert: plaintext is pending_encrypt (old format .enc can't verify hash)
    // old format .enc is NOT orphaned because plaintext exists
    assert_eq!(status.pending_encrypt.len(), 1);
    assert!(status.orphaned.is_empty());
}

#[test]
fn given_old_format_enc_without_plaintext_when_status_then_orphaned() {
    // Arrange: old format file WITHOUT its plaintext
    let temp = TempDir::new().unwrap();
    let dir = temp.path();

    // Old format: deleted.env.enc (no hash, no plaintext)
    std::fs::write(dir.join("deleted.env.enc"), "old-encrypted-data").unwrap();

    let settings = Arc::new(test_settings(dir.to_path_buf(), test_sops_config()));
    let fs = Arc::new(RealFileSystem);
    let cmd = Arc::new(RealCommandRunner);
    let service = SopsService::new(fs, cmd, settings);

    // Act
    let status = service.status(Some(dir)).unwrap();

    // Assert: old format .enc is orphaned (no matching plaintext)
    assert!(status.pending_encrypt.is_empty());
    assert_eq!(status.orphaned.len(), 1);
}

// ============================================================
// gitignore tests - moved to gitignore_service_test.rs
// ============================================================
// NOTE: These tests have been migrated to the dedicated GitignoreService test file.

// ============================================================
// decrypt_all() tests: never overwrite a plaintext that differs
// ============================================================

/// Stands in for the `sops` binary: "decrypts" by writing a marker into the
/// `--output` file, or fails like sops does for inputs containing `fail_marker`.
struct FakeSops {
    fail_marker: Option<&'static str>,
}

impl rsenv::infrastructure::traits::CommandRunner for FakeSops {
    fn run(&self, _cmd: &str, args: &[&str]) -> std::io::Result<std::process::Output> {
        use std::os::unix::process::ExitStatusExt;
        let input = *args.last().unwrap();
        if self.fail_marker.is_some_and(|m| input.contains(m)) {
            return Ok(std::process::Output {
                status: std::process::ExitStatus::from_raw(1 << 8),
                stdout: vec![],
                stderr: b"Failed to get the data key".to_vec(),
            });
        }
        let output = args[args.iter().position(|a| *a == "--output").unwrap() + 1];
        std::fs::write(output, format!("decrypted from {input}"))?;
        Ok(std::process::Output {
            status: std::process::ExitStatus::from_raw(0),
            stdout: vec![],
            stderr: vec![],
        })
    }

    fn run_with_stdin(
        &self,
        _cmd: &str,
        _args: &[&str],
        _stdin: &str,
    ) -> std::io::Result<std::process::Output> {
        unimplemented!("not used by decrypt")
    }

    fn run_interactive(
        &self,
        _cmd: &str,
        _args: &[&str],
    ) -> std::io::Result<std::process::ExitStatus> {
        unimplemented!("not used by decrypt")
    }
}

fn decrypt_service(dir: &std::path::Path, fail_marker: Option<&'static str>) -> SopsService {
    let settings = Arc::new(test_settings(dir.to_path_buf(), test_sops_config()));
    SopsService::new(
        Arc::new(RealFileSystem),
        Arc::new(FakeSops { fail_marker }),
        settings,
    )
}

/// Ciphertext name rsenv would give `content` stored as `name`.
fn enc_name(name: &str, content: &str) -> String {
    format!(
        "{name}.{}.enc",
        rsenv::application::hash::content_hash(content.as_bytes())
    )
}

#[test]
fn given_missing_plaintext_when_decrypt_all_then_decrypted() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let dir = temp.path();
    std::fs::write(dir.join(enc_name("app.env", "KEY=1")), "ciphertext").unwrap();
    let service = decrypt_service(dir, None);

    // Act
    let report = service.decrypt_all(Some(dir), false).unwrap();

    // Assert
    assert_eq!(report.decrypted, vec![dir.join("app.env")]);
    assert!(report.kept.is_empty() && report.up_to_date.is_empty() && report.failed.is_empty());
    assert!(dir.join("app.env").exists());
}

#[test]
fn given_plaintext_matching_ciphertext_hash_when_decrypt_all_then_left_untouched() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let dir = temp.path();
    std::fs::write(dir.join("app.env"), "KEY=1").unwrap();
    std::fs::write(dir.join(enc_name("app.env", "KEY=1")), "ciphertext").unwrap();
    let service = decrypt_service(dir, None);

    // Act
    let report = service.decrypt_all(Some(dir), false).unwrap();

    // Assert
    assert_eq!(report.up_to_date, vec![dir.join("app.env")]);
    assert!(report.decrypted.is_empty());
    assert_eq!(
        std::fs::read_to_string(dir.join("app.env")).unwrap(),
        "KEY=1"
    );
}

/// The plaintext was edited after the last encrypt: decrypting would silently
/// revert the edit to the older ciphertext.
#[test]
fn given_plaintext_edited_since_encrypt_when_decrypt_all_then_kept_and_reported() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let dir = temp.path();
    std::fs::write(dir.join("app.env"), "KEY=edited").unwrap();
    let enc = dir.join(enc_name("app.env", "KEY=1"));
    std::fs::write(&enc, "ciphertext").unwrap();
    let service = decrypt_service(dir, None);

    // Act
    let report = service.decrypt_all(Some(dir), false).unwrap();

    // Assert
    assert_eq!(report.kept, vec![enc]);
    assert!(report.decrypted.is_empty());
    assert_eq!(
        std::fs::read_to_string(dir.join("app.env")).unwrap(),
        "KEY=edited"
    );
}

/// An old-format `X.enc` carries no hash, so an existing plaintext cannot be
/// shown to be identical: keep it.
#[test]
fn given_old_format_enc_with_existing_plaintext_when_decrypt_all_then_kept() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let dir = temp.path();
    std::fs::write(dir.join("app.env"), "KEY=1").unwrap();
    std::fs::write(dir.join("app.env.enc"), "ciphertext").unwrap();
    let service = decrypt_service(dir, None);

    // Act
    let report = service.decrypt_all(Some(dir), false).unwrap();

    // Assert
    assert_eq!(report.kept, vec![dir.join("app.env.enc")]);
    assert_eq!(
        std::fs::read_to_string(dir.join("app.env")).unwrap(),
        "KEY=1"
    );
}

#[test]
fn given_force_when_decrypt_all_then_differing_plaintext_overwritten() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let dir = temp.path();
    std::fs::write(dir.join("app.env"), "KEY=edited").unwrap();
    std::fs::write(dir.join(enc_name("app.env", "KEY=1")), "ciphertext").unwrap();
    let service = decrypt_service(dir, None);

    // Act
    let report = service.decrypt_all(Some(dir), true).unwrap();

    // Assert
    assert_eq!(report.decrypted, vec![dir.join("app.env")]);
    assert!(report.kept.is_empty());
    assert_ne!(
        std::fs::read_to_string(dir.join("app.env")).unwrap(),
        "KEY=edited"
    );
}

/// One broken ciphertext must not stop the run: the others are decrypted and the
/// failure is reported with sops' message.
#[test]
fn given_one_sops_failure_when_decrypt_all_then_others_decrypted_and_failure_reported() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let dir = temp.path();
    let broken = dir.join(enc_name("broken.env", "X=1"));
    std::fs::write(&broken, "ciphertext").unwrap();
    std::fs::write(dir.join(enc_name("good.env", "Y=1")), "ciphertext").unwrap();
    let service = decrypt_service(dir, Some("broken"));

    // Act
    let report = service.decrypt_all(Some(dir), false).unwrap();

    // Assert
    assert_eq!(report.decrypted, vec![dir.join("good.env")]);
    assert_eq!(report.failed.len(), 1);
    assert_eq!(report.failed[0].0, broken);
    assert!(
        report.failed[0].1.contains("Failed to get the data key"),
        "{}",
        report.failed[0].1
    );
}
