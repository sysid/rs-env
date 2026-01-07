//! .envrc section management for direnv integration.

use std::path::Path;
use std::sync::Arc;

use regex::Regex;

use crate::application::{ApplicationError, ApplicationResult};
use crate::infrastructure::traits::FileSystem;

pub const START_SECTION_DELIMITER: &str =
    "#------------------------------- rsenv start --------------------------------";
pub const VARS_SECTION_DELIMITER: &str =
    "#-------------------------------- rsenv vars --------------------------------";
pub const END_SECTION_DELIMITER: &str =
    "#-------------------------------- rsenv end ---------------------------------";

/// Update .envrc file with rsenv section.
/// Replaces existing section or appends if not present.
pub fn update_dot_envrc(
    fs: &Arc<dyn FileSystem>,
    target_file_path: &Path,
    data: &str,
) -> ApplicationResult<()> {
    // Ensure file exists
    if !fs.exists(target_file_path) {
        // Create empty file
        fs.write(target_file_path, "")
            .map_err(|e| ApplicationError::OperationFailed {
                context: format!("create .envrc at {}", target_file_path.display()),
                source: Box::new(e),
            })?;
    }

    // Ensure data ends with newline so END delimiter is on its own line
    let data_normalized = if data.ends_with('\n') {
        data.to_string()
    } else {
        format!("{}\n", data)
    };

    let section = format!(
        "\n{}\n{}{}\n",
        START_SECTION_DELIMITER, data_normalized, END_SECTION_DELIMITER
    );

    let content =
        fs.read_to_string(target_file_path)
            .map_err(|e| ApplicationError::OperationFailed {
                context: format!("read .envrc at {}", target_file_path.display()),
                source: Box::new(e),
            })?;

    let lines: Vec<&str> = content.lines().collect();

    let start_index = lines
        .iter()
        .position(|l| l.starts_with(START_SECTION_DELIMITER));
    let end_index = lines
        .iter()
        .position(|l| l.starts_with(END_SECTION_DELIMITER));

    let new_content = match (start_index, end_index) {
        (Some(start), Some(end)) if start < end => {
            let mut result = String::new();
            result.push_str(&lines[..start].join("\n"));
            result.push_str(&section);
            if end + 1 < lines.len() {
                result.push_str(&lines[end + 1..].join("\n"));
            }
            result
        }
        _ => {
            let mut result = content.clone();
            result.push_str(&section);
            result
        }
    };

    fs.write(target_file_path, &new_content)
        .map_err(|e| ApplicationError::OperationFailed {
            context: format!("write .envrc at {}", target_file_path.display()),
            source: Box::new(e),
        })?;

    Ok(())
}

