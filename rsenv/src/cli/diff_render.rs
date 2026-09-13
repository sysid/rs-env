//! Unified diff rendering for `rsenv swap diff --patch`.
//!
//! Pure formatting: no filesystem access, so it is unit-testable on its own.

use similar::TextDiff;

use crate::application::services::is_binary;

/// Number of unchanged context lines around each hunk, matching git's default.
const CONTEXT_RADIUS: usize = 3;

/// Render a unified diff between two byte buffers.
///
/// Binary content is never rendered as a patch - the bytes may not be valid UTF-8, and a
/// screenful of mojibake helps nobody. Git behaves the same way.
pub fn render_unified(old: &[u8], new: &[u8], old_label: &str, new_label: &str) -> String {
    if is_binary(old) || is_binary(new) {
        return binary_marker(old_label, new_label);
    }

    let old_text = String::from_utf8_lossy(old);
    let new_text = String::from_utf8_lossy(new);

    TextDiff::from_lines(old_text.as_ref(), new_text.as_ref())
        .unified_diff()
        .context_radius(CONTEXT_RADIUS)
        .header(old_label, new_label)
        .to_string()
}

/// The stand-in emitted instead of a patch for binary content.
pub fn binary_marker(old_label: &str, new_label: &str) -> String {
    format!("Binary files {} and {} differ\n", old_label, new_label)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_changed_text_when_render_unified_then_emits_header_and_hunk() {
        // Arrange
        let old = b"alpha\nbeta\ngamma\n";
        let new = b"alpha\nBETA\ngamma\n";

        // Act
        let patch = render_unified(old, new, "baseline:notes.md", "notes.md");

        // Assert
        assert!(
            patch.contains("--- baseline:notes.md"),
            "patch was: {}",
            patch
        );
        assert!(patch.contains("+++ notes.md"), "patch was: {}", patch);
        assert!(patch.contains("-beta"), "patch was: {}", patch);
        assert!(patch.contains("+BETA"), "patch was: {}", patch);
        // Unchanged context is carried through
        assert!(patch.contains("alpha"), "patch was: {}", patch);
    }

    #[test]
    fn given_identical_text_when_render_unified_then_emits_nothing() {
        // Arrange
        let content = b"same\n";

        // Act
        let patch = render_unified(content, content, "a", "b");

        // Assert
        assert!(patch.is_empty(), "patch was: {}", patch);
    }

    #[test]
    fn given_binary_bytes_when_render_unified_then_returns_binary_marker() {
        // Arrange - NUL byte makes this binary by git's heuristic
        let old: &[u8] = &[0xFF, 0xFE, 0x00, 0x01];
        let new: &[u8] = &[0x00, 0x01, 0x02];

        // Act
        let patch = render_unified(old, new, "baseline:blob.bin", "blob.bin");

        // Assert - never attempt to render the bytes
        assert_eq!(
            patch,
            "Binary files baseline:blob.bin and blob.bin differ\n"
        );
    }

    #[test]
    fn given_one_binary_side_when_render_unified_then_returns_binary_marker() {
        // Arrange - a file that became binary is still not renderable
        let old = b"readable text\n";
        let new: &[u8] = &[0x00, 0x01];

        // Act
        let patch = render_unified(old, new, "a", "b");

        // Assert
        assert_eq!(patch, "Binary files a and b differ\n");
    }

    #[test]
    fn given_added_lines_when_render_unified_then_marks_them_added() {
        // Arrange
        let old = b"one\n";
        let new = b"one\ntwo\nthree\n";

        // Act
        let patch = render_unified(old, new, "a", "b");

        // Assert
        assert!(patch.contains("+two"), "patch was: {}", patch);
        assert!(patch.contains("+three"), "patch was: {}", patch);
    }
}
