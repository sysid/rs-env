//! Editor resolution for the vim-driven env commands.

use std::path::Path;

/// Does this editor command understand vim's command line?
///
/// `env edit-leaf` and `env tree-edit` do not merely open files: they drive the editor
/// with vim-only flags - `-O` for the vertical-split grid, `-S` to source a generated
/// vimscript. Any other editor would take the flag for a filename, so the commands have
/// to refuse rather than open something unusable.
pub fn is_vim_family(editor: &str) -> bool {
    let stem = Path::new(editor)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    stem == "vi" || stem == "view" || stem.contains("vim")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_vim_variants_when_checked_then_accepted() {
        for editor in ["vim", "nvim", "gvim", "mvim", "vimdiff", "vi", "view"] {
            assert!(is_vim_family(editor), "{editor} should be accepted");
        }
    }

    #[test]
    fn given_absolute_path_when_checked_then_only_the_program_name_matters() {
        assert!(is_vim_family("/opt/homebrew/bin/nvim"));
        assert!(!is_vim_family("/opt/homebrew/bin/code"));
    }

    #[test]
    fn given_non_vim_editors_when_checked_then_rejected() {
        for editor in ["code", "emacs", "nano", "hx", "subl", ""] {
            assert!(!is_vim_family(editor), "{editor} should be rejected");
        }
    }
}