/// Update only the vars section of an existing rsenv section.
/// Preserves the header (between start and vars markers) and only modifies
/// the content between vars and end markers.
///
/// If the file has a legacy section (no vars marker), it will auto-migrate
/// by inserting the vars marker.
///
/// Returns error if no rsenv section exists.
pub fn update_vars_section(
    fs: &Arc<dyn FileSystem>,
    target_file_path: &Path,
    vars_data: &str,
) -> ApplicationResult<()> {
    let content =
        fs.read_to_string(target_file_path)
            .map_err(|e| ApplicationError::OperationFailed {
                context: format!("read .envrc at {}", target_file_path.display()),
                source: Box::new(e),
            })?;

    let lines: Vec<&str> = content.lines().collect();

    let start_index = lines
        .iter()
        .position(|l| l.starts_with(START_SECTION_DELIMITER));
    let end_index = lines
        .iter()
        .position(|l| l.starts_with(END_SECTION_DELIMITER));
    let vars_index = lines
        .iter()
        .position(|l| l.starts_with(VARS_SECTION_DELIMITER));

    // Must have start and end markers
    let (start_idx, end_idx) = match (start_index, end_index) {
        (Some(s), Some(e)) if s < e => (s, e),
        _ => {
            return Err(ApplicationError::OperationFailed {
                context: format!(
                    "no rsenv section found in {}. Run 'rsenv init' first.",
                    target_file_path.display()
                ),
                source: Box::new(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "rsenv section not found",
                )),
            });
        }
    };

    // Normalize vars data
    let vars_normalized = if vars_data.is_empty() {
        String::new()
    } else if vars_data.ends_with('\n') {
        vars_data.to_string()
    } else {
        format!("{}\n", vars_data)
    };

    let new_content = match vars_index {
        Some(vars_idx) if vars_idx > start_idx && vars_idx < end_idx => {
            // Normal case: all three markers present
            // Keep: pre-section + start marker + header (up to vars marker) + vars marker + new vars + end marker + post-section
            let mut result = String::new();

            // Pre-section content
            if start_idx > 0 {
                result.push_str(&lines[..start_idx].join("\n"));
                result.push('\n');
            }

            // Header section (start marker through vars marker inclusive)
            result.push_str(&lines[start_idx..=vars_idx].join("\n"));
            result.push('\n');

            // New vars content
            result.push_str(&vars_normalized);

            // End marker
            result.push_str(END_SECTION_DELIMITER);
            result.push('\n');

            // Post-section content
            if end_idx + 1 < lines.len() {
                result.push_str(&lines[end_idx + 1..].join("\n"));
                if !result.ends_with('\n') {
                    result.push('\n');
                }
            }

            result
        }
        _ => {
            // Legacy migration: no vars marker, insert one
            eprintln!(
                "Warning: Migrating legacy rsenv section format in {}",
                target_file_path.display()
            );

            let mut result = String::new();

            // Pre-section content
            if start_idx > 0 {
                result.push_str(&lines[..start_idx].join("\n"));
                result.push('\n');
            }

            // Header section (start marker to end marker, exclusive)
            result.push_str(&lines[start_idx..end_idx].join("\n"));
            result.push('\n');

            // Insert vars marker
            result.push_str(VARS_SECTION_DELIMITER);
            result.push('\n');

            // New vars content
            result.push_str(&vars_normalized);

            // End marker
            result.push_str(END_SECTION_DELIMITER);
            result.push('\n');

            // Post-section content
            if end_idx + 1 < lines.len() {
                result.push_str(&lines[end_idx + 1..].join("\n"));
                if !result.ends_with('\n') {
                    result.push('\n');
                }
            }

            result
        }
    };

    fs.write(target_file_path, &new_content)
        .map_err(|e| ApplicationError::OperationFailed {
            context: format!("write .envrc at {}", target_file_path.display()),
            source: Box::new(e),
        })?;

    Ok(())
}

pub const RSENV_SWAPPED_MARKER: &str = "export RSENV_SWAPPED=1";

/// Add RSENV_SWAPPED marker to dot.envrc if not present.
/// This marker is placed OUTSIDE the rsenv section as a standalone line.
pub fn add_swapped_marker(
    fs: &Arc<dyn FileSystem>,
    dot_envrc_path: &Path,
) -> ApplicationResult<()> {
    if !fs.exists(dot_envrc_path) {
        // Create file with just the marker
        fs.write(dot_envrc_path, &format!("{}\n", RSENV_SWAPPED_MARKER))
            .map_err(|e| ApplicationError::OperationFailed {
                context: format!("create dot.envrc at {}", dot_envrc_path.display()),
                source: Box::new(e),
            })?;
        return Ok(());
    }

    let content =
        fs.read_to_string(dot_envrc_path)
            .map_err(|e| ApplicationError::OperationFailed {
                context: format!("read dot.envrc at {}", dot_envrc_path.display()),
                source: Box::new(e),
            })?;

    // Check if marker already exists
    if content
        .lines()
        .any(|line| line.trim() == RSENV_SWAPPED_MARKER)
    {
        return Ok(()); // Idempotent: marker already present
    }

    // Append marker
    let new_content = if content.ends_with('\n') || content.is_empty() {
        format!("{}{}\n", content, RSENV_SWAPPED_MARKER)
    } else {
        format!("{}\n{}\n", content, RSENV_SWAPPED_MARKER)
    };

    fs.write(dot_envrc_path, &new_content)
        .map_err(|e| ApplicationError::OperationFailed {
            context: format!("write dot.envrc at {}", dot_envrc_path.display()),
            source: Box::new(e),
        })?;

    Ok(())
}

