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
    encrypted_filename, encrypted_siblings, file_hash, is_old_enc_format, parse_encrypted_filename,
};
use crate::application::{ApplicationError, ApplicationResult};
use crate::config::Settings;
use crate::domain::{SopsStatus, StaleFile};
use crate::infrastructure::traits::{CommandRunner, FileSystem};
use crate::infrastructure::InfraError;

/// Outcome of a directory-wide decrypt. The run never stops at one file: every
/// `.enc` ends up in exactly one list.
#[derive(Debug, Default)]
pub struct DecryptReport {
    /// Plaintext written (it was missing, or `force` was set).
    pub decrypted: Vec<PathBuf>,
    /// Plaintext already identical to the ciphertext (hash match); not touched.
    pub up_to_date: Vec<PathBuf>,
    /// Ciphertexts NOT decrypted because their plaintext exists and differs, or
    /// cannot be verified (old `X.enc` format). Decrypting would overwrite it.
    pub kept: Vec<PathBuf>,
    /// Ciphertexts sops failed on, with the error message.
    pub failed: Vec<(PathBuf, String)>,
}

/// What decrypting one ciphertext did.
#[derive(Debug, PartialEq, Eq)]
pub enum DecryptOutcome {
    Decrypted(PathBuf),
    UpToDate(PathBuf),
    /// Existing plaintext differs (or is unverifiable) and was left alone.
    Kept {
        plaintext: PathBuf,
    },
}

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
            .unwrap_or_else(|| self.settings.vaults_dir());

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
        for path in encrypted_siblings(plaintext) {
            debug!("delete_old_enc_files: removing {}", path.display());
            let _ = self.fs.remove_file(&path);
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

        let output = Self::plaintext_path(input)?;

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

    /// Plaintext path a ciphertext decrypts to.
    ///
    /// New format `{name}.{hash}.enc` -> `{name}`; old format `{name}.enc` -> `{name}`.
    fn plaintext_path(input: &Path) -> ApplicationResult<PathBuf> {
        let input_str = input.to_string_lossy();
        let Some(without_enc) = input_str.strip_suffix(".enc") else {
            return Err(ApplicationError::OperationFailed {
                context: format!("file does not have .enc extension: {}", input.display()),
                source: Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "expected .enc file",
                )),
            });
        };
        Ok(match parse_encrypted_filename(input) {
            Some((plaintext_name, _hash)) => input
                .parent()
                .unwrap_or(Path::new("."))
                .join(plaintext_name),
            None => PathBuf::from(without_enc),
        })
    }

    /// Decrypt one ciphertext without clobbering a plaintext that differs from it.
    ///
    /// An existing plaintext is only overwritten with `force`: it may hold edits made
    /// since the last encrypt, which decrypting would silently revert. Identity is
    /// proven by the hash in the ciphertext name; an old-format `X.enc` has none, so
    /// its existing plaintext is kept as well.
    pub fn decrypt(&self, input: &Path, force: bool) -> ApplicationResult<DecryptOutcome> {
        let plaintext = Self::plaintext_path(input)?;
        if self.fs.exists(&plaintext) {
            let matches = match parse_encrypted_filename(input) {
                Some((_, enc_hash)) => file_hash(&plaintext)? == enc_hash,
                None => false,
            };
            if matches {
                debug!("decrypt: up to date: {}", plaintext.display());
                return Ok(DecryptOutcome::UpToDate(plaintext));
            }
            if !force {
                debug!(
                    "decrypt: keeping differing plaintext: {}",
                    plaintext.display()
                );
                return Ok(DecryptOutcome::Kept { plaintext });
            }
        }
        self.decrypt_file(input).map(DecryptOutcome::Decrypted)
    }

    /// Decrypt all .enc files in a directory.
    ///
    /// Uses parallel execution with rayon. Never stops at a failing file: every
    /// ciphertext is reported in exactly one list of the returned report.
    ///
    /// # Arguments
    /// * `base_dir` - Directory to process (defaults to vault if None)
    /// * `force` - Overwrite existing plaintext that differs from its ciphertext
    pub fn decrypt_all(
        &self,
        base_dir: Option<&Path>,
        force: bool,
    ) -> ApplicationResult<DecryptReport> {
        debug!(
            "decrypt_all: base_dir={}, force={}",
            base_dir
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "default".into()),
            force
        );
        let dir = base_dir
            .map(PathBuf::from)
            .unwrap_or_else(|| self.settings.vaults_dir());

        let files = self.collect_files(&dir, &self.settings.sops.file_extensions_dec, &[])?;

        let outcomes: Vec<(PathBuf, ApplicationResult<DecryptOutcome>)> = files
            .par_iter()
            .map(|path| (path.clone(), self.decrypt(path, force)))
            .collect();

        let mut report = DecryptReport::default();
        for (enc, outcome) in outcomes {
            match outcome {
                Ok(DecryptOutcome::Decrypted(p)) => report.decrypted.push(p),
                Ok(DecryptOutcome::UpToDate(p)) => report.up_to_date.push(p),
                Ok(DecryptOutcome::Kept { .. }) => report.kept.push(enc),
                Err(e) => report.failed.push((enc, e.to_string())),
            }
        }
        report.decrypted.sort();
        report.up_to_date.sort();
        report.kept.sort();
        report.failed.sort();

        debug!(
            "decrypt_all: decrypted={}, up_to_date={}, kept={}, failed={}",
            report.decrypted.len(),
            report.up_to_date.len(),
            report.kept.len(),
            report.failed.len()
        );
        Ok(report)
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
