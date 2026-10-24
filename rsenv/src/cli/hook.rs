//! Pre-commit hook script installed by `rsenv hook install`.

/// What the installed hook checks before a commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookScope {
    /// The rsenv base-dir repo: check every vault under it.
    Vaults,
    /// Any other repo (e.g. a secrets store): check that repo's own files.
    Dir,
}

/// Render the pre-commit hook for `scope`.
pub fn pre_commit_hook(scope: HookScope) -> String {
    // The hook runs from the repo it is installed in, so a Dir hook resolves that
    // repo at commit time instead of baking in the path it was installed from.
    let (flags, what) = match scope {
        HookScope::Vaults => ("--global", "vault(s)"),
        HookScope::Dir => (r#"--dir "$(git rev-parse --show-toplevel)""#, "this repo"),
    };
    format!(
        r#"#!/bin/bash
# rsenv pre-commit hook - prevents committing with stale/unencrypted files
# Installed by: rsenv hook install

if ! command -v rsenv &> /dev/null; then
    echo "Warning: rsenv not found in PATH, skipping encryption check"
    exit 0
fi

if ! rsenv sops status {flags} --check 2>/dev/null; then
    echo ""
    echo "ERROR: Unencrypted or stale files in {what}."
    echo "       Run 'rsenv sops encrypt {flags}' to update encryption."
    echo "       Use 'rsenv sops status {flags}' to see details."
    echo ""
    exit 1
fi
"#
    )
}
