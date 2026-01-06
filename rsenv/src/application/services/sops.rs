//! SOPS encryption/decryption service
//!
//! Provides batch encryption/decryption operations for files matching
//! configured patterns. Mirrors confguard's SopsManager functionality.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use rayon::prelude::*;
use walkdir::WalkDir;

use crate::application::{ApplicationError, ApplicationResult};
use crate::config::Settings;
use crate::domain::SopsStatus;
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

        Ok(files)
    }

    /// Get encryption/decryption status for a directory.
    ///
    /// # Arguments
    /// * `base_dir` - Directory to check (defaults to vault if None)
    ///
    /// # Returns
    /// SopsStatus with categorized files
    pub fn status(&self, base_dir: Option<&Path>) -> ApplicationResult<SopsStatus> {
        let dir = base_dir.map(PathBuf::from).unwrap_or_else(|| {
            // Default to vault base dir if no dir specified
            self.settings.vault_base_dir.clone()
        });

        // Collect files matching encryption patterns (pending encryption)
        let pending_encrypt = self.collect_files(
            &dir,
            &self.settings.sops.file_extensions_enc,
            &self.settings.sops.file_names_enc,
        )?;

        // Collect already encrypted files (*.enc)
        let encrypted = self.collect_files(&dir, &self.settings.sops.file_extensions_dec, &[])?;

        // Find plaintext files that have an .enc counterpart (pending clean)
        let pending_clean: Vec<PathBuf> = pending_encrypt
            .iter()
            .filter(|path| {
                // Check if {path}.enc exists
                let enc_path = PathBuf::from(format!("{}.enc", path.display()));
                self.fs.exists(&enc_path)
            })
            .cloned()
            .collect();

        Ok(SopsStatus {
            pending_encrypt,
            encrypted,
            pending_clean,
        })
    }

    /// Encrypt a single file using SOPS.
    ///
    /// Output file will have `.enc` suffix appended.
    /// Uses GPG or Age key from settings.
    ///
    /// # Arguments
    /// * `input` - Path to plaintext file
    ///
    /// # Returns
    /// Path to encrypted output file
    pub fn encrypt_file(&self, input: &Path) -> ApplicationResult<PathBuf> {
        let output = PathBuf::from(format!("{}.enc", input.display()));

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

        // Special handling for .env files
        let is_env_file = input
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e == "env" || e == "envrc")
            .unwrap_or(false);

        let mut args: Vec<&str> = vec!["-e", key_flag, key_value];

        if is_env_file {
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

        Ok(output)
    }

    /// Decrypt a single .enc file using SOPS.
    ///
    /// Output file will have `.enc` suffix removed.
    ///
    /// # Arguments
    /// * `input` - Path to encrypted file (must end in .enc)
    ///
    /// # Returns
    /// Path to decrypted output file
    pub fn decrypt_file(&self, input: &Path) -> ApplicationResult<PathBuf> {
        // Strip .enc suffix for output
        let input_str = input.to_string_lossy();
        let output = if input_str.ends_with(".enc") {
            PathBuf::from(&input_str[..input_str.len() - 4])
        } else {
            return Err(ApplicationError::OperationFailed {
                context: format!("file does not have .enc extension: {}", input.display()),
                source: Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "expected .enc file",
                )),
            });
        };

        // Special handling for .env files
        let is_env_file = output
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e == "env" || e == "envrc")
            .unwrap_or(false);

        let mut args: Vec<&str> = vec!["-d"];

        if is_env_file {
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

        Ok(output)
    }

    /// Encrypt all files matching configured patterns.
    ///
    /// Uses parallel execution with rayon.
    ///
    /// # Arguments
    /// * `base_dir` - Directory to process (defaults to vault if None)
    ///
    /// # Returns
    /// Vec of encrypted output file paths
    pub fn encrypt_all(&self, base_dir: Option<&Path>) -> ApplicationResult<Vec<PathBuf>> {
        let dir = base_dir
            .map(PathBuf::from)
            .unwrap_or_else(|| self.settings.vault_base_dir.clone());

        let files = self.collect_files(
            &dir,
            &self.settings.sops.file_extensions_enc,
            &self.settings.sops.file_names_enc,
        )?;

        // Encrypt all matching files (overwrites existing .enc files)
        let to_encrypt = files;

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

        Ok(outputs)
    }

    /// Delete plaintext files that have .enc counterparts.
    ///
    /// Only deletes files matching encryption patterns that have an
    /// encrypted version alongside them.
    ///
    /// # Arguments
    /// * `base_dir` - Directory to process (defaults to vault if None)
    ///
    /// # Returns
    /// Vec of deleted file paths
    pub fn clean(&self, base_dir: Option<&Path>) -> ApplicationResult<Vec<PathBuf>> {
        let status = self.status(base_dir)?;

        let mut deleted = Vec::new();
        for path in &status.pending_clean {
            self.fs
                .remove_file(path)
                .map_err(|e| ApplicationError::OperationFailed {
                    context: format!("delete plaintext: {}", path.display()),
                    source: Box::new(e),
                })?;
            deleted.push(path.clone());
        }

        Ok(deleted)
    }
}
