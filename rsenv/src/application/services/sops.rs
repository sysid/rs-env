//! SOPS encryption/decryption service
//!
//! Provides batch encryption/decryption operations for files matching
//! configured patterns. Uses content-addressed filenames for staleness detection:
//! `{name}.{hash8}.enc` where hash8 is the first 8 hex chars of SHA-256.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rayon::prelude::*;
use tracing::debug;
use walkdir::WalkDir;

use crate::application::hash::{
    encrypted_filename, file_hash, is_old_enc_format, parse_encrypted_filename,
};
use crate::application::{ApplicationError, ApplicationResult};
use crate::config::Settings;
use crate::domain::{SopsStatus, StaleFile};
use crate::infrastructure::traits::{CommandRunner, FileSystem};
use crate::infrastructure::InfraError;

/// SOPS encryption/decryption service.
pub struct SopsService {
    fs: Arc<dyn FileSystem>,
    cmd: Arc<dyn CommandRunner>,
    settings: Arc<Settings>,
}

impl SopsService {
    /// Create a new SOPS service.
    pub fn new(
        fs: Arc<dyn FileSystem>,
        cmd: Arc<dyn CommandRunner>,
        settings: Arc<Settings>,
    ) -> Self {
        Self { fs, cmd, settings }
    }

    /// Collect files matching extensions OR exact filenames.
    ///
    /// Recursively walks the directory and returns files matching either:
    /// - File extension (case-sensitive, e.g., "env" matches "config.env")
    /// - Exact filename (case-sensitive, e.g., "dot_pypirc")
    ///
    /// # Arguments
    /// * `dir` - Directory to search
    /// * `extensions` - File extensions to match (without dot)
    /// * `filenames` - Exact filenames to match
    ///
    /// # Returns
    /// Vec of matching file paths
    pub fn collect_files(
        &self,
        dir: &Path,
        extensions: &[String],
        filenames: &[String],
    ) -> ApplicationResult<Vec<PathBuf>> {
        debug!(
            "collect_files: dir={}, extensions={:?}, filenames={:?}",
            dir.display(),
            extensions,
            filenames
        );
        let mut files = Vec::new();

        for entry in WalkDir::new(dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            let path = entry.path();
            let file_name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();

            // Check exact filename match
            if filenames.iter().any(|name| name == file_name) {
                files.push(path.to_path_buf());
                continue;
            }

            // Check extension match
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if extensions.iter().any(|e| e == ext) {
                    files.push(path.to_path_buf());
                }
            }
        }

