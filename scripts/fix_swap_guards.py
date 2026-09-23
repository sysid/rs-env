#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""One-shot migration: stop branching on the cached RSENV_SWAPPED, ask rsenv instead.

Why
---
`RSENV_SWAPPED` is a snapshot taken at the last direnv evaluation. rsenv now bumps the
vault `dot.envrc`'s mtime on every swap, so the snapshot refreshes at the next prompt of
every shell in the project - good enough for a prompt icon, not good enough for a guard:
a shell that has not reached a prompt yet, or a script that captured the environment
earlier, still holds the stale value. A stale `1` makes

    if [[ "${RSENV_SWAPPED}" != "1" && ( -d "thoughts" || -d "specs" ) ]]; then

pass exactly when it should fire - the project is swapped OUT while `thoughts/` still
exists, which is the state the guard exists to catch.

This script rewrites those guards to ask rsenv for the live state:

    if ! rsenv_swapped_in && [[ -d "thoughts" || -d "specs" ]]; then

`rsenv_swapped_in` lives in `$TW_BINX/lib/sane_fn.sh`. Sourcing it at the top of
`dot.envrc` is not enough: `export_function swapout` makes direnv write the body out to
`.direnv/funcs/swapout`, a standalone `#!/usr/bin/env bash` script on PATH. That runs in a
fresh process where nothing else from `dot.envrc` exists, so the helper would be
"command not found" (127). The script therefore also inserts the `source` line inside
every function that calls the helper.

`RSENV_SWAPPED` itself stays: it is still the right thing for display-only consumers such
as starship, which cannot afford a subprocess per prompt.

Usage
-----
    ./scripts/fix_swap_guards.py              # dry run, prints the plan
    ./scripts/fix_swap_guards.py --apply      # rewrite the files
    rsenv sops encrypt --global               # re-encrypt afterwards

Dry run is the default: dot.envrc holds plaintext secrets and is gitignored, so an
unwanted rewrite has no git safety net. Every rewrite is checked with `bash -n` before it
is written - a syntax error here breaks every shell that enters the project.
"""

from __future__ import annotations

import argparse
import hashlib
import re
import subprocess
import sys
import tempfile
from pathlib import Path

HELPER = "rsenv_swapped_in"

# The helper's home. Sourced by dot.envrc, so the guard can call it.
HELPER_SOURCE = "sane_fn.sh"

# Variant A (tab-indented, one test with a parenthesised group) and variant B (two
# chained tests). Both capture the indent and the surviving directory test verbatim, so
# a vault guarding something other than thoughts/specs migrates unchanged in substance.
GUARDS = (
    re.compile(
        r'^(?P<indent>\s*)if \[\[ "\$\{RSENV_SWAPPED\}" != "1" && \( (?P<rest>.+?) \) \]\]; then$'
    ),
    re.compile(
        r'^(?P<indent>\s*)if \[\[ "\$\{RSENV_SWAPPED\}" != "1" \]\] && \[\[ (?P<rest>.+?) \]\]; then$'
    ),
)

# Shell function definitions, matched at top level only (`name() {` / `}`).
FUNCTION_OPEN = re.compile(r"^(?P<name>[A-Za-z_][A-Za-z0-9_-]*)\(\)\s*\{\s*$")
FUNCTION_CLOSE = re.compile(r"^\}\s*$")

# Anything else that branches on the variable - reported, never rewritten blind.
UNHANDLED = re.compile(r"RSENV_SWAPPED")

# Lines that legitimately mention it: the EOF derivation (which produces the variable)
# and comments.
DERIVATION_SIGNATURE = "rsenv swap status --silent"


def content_hash(text: str) -> str:
    """8-char hash, matching rsenv's `application::hash::content_hash`."""
    return hashlib.sha256(text.encode()).hexdigest()[:8]


def bash_syntax_ok(text: str) -> tuple[bool, str]:
    """`bash -n` the candidate content. These files are sourced by every shell."""
    with tempfile.NamedTemporaryFile("w", suffix=".envrc", delete=True) as tmp:
        tmp.write(text)
        tmp.flush()
        result = subprocess.run(
            ["bash", "-n", tmp.name], capture_output=True, text=True
        )
    return result.returncode == 0, result.stderr.strip()


