#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Render the documentation diagrams in doc/diagrams/*.svg.

Hand-placed SVG, no layout engine: each diagram is a function that returns its
elements. The shared style (colors for light and dark mode, fonts, shapes) lives
here once; every output file is self-contained, so GitHub can show it via <img>.

Usage: doc/diagrams/build.py          # rewrites all SVGs next to this file
"""

from html import escape
from pathlib import Path

OUT = Path(__file__).parent

STYLE = """
    :root {
      --bg: #ffffff; --ink: #1f2328; --muted: #59636e; --row: #f6f8fa;
      --proj: #0969da; --proj-bg: #ddf4ff; --proj-line: #54aeff;
      --vault: #8250df; --vault-bg: #fbefff; --vault-line: #c297ff;
      --ok: #1a7f37; --ok-bg: #dafbe1; --ok-line: #4ac26b;
      --warn: #9a6700; --warn-bg: #fff8c5; --warn-line: #d4a72c;
      --bad: #cf222e; --bad-bg: #ffebe9; --bad-line: #ff8182;
      --link: #bf3989;
    }
    @media (prefers-color-scheme: dark) {
      :root {
        --bg: #0d1117; --ink: #e6edf3; --muted: #9198a1; --row: #161b22;
        --proj: #4493f8; --proj-bg: #0c2d6b; --proj-line: #1f6feb;
        --vault: #ab7df8; --vault-bg: #271052; --vault-line: #8957e5;
        --ok: #3fb950; --ok-bg: #033a16; --ok-line: #238636;
        --warn: #d29922; --warn-bg: #3b2300; --warn-line: #9e6a03;
        --bad: #f85149; --bad-bg: #490202; --bad-line: #da3633;
        --link: #f778ba;
      }
    }
    text { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Helvetica, Arial, sans-serif; fill: var(--ink); font-size: 13px; }
    .mono { font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; }
    .title { font-size: 17px; font-weight: 600; }
    .path { font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; font-size: 12px; fill: var(--muted); }
    .note { font-size: 12px; fill: var(--muted); }
    .strong { font-weight: 600; }
    .bg { fill: var(--bg); }
    .row { fill: var(--row); }
    .proj { fill: var(--proj-bg); stroke: var(--proj-line); stroke-width: 1.5; }
    .vault { fill: var(--vault-bg); stroke: var(--vault-line); stroke-width: 1.5; }
    .ok { fill: var(--ok-bg); stroke: var(--ok-line); stroke-width: 1.5; }
    .warn { fill: var(--warn-bg); stroke: var(--warn-line); stroke-width: 1.5; }
    .bad { fill: var(--bad-bg); stroke: var(--bad-line); stroke-width: 1.5; }
    .ink-proj { fill: var(--proj); } .ink-vault { fill: var(--vault); } .ink-ok { fill: var(--ok); }
    .ink-warn { fill: var(--warn); } .ink-bad { fill: var(--bad); } .ink-link { fill: var(--link); }
    .ink-muted { fill: var(--muted); }
    .line { stroke: var(--muted); stroke-width: 1.5; fill: none; }
    .arrow { stroke: var(--link); stroke-width: 2.5; fill: none; }
    .arrow-proj { stroke: var(--proj); stroke-width: 2; fill: none; stroke-dasharray: 6 4; }
    .arrow-vault { stroke: var(--vault); stroke-width: 2; fill: none; }
    .head { fill: var(--link); } .head-proj { fill: var(--proj); } .head-vault { fill: var(--vault); }
    .head-muted { fill: var(--muted); }
    .pill { fill: var(--bg); stroke: var(--link); stroke-width: 1.5; }
    .pill-text { font-size: 13px; font-weight: 600; fill: var(--link); }
    .strike { text-decoration: line-through; }