        debug!("collect_files: found {} files", files.len());
        Ok(files)
    }

    /// Find all encrypted files (.enc) in a directory, grouped by their plaintext name.
    ///
    /// Returns a map from plaintext filename to list of encrypted file paths.
    /// Handles both new format ({name}.{hash}.enc) and old format ({name}.enc).
    fn find_encrypted_files(&self, dir: &Path) -> ApplicationResult<HashMap<String, Vec<PathBuf>>> {
        let mut result: HashMap<String, Vec<PathBuf>> = HashMap::new();

        for entry in WalkDir::new(dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            let path = entry.path();
            let filename = match path.file_name().and_then(|n| n.to_str()) {
                Some(f) => f,
                None => continue,
            };

            if !filename.ends_with(".enc") {
                continue;
            }

            // Try to parse as new format first
            if let Some((plaintext_name, _hash)) = parse_encrypted_filename(path) {
                // Reconstruct full plaintext path
                let plaintext_path = path.parent().unwrap_or(dir).join(&plaintext_name);
                result
                    .entry(plaintext_path.to_string_lossy().to_string())
                    .or_default()
                    .push(path.to_path_buf());
            } else if is_old_enc_format(path) {
                // Old format: {name}.enc -> plaintext is {name}
                let plaintext_name = &filename[..filename.len() - 4]; // strip .enc
                let plaintext_path = path.parent().unwrap_or(dir).join(plaintext_name);
                result
                    .entry(plaintext_path.to_string_lossy().to_string())
                    .or_default()
                    .push(path.to_path_buf());
            }
        }

        Ok(result)
    }

    /// Get encryption/decryption status for a directory.
    ///
    /// Categorizes files into:
    /// - `pending_encrypt`: Plaintext files needing encryption (no .enc exists)
    /// - `stale`: Plaintext files with outdated encryption (hash doesn't match)
    /// - `current`: Plaintext files with current encryption (hash matches)
    /// - `orphaned`: Encrypted files without matching plaintext
    ///
    /// # Arguments
    /// * `base_dir` - Directory to check (defaults to vault if None)
    ///
    /// # Returns
    /// SopsStatus with categorized files
    pub fn status(&self, base_dir: Option<&Path>) -> ApplicationResult<SopsStatus> {
        debug!(
            "status: base_dir={}",
            base_dir
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "default".into())
        );
        let dir = base_dir
            .map(PathBuf::from)
            .unwrap_or_else(|| self.settings.vault_base_dir.clone());

        // Collect plaintext files matching encryption patterns
        let plaintext_files = self.collect_files(
            &dir,
            &self.settings.sops.file_extensions_enc,
            &self.settings.sops.file_names_enc,
        )?;

        // Find all encrypted files grouped by plaintext name
        let encrypted_map = self.find_encrypted_files(&dir)?;

        let mut pending_encrypt = Vec::new();
        let mut stale = Vec::new();
        let mut current = Vec::new();
        let mut processed_enc_files: std::collections::HashSet<PathBuf> =
            std::collections::HashSet::new();

        // Categorize each plaintext file
        for plaintext_path in &plaintext_files {
            let plaintext_key = plaintext_path.to_string_lossy().to_string();
            let current_hash = file_hash(plaintext_path)?;

            if let Some(enc_files) = encrypted_map.get(&plaintext_key) {
                // Check if any encrypted file has matching hash
                let mut found_current = false;
                let mut stale_enc: Option<(PathBuf, String)> = None;

                for enc_path in enc_files {
                    processed_enc_files.insert(enc_path.clone());

                    if let Some((_, enc_hash)) = parse_encrypted_filename(enc_path) {
                        if enc_hash == current_hash {
                            found_current = true;
                        } else {
                            stale_enc = Some((enc_path.clone(), enc_hash));
                        }
                    }
                    // Old format files are ignored for hash matching
                }

                if found_current {
                    current.push(plaintext_path.clone());
                } else if let Some((old_enc, old_hash)) = stale_enc {
                    stale.push(StaleFile {
                        plaintext: plaintext_path.clone(),
                        old_encrypted: old_enc,
                        old_hash,
                        new_hash: current_hash,
                    });
                } else {
                    // Only old-format .enc files exist - treat as pending
                    pending_encrypt.push(plaintext_path.clone());
                }
            } else {
                // No encrypted version exists
                pending_encrypt.push(plaintext_path.clone());
            }
        }

        // Find orphaned .enc files (no matching plaintext)
        let mut orphaned = Vec::new();
        for enc_files in encrypted_map.values() {
            for enc_path in enc_files {
                if !processed_enc_files.contains(enc_path) {
                    orphaned.push(enc_path.clone());
                }
            }
        }

        debug!(
            "status: pending_encrypt={}, stale={}, current={}, orphaned={}",
            pending_encrypt.len(),
            stale.len(),
            current.len(),
            orphaned.len()
        );

        Ok(SopsStatus {
            pending_encrypt,
            stale,
            current,
            orphaned,
        })
    }

    /// Check if a file is a dotenv file (uses SOPS dotenv format).
    fn is_dotenv_file(path: &Path) -> bool {
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e == "env")
            .unwrap_or(false)
    }

    /// Encrypt a single file using SOPS with hash-based filename.
    ///
    /// Output file will be `{input}.{hash8}.enc`.
    /// If a file with the same hash already exists, returns it without re-encrypting.
    /// Deletes any stale encrypted versions.
    ///
    /// # Arguments
    /// * `input` - Path to plaintext file
    ///
    /// # Returns
    /// Path to encrypted output file
    pub fn encrypt_file(&self, input: &Path) -> ApplicationResult<PathBuf> {
        debug!("encrypt_file: input={}", input.display());

        // Compute hash of plaintext content
        let hash = file_hash(input)?;
        let output = encrypted_filename(input, &hash);

        // Check if already encrypted with same hash
        if self.fs.exists(&output) {
            debug!("encrypt_file: already up-to-date: {}", output.display());
            return Ok(output);
        }

        // Delete any stale encrypted versions
        self.delete_old_enc_files(input)?;

        let key = self
            .settings
            .sops
            .gpg_key
            .as_ref()
            .or(self.settings.sops.age_key.as_ref())
            .ok_or_else(|| ApplicationError::Config {
                message: "no encryption key configured (gpg_key or age_key)".into(),
            })?;

        // Determine key type and args
        let (key_flag, key_value) = if self.settings.sops.age_key.is_some() {
            ("--age", key.as_str())
        } else {
            ("--pgp", key.as_str())
        };

        let is_dotenv = Self::is_dotenv_file(input);
        let mut args: Vec<&str> = vec!["-e", key_flag, key_value];

        if is_dotenv {
            args.extend(&["--input-type", "dotenv", "--output-type", "dotenv"]);
        }

        args.extend(&["--output", output.to_str().unwrap_or_default()]);
        args.push(input.to_str().unwrap_or_default());

        let result =
            self.cmd
                .run("sops", &args)
                .map_err(|e| ApplicationError::OperationFailed {
                    context: format!("run sops encrypt: {}", input.display()),
                    source: Box::new(e),
                })?;

        if !result.status.success() {
            let stderr = String::from_utf8_lossy(&result.stderr);
            return Err(ApplicationError::OperationFailed {
                context: format!("sops encrypt {}: {}", input.display(), stderr),
                source: Box::new(InfraError::Sops {
                    message: stderr.to_string(),
                    exit_code: result.status.code(),
                }),
            });
        }

        debug!("encrypt_file: output={}", output.display());
        Ok(output)
    }

    /// Delete old encrypted versions of a plaintext file.
    fn delete_old_enc_files(&self, plaintext: &Path) -> ApplicationResult<()> {
        let parent = plaintext.parent().unwrap_or(Path::new("."));
        let filename = plaintext
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();

        // Pattern: {filename}.*.enc or {filename}.enc (old format)
        for entry in std::fs::read_dir(parent).into_iter().flatten() {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            let path = entry.path();
            let enc_name = match path.file_name().and_then(|n| n.to_str()) {
                Some(n) => n,
                None => continue,
            };

            // Check if this is an encrypted version of our file
            if enc_name.starts_with(filename) && enc_name.ends_with(".enc") {
                // Verify it's for this plaintext (not a different file that happens to start similarly)
                if let Some((parsed_name, _)) = parse_encrypted_filename(&path) {
                    if parsed_name == filename {
                        debug!("delete_old_enc_files: removing {}", path.display());
                        let _ = self.fs.remove_file(&path);
                    }
                } else if is_old_enc_format(&path) {
                    // Old format: {filename}.enc
                    let old_plaintext = &enc_name[..enc_name.len() - 4];
                    if old_plaintext == filename {
                        debug!(
                            "delete_old_enc_files: removing old format {}",
                            path.display()
                        );
                        let _ = self.fs.remove_file(&path);
                    }
                }
            }
        }

        Ok(())
    }

    /// Decrypt a single .enc file using SOPS.
    ///
    /// Handles both new format ({name}.{hash}.enc) and old format ({name}.enc).
    ///
    /// # Arguments
    /// * `input` - Path to encrypted file (must end in .enc)
    ///
    /// # Returns
    /// Path to decrypted output file
    pub fn decrypt_file(&self, input: &Path) -> ApplicationResult<PathBuf> {
        debug!("decrypt_file: input={}", input.display());

        let input_str = input.to_string_lossy();
        if !input_str.ends_with(".enc") {
            return Err(ApplicationError::OperationFailed {
                context: format!("file does not have .enc extension: {}", input.display()),
                source: Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "expected .enc file",
                )),
            });
        }

        // Determine output path based on format
        let output = if let Some((plaintext_name, _hash)) = parse_encrypted_filename(input) {
            // New format: {name}.{hash}.enc -> {name}
            input
                .parent()
                .unwrap_or(Path::new("."))
                .join(plaintext_name)
        } else {
            // Old format: {name}.enc -> {name}
            PathBuf::from(&input_str[..input_str.len() - 4])
        };

        let is_dotenv = Self::is_dotenv_file(&output);
        let mut args: Vec<&str> = vec!["-d"];

        if is_dotenv {
            args.extend(&["--input-type", "dotenv", "--output-type", "dotenv"]);
        }

        args.extend(&["--output", output.to_str().unwrap_or_default()]);
        args.push(input.to_str().unwrap_or_default());

        let result =
            self.cmd
                .run("sops", &args)
                .map_err(|e| ApplicationError::OperationFailed {
                    context: format!("run sops decrypt: {}", input.display()),
                    source: Box::new(e),
                })?;

        if !result.status.success() {
            let stderr = String::from_utf8_lossy(&result.stderr);
            return Err(ApplicationError::OperationFailed {
                context: format!("sops decrypt {}: {}", input.display(), stderr),
                source: Box::new(InfraError::Sops {
                    message: stderr.to_string(),
                    exit_code: result.status.code(),
                }),
            });
        }

        debug!("decrypt_file: output={}", output.display());
        Ok(output)
    }

    /// Encrypt all files matching configured patterns.
    ///
    /// Uses parallel execution with rayon.
    /// Only encrypts files that need it (pending_encrypt or stale).
    ///
    /// # Arguments
    /// * `base_dir` - Directory to process (defaults to vault if None)
    ///
    /// # Returns
    /// Vec of encrypted output file paths
    pub fn encrypt_all(&self, base_dir: Option<&Path>) -> ApplicationResult<Vec<PathBuf>> {
        debug!(
            "encrypt_all: base_dir={}",
            base_dir
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "default".into())
        );

        let status = self.status(base_dir)?;

        // Collect files that need encryption
        let mut to_encrypt: Vec<PathBuf> = status.pending_encrypt;
        to_encrypt.extend(status.stale.iter().map(|s| s.plaintext.clone()));

        if to_encrypt.is_empty() {
            debug!("encrypt_all: all files up-to-date");
            return Ok(Vec::new());
        }

        // Parallel encryption
        let results: Vec<ApplicationResult<PathBuf>> = to_encrypt
            .par_iter()
            .map(|path| self.encrypt_file(path))
            .collect();

        // Collect successes, propagate first error
        let mut outputs = Vec::new();
        for result in results {
            outputs.push(result?);
        }

        debug!("encrypt_all: encrypted {} files", outputs.len());
        Ok(outputs)
    }

    /// Decrypt all .enc files in a directory.
    ///
    /// Uses parallel execution with rayon.
    ///
    /// # Arguments
    /// * `base_dir` - Directory to process (defaults to vault if None)
    ///
    /// # Returns
    /// Vec of decrypted output file paths
    pub fn decrypt_all(&self, base_dir: Option<&Path>) -> ApplicationResult<Vec<PathBuf>> {
        debug!(
            "decrypt_all: base_dir={}",
            base_dir
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "default".into())
        );
        let dir = base_dir
            .map(PathBuf::from)
            .unwrap_or_else(|| self.settings.vault_base_dir.clone());

        let files = self.collect_files(&dir, &self.settings.sops.file_extensions_dec, &[])?;

        // Parallel decryption
        let results: Vec<ApplicationResult<PathBuf>> = files
            .par_iter()
            .map(|path| self.decrypt_file(path))
            .collect();

        // Collect successes, propagate first error
        let mut outputs = Vec::new();
        for result in results {
            outputs.push(result?);
        }

        debug!("decrypt_all: decrypted {} files", outputs.len());
        Ok(outputs)
    }

    /// Delete plaintext files that have current (hash-matching) encrypted versions.
    ///
    /// Only deletes files where the plaintext hash matches the encrypted file's hash.
    /// This prevents accidental data loss when plaintext has changed since encryption.
    ///
    /// # Arguments
    /// * `base_dir` - Directory to process (defaults to vault if None)
    ///
    /// # Returns
    /// Vec of deleted file paths
    pub fn clean(&self, base_dir: Option<&Path>) -> ApplicationResult<Vec<PathBuf>> {
        debug!(
            "clean: base_dir={}",
            base_dir
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "default".into())
        );
        let status = self.status(base_dir)?;

        // Only delete plaintext files that are "current" (hash matches)
        let mut deleted = Vec::new();
        for path in &status.current {
            self.fs
                .remove_file(path)
                .map_err(|e| ApplicationError::OperationFailed {
                    context: format!("delete plaintext: {}", path.display()),
                    source: Box::new(e),
                })?;
            deleted.push(path.clone());
        }

        debug!("clean: deleted {} plaintext files", deleted.len());
        Ok(deleted)
    }
}