class Outcome:
    """What the migration would do to one dot.envrc."""

    def __init__(self, path: Path) -> None:
        self.path = path
        self.before = path.read_text()
        self.notes: list[str] = []
        self.blocked: str | None = None
        self.after = self._migrate()

    def _migrate(self) -> str:
        lines = self.before.split("\n")

        out: list[str] = []
        migrated = 0
        for ln in lines:
            new = self._rewrite(ln)
            if new is not None:
                migrated += 1
                out.append(new)
            else:
                out.append(ln)

        if migrated:
            self.notes.append(f"rewrote {migrated} guard(s)")

        out = self._source_helper_in_functions(out)
        self._report_leftovers(out)
        return "\n".join(out)

    def _source_helper_in_functions(self, lines: list[str]) -> list[str]:
        """Source sane_fn.sh inside every function that calls the helper.

        `export_function` writes the body out to `.direnv/funcs/<name>`, a standalone
        `#!/usr/bin/env bash` script that direnv puts on PATH. It runs in a fresh
        process, so nothing else from dot.envrc - including the top-level
        `source .../sane_fn.sh` - is in scope, and the helper would be a
        "command not found" returning 127. Each function has to source it itself.
        """
        out: list[str] = []
        i = 0
        while i < len(lines):
            line = lines[i]
            out.append(line)
            i += 1

            opener = FUNCTION_OPEN.match(line)
            if not opener:
                continue

            end = next(
                (j for j in range(i, len(lines)) if FUNCTION_CLOSE.match(lines[j])),
                len(lines),
            )
            body = lines[i:end]
            if not any(HELPER in ln for ln in body):
                continue
            if any(HELPER_SOURCE in ln for ln in body):
                continue

            indent = next(
                (
                    ln[: len(ln) - len(ln.lstrip())]
                    for ln in body
                    if ln.strip() and not ln.strip().startswith("#")
                ),
                "    ",
            )
            out.append(f'{indent}source "$TW_BINX/lib/{HELPER_SOURCE}"')
            self.notes.append(
                f"{opener.group('name')}(): sourced {HELPER_SOURCE} for the "
                f"exported-function context"
            )

        return out

    @staticmethod
    def _rewrite(line: str) -> str | None:
        for pattern in GUARDS:
            match = pattern.match(line)
            if match:
                return (
                    f"{match.group('indent')}if ! {HELPER} "
                    f"&& [[ {match.group('rest')} ]]; then"
                )
        return None

    def _report_leftovers(self, lines: list[str]) -> None:
        """Surface every remaining mention, so nothing migrates silently by halves."""
        for i, ln in enumerate(lines, start=1):
            if not UNHANDLED.search(ln):
                continue
            stripped = ln.strip()
            if stripped.startswith("#") or DERIVATION_SIGNATURE in ln:
                continue
            if "export RSENV_SWAPPED=1" in ln and DERIVATION_SIGNATURE in "\n".join(
                lines
            ):
                continue  # the derivation's own export line
            self.notes.append(f"left alone, check by hand: line {i}: {stripped}")

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
    blocked: list[Outcome] = []

    for path in envrcs:
        outcome = Outcome(path)
        if outcome.blocked:
            blocked.append(outcome)
            continue
        if not outcome.changed:
            continue

        ok, err = bash_syntax_ok(outcome.after)
        if not ok:
            outcome.blocked = f"bash -n rejected the result: {err}"
            blocked.append(outcome)
            continue

        changed.append(outcome)
        print(f"{path.parent.name}")
        print(f"  hash {content_hash(outcome.before)} -> {content_hash(outcome.after)}")
        for note in outcome.notes:
            print(f"  {note}")

    for outcome in blocked:
        print(f"{outcome.path.parent.name}")
        print(f"  SKIPPED: {outcome.blocked}")

    print()
    print(f"{len(changed)} of {len(envrcs)} vault(s) need migration", end="")
    print(f", {len(blocked)} skipped" if blocked else "")

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
