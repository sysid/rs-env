//! Content hashing for staleness detection
//!
//! Provides SHA-256 based content hashing for detecting when plaintext files
//! have changed since their last encryption.

use sha2::{Digest, Sha256};
use std::path::Path;

use crate::application::{ApplicationError, ApplicationResult};

/// Compute 8-character hex hash of content (first 32 bits of SHA-256).
///
/// # Arguments
/// * `content` - Byte slice to hash
///
/// # Returns
/// 8-character lowercase hex string (e.g., "a1b2c3d4")
pub fn content_hash(content: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content);
    let result = hasher.finalize();
    // First 4 bytes = 8 hex characters
    hex::encode(&result[..4])
}

/// Compute hash of file contents.
///
/// # Arguments
/// * `path` - Path to file
///
/// # Returns
/// 8-character hex hash of file content
pub fn file_hash(path: &Path) -> ApplicationResult<String> {
    let content = std::fs::read(path).map_err(|e| ApplicationError::OperationFailed {
        context: format!("read file for hashing: {}", path.display()),
        source: Box::new(e),
    })?;
    Ok(content_hash(&content))
}

/// Parse an encrypted filename to extract the original name and hash.
///
/// Pattern: `{name}.{hash8}.enc` where hash8 is exactly 8 hex characters.
///
/// # Arguments
/// * `enc_path` - Path to encrypted file
///
/// # Returns
/// Some((original_filename, hash)) or None if pattern doesn't match
///
/// # Examples
/// ```
/// use std::path::Path;
/// use rsenv::application::hash::parse_encrypted_filename;
///
/// let result = parse_encrypted_filename(Path::new("secrets.env.a1b2c3d4.enc"));
/// assert_eq!(result, Some(("secrets.env".to_string(), "a1b2c3d4".to_string())));
///
/// let result = parse_encrypted_filename(Path::new("secrets.env.enc"));
/// assert_eq!(result, None); // Old format, no hash
/// ```
pub fn parse_encrypted_filename(enc_path: &Path) -> Option<(String, String)> {
    let filename = enc_path.file_name()?.to_str()?;

    // Must end with .enc
    let without_enc = filename.strip_suffix(".enc")?;

    // Must have a hash component (8 hex chars) before .enc
    // Pattern: {name}.{hash8}
    let last_dot = without_enc.rfind('.')?;
    let hash = &without_enc[last_dot + 1..];

    // Validate hash is exactly 8 hex characters
    if hash.len() != 8 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }

    let name = &without_enc[..last_dot];
    if name.is_empty() {
        return None;
    }

    Some((name.to_string(), hash.to_lowercase()))
}

/// Check if a filename matches the old encryption format (without hash).
///
/// Old format: `{name}.enc` (no hash component)
/// New format: `{name}.{hash8}.enc`
///
/// # Arguments
/// * `enc_path` - Path to encrypted file
///
/// # Returns
/// true if this is the old format (needs migration)
pub fn is_old_enc_format(enc_path: &Path) -> bool {
    let filename = match enc_path.file_name().and_then(|f| f.to_str()) {
        Some(f) => f,
        None => return false,
    };

    // Must end with .enc
    if !filename.ends_with(".enc") {
        return false;
    }

    // If we can parse it as new format, it's not old format
    if parse_encrypted_filename(enc_path).is_some() {
        return false;
    }

    // Old format: just {name}.enc
    true
}

/// Generate encrypted filename with embedded hash.
///
/// # Arguments
/// * `plaintext` - Path to plaintext file
/// * `hash` - 8-character hex hash
///
/// # Returns
/// Path with format `{name}.{hash}.enc`
///
/// # Examples
/// ```
/// use std::path::Path;
/// use rsenv::application::hash::encrypted_filename;
///
/// let result = encrypted_filename(Path::new("secrets.env"), "a1b2c3d4");
/// assert_eq!(result.file_name().unwrap().to_str().unwrap(), "secrets.env.a1b2c3d4.enc");
/// ```
pub fn encrypted_filename(plaintext: &Path, hash: &str) -> std::path::PathBuf {
    let name = plaintext.file_name().unwrap().to_str().unwrap();
    plaintext.with_file_name(format!("{}.{}.enc", name, hash))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_hash_deterministic() {
        let hash1 = content_hash(b"hello world");
        let hash2 = content_hash(b"hello world");
        assert_eq!(hash1, hash2);
        assert_eq!(hash1.len(), 8);
    }

    #[test]
    fn test_content_hash_different_content() {
        let hash1 = content_hash(b"hello");
        let hash2 = content_hash(b"world");
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_parse_encrypted_filename_new_format() {
        let path = Path::new("/vault/secrets.env.a1b2c3d4.enc");
        let result = parse_encrypted_filename(path);
        assert_eq!(
            result,
            Some(("secrets.env".to_string(), "a1b2c3d4".to_string()))
        );
    }

    #[test]
    fn test_parse_encrypted_filename_old_format() {
        let path = Path::new("/vault/secrets.env.enc");
        let result = parse_encrypted_filename(path);
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_encrypted_filename_invalid_hash() {
        // Too short
        let path = Path::new("/vault/secrets.env.a1b2.enc");
        assert_eq!(parse_encrypted_filename(path), None);

        // Too long
        let path = Path::new("/vault/secrets.env.a1b2c3d4e5.enc");
        assert_eq!(parse_encrypted_filename(path), None);

        // Non-hex
        let path = Path::new("/vault/secrets.env.ghijklmn.enc");
        assert_eq!(parse_encrypted_filename(path), None);
    }

    #[test]
    fn test_is_old_enc_format() {
        assert!(is_old_enc_format(Path::new("secrets.env.enc")));
        assert!(!is_old_enc_format(Path::new("secrets.env.a1b2c3d4.enc")));
        assert!(!is_old_enc_format(Path::new("secrets.env")));
    }

    #[test]
    fn test_encrypted_filename() {
        let path = Path::new("/vault/secrets.env");
        let result = encrypted_filename(path, "a1b2c3d4");
        assert_eq!(result, Path::new("/vault/secrets.env.a1b2c3d4.enc"));
    }
}
