#!/usr/bin/env python3
"""Fail on characters that masquerade as ASCII letters or hide entirely.

A Cyrillic `а` (U+0430) renders identically to a Latin `a` in every editor,
diff and code-review UI. Substituted into an identifier, a package name or a
fixture it produces a value that looks correct and is not — and no amount of
reading catches it, because there is nothing to see.

This is not hypothetical here: milestone 926 introduced a Cyrillic `а` into a
test fixture's comment. It was found by an explicit non-ASCII sweep, not by
review. This check exists so the next one fails the build instead.

## What is flagged

1. **Latin-confusable letters** from other scripts — Cyrillic and Greek
   characters whose glyphs are indistinguishable from ASCII letters.
2. **Invisible and zero-width characters** — they can hide inside an
   identifier or split a word with nothing rendered.
3. **Non-breaking space** — renders as a space, is not one. In shell or YAML
   it silently changes the parse.
4. **Fullwidth forms** — `ａ` is not `a`.

## What is NOT flagged

Mathematical and typographic characters that are used deliberately and are
not confusable with any ASCII letter: `Δ`, `Σ`, `μ`, `π`, `λ`, `—`, `→`,
`✅`, box drawing, accented Latin in prose. The repository uses all of these
on purpose, and flagging them would train people to ignore this check.

The test is confusability with ASCII, not non-ASCII-ness.

Usage:
    python3 scripts/check-homoglyphs.py            # scan tracked files
    python3 scripts/check-homoglyphs.py PATH...    # scan specific paths

Exit status: 0 clean, 1 findings, 2 harness error.
"""

from __future__ import annotations

import subprocess
import sys
import unicodedata

# Cyrillic characters that render as ASCII letters. Curated rather than
# derived, so the set is auditable and cannot quietly widen.
CYRILLIC_CONFUSABLES = (
    "а"  # а -> a        А А -> A
    "А"
    "вВ"  # в В -> b B (В is exact)
    "еЕ"  # е Е -> e E
    "ѕЅ"  # ѕ Ѕ -> s S
    "іІ"  # і І -> i I
    "јЈ"  # ј Ј -> j J
    "кК"  # к К -> k K
    "мМ"  # м М -> m M
    "нН"  # н Н -> h H
    "оО"  # о О -> o O
    "рР"  # р Р -> p P
    "сС"  # с С -> c C
    "тТ"  # т Т -> t T
    "уУ"  # у У -> y Y
    "хХ"  # х Х -> x X
    "һ"  # һ -> h
    "ӏ"  # ӏ -> l
    "ԛ"  # ԛ -> q
    "ԝ"  # ԝ -> w
)

# Greek characters that render as ASCII letters. Note the deliberate
# omissions: Δ Σ μ π λ Ω are NOT here, because none is confusable with an
# ASCII letter and all are used as notation in this repository.
GREEK_CONFUSABLES = (
    "οΟ"  # ο Ο -> o O
    "Α"  # Α -> A
    "Β"  # Β -> B
    "Ε"  # Ε -> E
    "Ζ"  # Ζ -> Z
    "Η"  # Η -> H
    "Ι"  # Ι -> I
    "Κ"  # Κ -> K
    "Μ"  # Μ -> M
    "Ν"  # Ν -> N
    "Ρ"  # Ρ -> P
    "Τ"  # Τ -> T
    "Υ"  # Υ -> Y
    "Χ"  # Χ -> X
    "α"  # α -> a (in identifier contexts)
    "γ"  # γ -> y
    "ι"  # ι -> i
    "ν"  # ν -> v
    "ρ"  # ρ -> p
    "υ"  # υ -> u
    "χ"  # χ -> x
)

CONFUSABLE_LETTERS = set(CYRILLIC_CONFUSABLES + GREEK_CONFUSABLES)

