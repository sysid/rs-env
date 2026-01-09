//! Domain entities: core data structures

use std::collections::BTreeMap;
use std::path::PathBuf;

/// A project linked to a vault.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    pub project_dir: PathBuf,
    pub vault: Vault,
}

/// Secure storage location for a project.
/// Unifies confguard's "sentinel directory" and rplc's "mirror directory".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Vault {
    /// Absolute path to the vault directory
    pub path: PathBuf,
    /// Unique identifier, e.g., "myproject-a1b2c3d4"
    pub sentinel_id: String,
}

/// Environment file with optional parent links.
/// Supports DAG structure (multiple parents via `# rsenv: parent1.env, parent2.env`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvFile {
    /// Path to this env file
    pub path: PathBuf,
    /// Parent env files (supports DAG with multiple parents)
    pub parents: Vec<PathBuf>,
    /// Parsed environment variables
    pub variables: BTreeMap<String, String>,
}

impl EnvFile {
    /// Parse env file content.
    ///
    /// Extracts:
    /// - Parent links from `# rsenv: parent.env` or `# rsenv: a.env b.env`
    /// - Environment variables from `export KEY=value` lines
    ///
    /// # Arguments
    /// * `content` - File content to parse
    /// * `file_path` - Path to the file (used to resolve relative parent paths)
    pub fn parse(content: &str, file_path: PathBuf) -> Result<Self, EnvFileParseError> {
        let mut parents = Vec::new();
        let mut variables = BTreeMap::new();

        let parent_dir = file_path.parent();

        for line in content.lines() {
            let trimmed = line.trim();

            // Skip empty lines
            if trimmed.is_empty() {
                continue;
            }

            // Check for rsenv directive
            if let Some(directive) = trimmed.strip_prefix("# rsenv:") {
                let parent_specs = directive.trim();
                // v1 compatibility: space-separated parents (not comma)
                for parent_spec in parent_specs.split_whitespace() {
                    if !parent_spec.is_empty() {
                        // Expand environment variables in path (v1 behavior)
                        let expanded = expand_env_vars(parent_spec);
                        let parent_path = PathBuf::from(&expanded);
                        // If relative, resolve against file's directory
                        let resolved = if parent_path.is_absolute() {
                            parent_path
                        } else if let Some(dir) = parent_dir {
                            dir.join(parent_path)
                        } else {
                            parent_path
                        };
                        parents.push(resolved);
                    }
                }
                continue;
            }

            // Skip other comments
            if trimmed.starts_with('#') {
                continue;
            }

            // v1 compatibility: only parse "export VAR=value" lines
            if trimmed.starts_with("export ") {
                let rest = &trimmed[7..]; // skip "export "
                if let Some((key, value)) = parse_env_line(rest) {
                    variables.insert(key.to_string(), value);
                }
            }
        }

        Ok(Self {
            path: file_path,
            parents,
            variables,
        })
    }
}

/// Parse a single environment variable line.
/// Returns (key, value) with trailing comments and quotes stripped from value.
fn parse_env_line(line: &str) -> Option<(&str, String)> {
    let eq_pos = line.find('=')?;
    let key = line[..eq_pos].trim();
    let value = line[eq_pos + 1..].trim();

    // Strip trailing comment (outside quotes) before stripping quotes
    let value = strip_trailing_comment(value);

    // Strip surrounding quotes
    let value = strip_quotes(value);

    Some((key, value))
}

/// Strip trailing comment from a value, respecting quotes.
/// `'value'  # comment` → `'value'`
/// `value  # comment` → `value`
/// `'val#ue'` → `'val#ue'` (# inside quotes is not a comment)
fn strip_trailing_comment(s: &str) -> &str {
    let s = s.trim();
    let bytes = s.as_bytes();
    let mut in_single_quote = false;
    let mut in_double_quote = false;

    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'\'' if !in_double_quote => in_single_quote = !in_single_quote,
            b'"' if !in_single_quote => in_double_quote = !in_double_quote,
            b'#' if !in_single_quote && !in_double_quote => {
                return s[..i].trim_end();
            }
            _ => {}
        }
    }
    s
}

/// Strip surrounding quotes (single or double) from a value.
fn strip_quotes(s: &str) -> String {
    let s = s.trim();
    if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
        if s.len() >= 2 {
            return s[1..s.len() - 1].to_string();
        }
    }
    s.to_string()
}

/// Error parsing an env file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvFileParseError {
    pub message: String,
}

/// Expand environment variables in a path string.
///
/// Supports:
/// - `$VAR` syntax
/// - `${VAR}` syntax
/// - `~` for home directory
///
/// Uses shellexpand crate for robust expansion.
pub fn expand_env_vars(path: &str) -> String {
    shellexpand::full(path)
        .map(|s| s.into_owned())
        .unwrap_or_else(|_| path.to_string())
}

/// File managed by guard (symlinked into project).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuardedFile {
    /// Where the symlink lives in the project
    pub project_path: PathBuf,
    /// Where the actual file lives in the vault
    pub vault_path: PathBuf,
    /// Whether the file is SOPS-encrypted
    pub encrypted: bool,
}

/// File managed by the swap mechanism.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwapFile {
    /// Path in the project directory
    pub project_path: PathBuf,
    /// Path in the vault (override version)
    pub vault_path: PathBuf,
    /// Current swap state
    pub state: SwapState,
}

/// State of a swappable file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwapState {
    /// Original in project, override in vault
    Out,
    /// Override in project, original backed up
    In { hostname: String },
}

/// SOPS encryption status for a directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SopsStatus {
    /// Files that would be encrypted (matching enc patterns, not yet encrypted)
    pub pending_encrypt: Vec<PathBuf>,
    /// Files that are already encrypted (*.enc)
    pub encrypted: Vec<PathBuf>,
    /// Files that would be deleted by clean (plaintext matching enc patterns)
    pub pending_clean: Vec<PathBuf>,
}