"""


def t(x, y, s, cls="", anchor="start", style=""):
    st = f' style="{style}"' if style else ""
    return f'<text class="{cls}" x="{x}" y="{y}" text-anchor="{anchor}"{st}>{escape(s)}</text>'


def rect(x, y, w, h, cls, rx=8):
    return f'<rect class="{cls}" x="{x}" y="{y}" width="{w}" height="{h}" rx="{rx}"/>'


def card(x, y, w, h, kind, title, path=None, badge=None, badge_kind=None):
    """Rounded container with a colored title and an optional path and badge."""
    out = [rect(x, y, w, h, kind, rx=14), t(x + 20, y + 32, title, f"title ink-{kind}")]
    if path:
        out.append(t(x + 20, y + 52, path, "path"))
    if badge:
        bw = 9 + 6.6 * len(badge)
        bk = badge_kind or kind
        out.append(
            f'<rect x="{x + w - 16 - bw}" y="{y + 16}" width="{bw}" height="22" rx="11" '
            f'style="fill: var(--{bk})"/>'
        )
        out.append(
            t(x + w - 16 - bw / 2, y + 31, badge, "strong", "middle", "font-size: 11px; fill: var(--bg)")
        )
    return out


def row(x, y, w, name, note=None, kind="row", h=30, size=14, name_cls=""):
    """One file line in a card: monospace name left, muted note right."""
    out = [rect(x, y, w, h, kind)]
    out.append(t(x + 14, y + h / 2 + size * 0.36, name, f"mono {name_cls}", style=f"font-size: {size}px"))
    if note:
        out.append(t(x + w - 14, y + h / 2 + 4, note, "note", "end"))
    return out


def row2(x, y, w, name, note, kind="row", size=13):
    """Two-line file entry for names too long to share a line with their note."""
    return [
        rect(x, y, w, 44, kind),
        t(x + 14, y + 19, name, "mono", style=f"font-size: {size}px"),
        t(x + 14, y + 36, note, "note"),
    ]


def arrow(x1, y1, x2, y2, cls="arrow", head="head", curve=0):
    """Straight (or gently curved) arrow ending in a filled head at (x2, y2)."""
    import math

    ang = math.atan2(y2 - y1, x2 - x1)
    bx, by = x2 - 11 * math.cos(ang), y2 - 11 * math.sin(ang)
    if curve:
        mx, my = (x1 + bx) / 2, (y1 + by) / 2 + curve
        d = f"M{x1} {y1} Q {mx:.1f} {my:.1f} {bx:.1f} {by:.1f}"
    else:
        d = f"M{x1} {y1} L {bx:.1f} {by:.1f}"
    lx, ly = x2 - 12 * math.cos(ang - 0.45), y2 - 12 * math.sin(ang - 0.45)
    rx_, ry = x2 - 12 * math.cos(ang + 0.45), y2 - 12 * math.sin(ang + 0.45)
    return [
        f'<path class="{cls}" d="{d}"/>',
        f'<path class="{head}" d="M{x2} {y2} L {lx:.1f} {ly:.1f} L {rx_:.1f} {ry:.1f} z"/>',
    ]


def pill(cx, cy, label, cls="pill", text_cls="pill-text"):
    w = 22 + 7.3 * len(label)
    return [rect(cx - w / 2, cy - 14, w, 28, cls, rx=14), t(cx, cy + 5, label, text_cls, "middle")]


def svg(name, w, h, title, desc, body):
    parts = [
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" width="{w}" height="{h}" '
        f'role="img" aria-labelledby="t d">',
        f'  <title id="t">{escape(title)}</title>',
        f'  <desc id="d">{escape(desc)}</desc>',
        f"  <style>{STYLE}  </style>",
        f'  {rect(0, 0, w, h, "bg", rx=12)}',
    ]
    parts += [f"  {e}" for e in body]
    parts.append("</svg>\n")
    (OUT / f"{name}.svg").write_text("\n".join(parts))
    print(f"wrote {name}.svg")


# --------------------------------------------------------------------------- diagrams


def vault_link():
    b = card(24, 24, 300, 300, "proj", "Your project", "~/projects/myapp/", "git-tracked")
    b += row(44, 96, 260, ".envrc", "symlink", kind="warn", h=34)
    for i, n in enumerate(["src/", "Makefile", "docker-compose.yml", "config/database.yml"]):
        b += row(44, 138 + 36 * i, 260, n)
    b.append(t(44, 306, "minimal footprint: one symlink", "note"))
    b += card(556, 24, 300, 300, "vault", "The vault", "~/.rsenv/vaults/myapp-a1b2c3d4/")
    b += row(576, 96, 260, "dot.envrc", "the real .envrc", kind="warn", h=34)
    for i, (n, note) in enumerate(
        [("envs/", "local.env, prod.env"), ("guarded/", "secrets"), ("swap/", "dev overrides"), (".rsenv.toml", "vault config")]
    ):
        b += row(576, 138 + 36 * i, 260, n, note)
    b.append(t(576, 306, "outside project git: your personal workspace", "note"))
    b += arrow(304, 113, 576, 113)
    b += pill(440, 148, "the only trace in your repo")
    b.append(t(440, 356, "rsenv vault init moves .envrc into the vault as dot.envrc and leaves the symlink behind", "note", "middle"))
    svg("vault-link", 880, 380, "rsenv: project and vault",
        "The project keeps one symlink, .envrc, pointing to dot.envrc in the vault outside the project. "
        "The vault holds env files, guarded secrets, swap overrides and the vault config.", b)


def vault_concept():
    b = card(24, 24, 280, 236, "proj", "Your project", "git-tracked")
    b += row(44, 96, 240, ".envrc", "symlink", kind="warn", h=34)
    b.append(t(44, 160, "your shell, after direnv loads it:", "note"))
    b += row(44, 172, 240, "RSENV_VAULT=…", kind="row", size=13)
    b += card(536, 24, 320, 236, "vault", "Vault root", "~/.rsenv/vaults/myproject-a1b2c3d4/", "not in git", "vault")
    b += rect(556, 80, 280, 160, "row", rx=10), t(572, 104, "dot.envrc", "mono strong", style="font-size: 14px")
    b.append(t(572, 130, "# rsenv section (managed)", "mono ink-muted", style="font-size: 12px"))
    b.append(t(572, 150, "export RSENV_VAULT=…", "mono ink-vault", style="font-size: 12px"))
    b.append(t(572, 176, "# your original .envrc,", "mono ink-muted", style="font-size: 12px"))
    b.append(t(572, 194, "# kept as it was", "mono ink-muted", style="font-size: 12px"))
    b += arrow(284, 113, 556, 104)
    b += pill(420, 86, "symlink")
    b += arrow(284, 189, 556, 160, cls="arrow-proj", head="head-proj", curve=40)
    b.append(t(420, 220, "RSENV_VAULT points back to the vault", "note strong ink-proj", "middle"))
    b.append(t(440, 296, "One link, both directions: the symlink finds the vault's .envrc, RSENV_VAULT tells rsenv where the vault is.", "note", "middle"))
    svg("vault-concept", 880, 320, "The vault concept",
        "The project's .envrc is a symlink to dot.envrc in the vault. dot.envrc exports RSENV_VAULT, "
        "which points from the shell back to the vault root.", b)


def swap_states():
    b = card(24, 24, 300, 300, "proj", "Swapped out", "normal state")
    b.append(t(44, 100, "project/", "path"))
    b += row2(44, 108, 260, "docker-compose.yml", "official version")
    b.append(t(44, 184, "vault/swap/", "path"))
    b += row2(44, 192, 260, "docker-compose.yml", "your dev version", kind="vault")
    b.append(t(44, 290, "git sees the official file", "note"))
    b += card(456, 24, 400, 300, "ok", "Swapped in", "working")
    b.append(t(476, 100, "project/", "path"))
    b += row2(476, 108, 360, "docker-compose.yml", "your dev version, moved here", kind="vault")
    b.append(t(476, 184, "vault/swap/", "path"))
    b += row2(476, 192, 360, "docker-compose.yml.rsenv_original", "backup of the official version")
    b += row2(476, 242, 360, "docker-compose.yml@@HOST@@rsenv_active", "sentinel: which host swapped in", kind="warn", size=12)
    b += arrow(324, 110, 456, 110)
    b += pill(390, 88, "rsenv swap in")
    b += arrow(456, 230, 324, 230)
    b += pill(390, 252, "rsenv swap out")
    b.append(t(440, 356, "Swap out restores the official file; your changes to the dev version stay in the vault.", "note", "middle"))
    svg("swap-states", 880, 380, "Swap in and swap out",
        "Swapped out, the project has the official file and the vault holds your dev version. Swap in moves "
        "the dev version into the project, backs up the official file and writes a host sentinel. Swap out reverses it.", b)


def env_tree():
    nodes = {
        "base": (365, 24, "base.env", None, ["DB_HOST=localhost", "DB_PORT=5432", "LOG_LEVEL=info"], set()),
        "local": (40, 190, "local.env", "base.env", ["# local dev"], set()),
        "test": (215, 190, "test.env", "base.env", ["# testing"], set()),
        "cloud": (530, 190, "cloud.env", "base.env", ["DB_HOST=rds.aws.com"], {0}),
        "staging": (430, 340, "staging.env", "cloud.env", ["# staging"], set()),
        "prod": (640, 340, "prod.env", "cloud.env", ["LOG_LEVEL=error"], {0}),
    }
    W = 170
    b = []
    edges = [("base", "local"), ("base", "test"), ("base", "cloud"), ("cloud", "staging"), ("cloud", "prod")]
    heights = {}
    for k, (x, y, name, parent, lines, _) in nodes.items():
        heights[k] = 46 + 20 * len(lines) + (20 if parent else 0)
    for a, c in edges:
        ax, ay = nodes[a][0] + W / 2, nodes[a][1] + heights[a]
        cx, cy = nodes[c][0] + W / 2, nodes[c][1]
        b.append(f'<path class="line" d="M{ax} {ay} C {ax} {ay + 40}, {cx} {cy - 40}, {cx} {cy - 8}"/>')
        b.append(f'<path class="head-muted" d="M{cx} {cy} l -5 -9 h 10 z"/>')
    for k, (x, y, name, parent, lines, overrides) in nodes.items():
        kind = "vault" if k == "base" else "proj"
        b.append(rect(x, y, W, heights[k], kind, rx=10))
        b.append(t(x + 14, y + 26, name, f"mono strong ink-{kind}", style="font-size: 14px"))
        yy = y + 48
        if parent:
            b.append(t(x + 14, yy, f"# rsenv: {parent}", "mono ink-muted", style="font-size: 12px"))
            yy += 20
        for i, line in enumerate(lines):
            cls = "mono ink-bad strong" if i in overrides else ("mono ink-muted" if line.startswith("#") else "mono")
            b.append(t(x + 14, yy, line, cls, style="font-size: 12px"))
            yy += 20
    b.append(t(24, 470, "first line # rsenv: parent.env links a child to its parent;", "note"))
    b.append(t(24, 488, "red values override what the parent set", "note ink-bad"))
    svg("env-tree", 880, 504, "Environment hierarchy",
        "base.env is the root. local.env, test.env and cloud.env name base.env as parent; staging.env and "
        "prod.env name cloud.env. cloud.env overrides DB_HOST, prod.env overrides LOG_LEVEL.", b)


def env_merge():
    b = [t(24, 40, "rsenv env build prod.env", "mono strong", style="font-size: 15px")]
    chain = [("prod.env", "proj"), ("cloud.env", "proj"), ("base.env", "vault")]
    for i, (n, k) in enumerate(chain):
        y = 64 + 70 * i
        b.append(rect(24, y, 170, 42, k, rx=10))
        b.append(t(109, y + 26, n, f"mono strong ink-{k}", "middle", "font-size: 14px"))
        if i < 2:
            b += arrow(109, y + 42, 109, y + 70, cls="line", head="head-muted")
            b.append(t(122, y + 60, "inherits", "note"))
    b += arrow(214, 140, 330, 140)
    b += pill(272, 116, "merge")
    b += card(340, 56, 516, 190, "ok", "Merged output", "child wins")
    vals = [("export DB_HOST=rds.aws.com", "cloud.env", "proj"),
            ("export DB_PORT=5432", "base.env", "vault"),
            ("export LOG_LEVEL=error", "prod.env", "proj")]
    for i, (v, src, k) in enumerate(vals):
        y = 120 + 38 * i
        b += row(360, y, 476, v, None, size=13)
        b.append(rect(726, y + 5, 96, 20, k, rx=10))
        b.append(t(774, y + 19, src, f"mono ink-{k}", "middle", "font-size: 11px"))
    svg("env-merge", 880, 270, "Building an environment",
        "rsenv env build prod.env follows prod.env to cloud.env to base.env and merges them; the nearest "
        "file wins: DB_HOST from cloud.env, DB_PORT from base.env, LOG_LEVEL from prod.env.", b)


def guard():
    b = card(24, 24, 250, 170, "bad", "Before", "project/")
    b += row2(44, 84, 210, "config/secrets.yaml", "real file")
    b.append(t(44, 166, "git tracks the secret", "note strong ink-bad"))
    b += card(334, 24, 250, 170, "ok", "After", "project/")
    b += row2(354, 84, 210, "config/secrets.yaml", "symlink", kind="warn")
    b.append(t(354, 166, "git tracks a harmless symlink", "note strong ink-ok"))
    b += card(606, 24, 250, 170, "vault", "Vault", "vault/guarded/")
    b += row2(626, 84, 210, "config/secrets.yaml", "real file")
    b.append(t(626, 166, "safe, outside git", "note strong ink-vault"))
    b += arrow(274, 106, 334, 106)
    b += arrow(564, 106, 626, 106)
    b.append('<path class="line" d="M304 222 V 112" style="stroke-dasharray: 3 3"/>')
    b += pill(304, 236, "rsenv guard add config/secrets.yaml")
    svg("guard", 880, 270, "Guarding a file",
        "Before, git tracks the real secrets file. rsenv guard add moves it into the vault's guarded "
        "directory and leaves a symlink in the project, so git only tracks the symlink.", b)


def sops_hash():
    b = []
    b += row(24, 40, 170, "secrets.env", kind="proj", h=40)
    b += arrow(194, 60, 330, 60)
    b += pill(262, 38, "encrypt")
    b.append(rect(330, 40, 290, 40, "ok", rx=8))
    b.append(t(346, 66, "secrets.env.", "mono", style="font-size: 15px"))
    b.append(t(453, 66, "a1b2c3d4", "mono strong ink-link", style="font-size: 15px"))
    b.append(t(526, 66, ".enc", "mono", style="font-size: 15px"))
    b.append(f'<path class="line" d="M455 86 v 8 h 72 v -8"/>')
    b.append(t(491, 112, "first 8 hex of SHA-256(plaintext)", "note", "middle"))
    b += row(24, 160, 170, "secrets.env", "edited", kind="warn", h=40)
    b += arrow(194, 180, 300, 180)
    b.append(rect(300, 162, 150, 36, "warn", rx=18))
    b.append(t(375, 185, "status: stale", "strong ink-warn", "middle"))
    b += arrow(450, 180, 560, 180)
    b += pill(505, 158, "re-encrypt")
    b.append(rect(560, 160, 296, 40, "ok", rx=8))
    b.append(t(576, 186, "secrets.env.", "mono", style="font-size: 15px"))
    b.append(t(683, 186, "f9e8d7c6", "mono strong ink-link", style="font-size: 15px"))
    b.append(t(756, 186, ".enc", "mono", style="font-size: 15px"))
    b.append(t(708, 222, "secrets.env.a1b2c3d4.enc is removed", "note strike", "middle"))
    b.append(t(24, 262, "The hash in the name is the staleness check: no key is needed to see that a ciphertext is out of date.", "note"))
    svg("sops-hash", 880, 284, "Content-addressed encryption",
        "Encrypting secrets.env writes secrets.env.a1b2c3d4.enc, where a1b2c3d4 is a SHA-256 prefix of the "
        "plaintext. Editing the plaintext changes the hash, status reports stale, and re-encrypting writes a new name.", b)


def workflow():
    steps = [
        ("rsenv vault init", "create vault, link via .envrc symlink", "vault", "setup"),
        ("rsenv env select", "pick environment, export variables", "proj", "setup"),
        ("rsenv guard add .env", "move secrets to vault (permanent)", "bad", "setup"),
        ("rsenv swap in", "swap in dev overrides (temporary)", "ok", "session"),
        ("rsenv sops encrypt", "encrypt vault at rest", "warn", "session"),
        (None, "… work …", None, None),
        ("rsenv swap out", "restore originals, no traces", "ok", "session"),
        ("rsenv sops encrypt", "re-encrypt if changed", "warn", "session"),
    ]
    b = []
    x0, y0, dy = 64, 40, 52
    b.append(f'<path class="line" d="M{x0} {y0} V {y0 + dy * (len(steps) - 1)}"/>')
    n = 0
    for i, (cmd, desc, kind, _) in enumerate(steps):
        y = y0 + dy * i
        if cmd is None:
            b.append(rect(x0 - 40, y - 14, 760, 28, "row", rx=14))
            b.append(t(x0 + 36, y + 5, desc, "note strong"))
            continue
        n += 1
        b.append(f'<circle class="{kind}" cx="{x0}" cy="{y}" r="16"/>')
        b.append(t(x0, y + 5, str(n), f"strong ink-{kind}", "middle"))
        b.append(t(x0 + 36, y + 5, cmd, "mono strong", style="font-size: 14px"))
        b.append(t(x0 + 260, y + 5, desc, "note"))
    svg("workflow", 880, y0 + dy * (len(steps) - 1) + 36, "rsenv workflow",
        "1 rsenv vault init, 2 rsenv env select, 3 rsenv guard add, 4 rsenv swap in, 5 rsenv sops encrypt, "
        "then work, 6 rsenv swap out, 7 rsenv sops encrypt.", b)


def backup():
    b = card(24, 24, 520, 300, "vault", "~/.rsenv", "one git repo, covering every vault")
    b.append(t(44, 100, "vaults/myproject-abc123/", "path"))
    rows = [
        ("envs/local.env", "gitignored", "bad", "ink-muted"),
        ("envs/local.env.a1b2c3d4.enc", "committed", "ok", ""),
        ("guarded/secrets.yaml.b9c0d1e2.enc", "committed", "ok", ""),
        ("swap/docker-compose.yml", "committed", "ok", ""),
        ("dot.envrc.f3a4b5c6.enc", "committed", "ok", ""),
    ]
    for i, (n, note, k, cls) in enumerate(rows):
        y = 110 + 38 * i
        b += row(44, y, 480, n, None, size=13, name_cls=cls)
        b.append(rect(412, y + 6, 98, 18, k, rx=9))
        b.append(t(461, y + 19, note, f"ink-{k} strong", "middle", "font-size: 11px"))
    b += arrow(544, 174, 700, 174)
    b += pill(622, 148, "commit --push")
    b.append('<path class="ok" d="M708 150 a 70 14 0 0 1 140 0 v 56 a 70 14 0 0 1 -140 0 z"/>')
    b.append('<path class="ok" d="M708 150 a 70 14 0 0 0 140 0" style="fill: none"/>')
    b.append(t(778, 192, "remote", "strong ink-ok", "middle"))
    b.append(t(778, 210, "your backup", "note", "middle"))
    b.append(t(44, 352, "Plaintext never leaves the machine: the gitignore keeps it out, the pre-commit hook blocks stale ciphertext.", "note"))
    svg("backup", 880, 372, "What the vault repo backs up",
        "~/.rsenv is one git repo for all vaults. Ciphertexts, swap files and the encrypted dot.envrc are "
        "committed and pushed to a remote; plaintext env files are gitignored and never committed.", b)


if __name__ == "__main__":
    for f in (vault_link, vault_concept, swap_states, env_tree, env_merge, guard, sops_hash, workflow, backup):
        f()
