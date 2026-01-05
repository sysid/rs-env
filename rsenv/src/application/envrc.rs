//! .envrc section management for direnv integration.

use std::path::Path;
use std::sync::Arc;

use regex::Regex;

use crate::application::{ApplicationError, ApplicationResult};
use crate::infrastructure::traits::FileSystem;

pub const START_SECTION_DELIMITER: &str =
    "#------------------------------- rsenv start --------------------------------";
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