/// Remove RSENV_SWAPPED marker from dot.envrc if present.
pub fn remove_swapped_marker(
    fs: &Arc<dyn FileSystem>,
    dot_envrc_path: &Path,
) -> ApplicationResult<()> {
    if !fs.exists(dot_envrc_path) {
        return Ok(()); // Nothing to remove
    }

    let content =
        fs.read_to_string(dot_envrc_path)
            .map_err(|e| ApplicationError::OperationFailed {
                context: format!("read dot.envrc at {}", dot_envrc_path.display()),
                source: Box::new(e),
            })?;

    // Filter out marker lines
    let lines: Vec<&str> = content
        .lines()
        .filter(|line| line.trim() != RSENV_SWAPPED_MARKER)
        .collect();

    let new_content = if lines.is_empty() {
        String::new()
    } else {
        format!("{}\n", lines.join("\n"))
    };

    fs.write(dot_envrc_path, &new_content)
        .map_err(|e| ApplicationError::OperationFailed {
            context: format!("write dot.envrc at {}", dot_envrc_path.display()),
            source: Box::new(e),
        })?;

    Ok(())
}

/// Metadata extracted from rsenv section in dot.envrc.
#[derive(Debug, Clone)]
pub struct RsenvMetadata {
    pub relative: bool,
    pub sentinel: String,
    pub source_dir: String,
}

/// Parse rsenv metadata from dot.envrc content.
/// Returns None if no rsenv section found or metadata is incomplete.
pub fn parse_rsenv_metadata(content: &str) -> Option<RsenvMetadata> {
    // Check for rsenv section
    if !content.contains(START_SECTION_DELIMITER) {
        return None;
    }

    let mut relative: Option<bool> = None;
    let mut sentinel: Option<String> = None;
    let mut source_dir: Option<String> = None;

    for line in content.lines() {
        let line = line.trim();

        if line.starts_with("# config.relative = ") {
            let value = line.strip_prefix("# config.relative = ")?;
            relative = Some(value == "true");
        } else if line.starts_with("# state.sentinel = '") {
            let value = line
                .strip_prefix("# state.sentinel = '")?
                .strip_suffix('\'')?;
            sentinel = Some(value.to_string());
        } else if line.starts_with("# state.sourceDir = '") {
            let value = line
                .strip_prefix("# state.sourceDir = '")?
                .strip_suffix('\'')?;
            source_dir = Some(value.to_string());
        }
    }

    Some(RsenvMetadata {
        relative: relative?,
        sentinel: sentinel?,
        source_dir: source_dir?,
    })
}

/// Update state.sourceDir in dot.envrc file.
pub fn update_source_dir(
    fs: &Arc<dyn FileSystem>,
    path: &Path,
    new_source_dir: &str,
) -> ApplicationResult<()> {
    let content = fs
        .read_to_string(path)
        .map_err(|e| ApplicationError::OperationFailed {
            context: format!("read dot.envrc at {}", path.display()),
            source: Box::new(e),
        })?;

    // Replace the state.sourceDir line
    let re = Regex::new(r"# state\.sourceDir = '[^']*'").map_err(|e| {
        ApplicationError::OperationFailed {
            context: "compile regex".to_string(),
            source: Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                e.to_string(),
            )),
        }
    })?;

    let new_content = re.replace(
        &content,
        format!("# state.sourceDir = '{}'", new_source_dir),
    );

    fs.write(path, &new_content)
        .map_err(|e| ApplicationError::OperationFailed {
            context: format!("write dot.envrc at {}", path.display()),
            source: Box::new(e),
        })?;

    Ok(())
}

/// Delete rsenv section from file.
pub fn delete_section(fs: &Arc<dyn FileSystem>, file_path: &Path) -> ApplicationResult<()> {
    let content = fs
        .read_to_string(file_path)
        .map_err(|e| ApplicationError::OperationFailed {
            context: format!("read file at {}", file_path.display()),
            source: Box::new(e),
        })?;

    let pattern = format!(
        r"(?s){start}.*{end}\n?",
        start = regex::escape(START_SECTION_DELIMITER),
        end = regex::escape(END_SECTION_DELIMITER),
    );
    let re = Regex::new(&pattern).map_err(|e| ApplicationError::OperationFailed {
        context: "compile regex".to_string(),
        source: Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            e.to_string(),
        )),
    })?;

    // Check for multiple sections
    let matches: Vec<_> = re.find_iter(&content).collect();
    if matches.len() > 1 {
        return Err(ApplicationError::OperationFailed {
            context: format!("multiple rsenv sections in {}", file_path.display()),
            source: Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "multiple sections found",
            )),
        });
    }

    let result = re.replace(&content, "");

    fs.write(file_path, &result)
        .map_err(|e| ApplicationError::OperationFailed {
            context: format!("write file at {}", file_path.display()),
            source: Box::new(e),
        })?;

    Ok(())
}
