#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""One-shot migration: stop storing RSENV_SWAPPED in dot.envrc, derive it instead.

Why
---
`dot.envrc` is content-addressed (`dot.envrc.<sha256-prefix>.enc`) and SOPS-encrypted.
rsenv <= 5.5.0 appended a literal `export RSENV_SWAPPED=1` on swap-in and removed it on
swap-out, so the file's hash flipped with swap state: `rsenv sops status --global` was
permanently stale for every swapped-in project, and the pre-commit hook (which checks
*all* vaults) blocked unrelated commits.

The literal line was also appended at EOF, i.e. *after* the `${RSENV_SWAPPED}` consumer
in every vault inspected - so it never even took effect within a single direnv load.

This script replaces the stored line with a derivation that asks rsenv for the live swap
state, placed above the consumer. dot.envrc then has a swap-state-invariant hash.

Usage
-----
    ./scripts/migrate_swapped_marker.py              # dry run, prints the plan
    ./scripts/migrate_swapped_marker.py --apply      # rewrite the files
    rsenv sops encrypt --global                      # re-encrypt afterwards

Dry run is the default: dot.envrc holds plaintext secrets and is gitignored, so an
unwanted rewrite has no git safety net.
"""

from __future__ import annotations

import argparse
import hashlib
import sys
from pathlib import Path

LEGACY_MARKER = "export RSENV_SWAPPED=1"

# Superseded interim fix: a state file rsenv never writes.
LEGACY_STATE_FILE_FRAGMENT = "swap/.swapped"

DERIVATION = [
    # NOT `# rsenv:` - that prefix is the env-hierarchy parent link, and a directory scan
    # for it read this sentence's words as a parent list (see scripts/fix_swap_guards.py).
    "# rsenv-note: RSENV_SWAPPED is DERIVED, never stored here - dot.envrc is content-addressed",
    "# and SOPS-encrypted, so a line that changes with swap state churns the ciphertext.",
    "# At EOF on purpose: rsenv must be on PATH, and direnv exports the final environment",
    "# regardless of line order, so consumers like starship see it at every prompt.",
    "rsenv swap status --silent >/dev/null 2>&1",
    "[[ $? -eq 1 ]] && export RSENV_SWAPPED=1 || unset RSENV_SWAPPED",
]

# Identifies an already-migrated file regardless of comment wording.
DERIVATION_SIGNATURE = "rsenv swap status --silent"


def content_hash(text: str) -> str:
    """8-char hash, matching rsenv's `application::hash::content_hash`."""
    return hashlib.sha256(text.encode()).hexdigest()[:8]


class Outcome:
    """What the migration would do to one dot.envrc."""

    def __init__(self, path: Path) -> None:
        self.path = path
        self.before = path.read_text()
        self.notes: list[str] = []
        self.after = self._migrate()

    def _migrate(self) -> str:
        lines = self.before.split("\n")

        kept = [
            ln
            for ln in lines
            if ln.strip() != LEGACY_MARKER
            and LEGACY_STATE_FILE_FRAGMENT not in ln
        ]
        removed = len(lines) - len(kept)
        if removed:
            self.notes.append(f"removed {removed} stale marker line(s)")

        # Only a TOP-LEVEL derivation counts: an indented one sits inside a shell
        # function and runs solely when that function is called, so direnv never
        # exports it and starship & co. never see it.
        if any(DERIVATION_SIGNATURE in ln and not ln[:1].isspace() for ln in kept):
            self.notes.append("top-level derivation already present")
            return "\n".join(kept)

        if not any("RSENV_SWAPPED" in ln for ln in kept):
            # Nothing here reads it - but it is still exported for starship and friends,
            # so only skip vaults that never carried a marker in the first place.
            if not removed:
                return "\n".join(kept)

        # Append at EOF, where the legacy marker lived: PATH is fully set up by then.
        at = len(kept)
        while at > 0 and not kept[at - 1].strip():
            at -= 1
        self.notes.append(f"appended derivation at line {at + 1}")
        return "\n".join(kept[:at] + DERIVATION + kept[at:])

    @property
    def changed(self) -> bool:
        return self.before != self.after


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--vaults-dir",
        type=Path,
        default=Path.home() / ".rsenv" / "vaults",
        help="directory holding the vaults (default: ~/.rsenv/vaults)",
    )
    parser.add_argument(
        "--apply", action="store_true", help="write the changes (default: dry run)"
    )
    args = parser.parse_args()

    envrcs = sorted(args.vaults_dir.glob("*/dot.envrc"))
    if not envrcs:
        print(f"no dot.envrc found under {args.vaults_dir}", file=sys.stderr)
        return 1

    changed: list[Outcome] = []

    for path in envrcs:
        outcome = Outcome(path)
        if not outcome.changed:
            continue
        changed.append(outcome)
        print(f"{path.parent.name}")
        print(f"  hash {content_hash(outcome.before)} -> {content_hash(outcome.after)}")
        for note in outcome.notes:
            print(f"  {note}")

    print()
    print(f"{len(changed)} of {len(envrcs)} vault(s) need migration")

    if not args.apply:
        print("dry run - re-run with --apply to write")
        return 0

    for outcome in changed:
        outcome.path.write_text(outcome.after)
    print(f"rewrote {len(changed)} file(s)")
    print("next: rsenv sops encrypt --global")
    return 0


if __name__ == "__main__":
    sys.exit(main())