# Invisible, zero-width, and deceptive whitespace.
INVISIBLE = {
    " ": "NO-BREAK SPACE",
    "​": "ZERO WIDTH SPACE",
    "‌": "ZERO WIDTH NON-JOINER",
    "‍": "ZERO WIDTH JOINER",
    "‎": "LEFT-TO-RIGHT MARK",
    "‏": "RIGHT-TO-LEFT MARK",
    " ": "LINE SEPARATOR",
    " ": "PARAGRAPH SEPARATOR",
    "‪": "LEFT-TO-RIGHT EMBEDDING",
    "‫": "RIGHT-TO-LEFT EMBEDDING",
    "‬": "POP DIRECTIONAL FORMATTING",
    "‭": "LEFT-TO-RIGHT OVERRIDE",
    "‮": "RIGHT-TO-LEFT OVERRIDE",
    "⁠": "WORD JOINER",
    "﻿": "ZERO WIDTH NO-BREAK SPACE (BOM)",
}

# Binary and generated content: scanning these is noise, and a homoglyph in
# a PNG is not a homoglyph.
SKIP_SUFFIXES = (
    ".png", ".jpg", ".jpeg", ".gif", ".ico", ".pdf", ".zip", ".gz", ".tar",
    ".woff", ".woff2", ".ttf", ".otf", ".bin", ".so", ".dylib", ".a", ".rlib",
    ".deb", ".rpm", ".apk", ".ipk", ".whl", ".jar", ".class", ".wasm",
)


def is_fullwidth(ch: str) -> bool:
    return 0xFF01 <= ord(ch) <= 0xFF5E


def classify(ch: str) -> str | None:
    if ch in INVISIBLE:
        return f"invisible ({INVISIBLE[ch]})"
    if ch in CONFUSABLE_LETTERS:
        script = "Cyrillic" if ch in CYRILLIC_CONFUSABLES else "Greek"
        return f"{script} letter confusable with ASCII"
    if is_fullwidth(ch):
        return "fullwidth form confusable with ASCII"
    return None


def tracked_files() -> list[str]:
    out = subprocess.run(
        ["git", "ls-files"], capture_output=True, text=True, check=True
    ).stdout
    return [p for p in out.splitlines() if p]


def scan(paths: list[str]) -> list[tuple]:
    findings = []
    for path in paths:
        if path.endswith(SKIP_SUFFIXES):
            continue
        try:
            with open(path, encoding="utf-8") as fh:
                text = fh.read()
        except (OSError, UnicodeDecodeError):
            continue  # binary or unreadable: not our concern
        for lineno, line in enumerate(text.splitlines(), 1):
            for col, ch in enumerate(line, 1):
                why = classify(ch)
                if why:
                    findings.append((path, lineno, col, ch, why, line))
    return findings


def main(argv: list[str]) -> int:
    try:
        paths = argv[1:] or tracked_files()
    except subprocess.CalledProcessError:
        print("check-homoglyphs: not a git repository", file=sys.stderr)
        return 2

    findings = scan(paths)
    if not findings:
        print(f"check-homoglyphs: clean ({len(paths)} files scanned)")
        return 0

    print(f"check-homoglyphs: {len(findings)} finding(s)\n", file=sys.stderr)
    for path, lineno, col, ch, why, line in findings:
        name = unicodedata.name(ch, "<unnamed>")
        print(f"{path}:{lineno}:{col}: U+{ord(ch):04X} {name}", file=sys.stderr)
        print(f"    {why}", file=sys.stderr)
        print(f"    {line.strip()[:100]}", file=sys.stderr)
        caret = " " * (min(col, 100) + 3) + "^"
        print(caret, file=sys.stderr)
    print(
        "\nThese characters render as ASCII but are not ASCII. Replace them "
        "with the ASCII character they imitate.\n"
        "Mathematical notation (Δ, Σ, μ, π, λ) and typography (—, →, ✅) are "
        "NOT flagged and need no change.",
        file=sys.stderr,
    )
    return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
