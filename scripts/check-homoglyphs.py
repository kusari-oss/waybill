#!/usr/bin/env python3
"""Fail on characters that masquerade as ASCII letters or hide entirely.

A Cyrillic U+0430 renders identically to a Latin `a` in every editor,
diff and code-review UI. Substituted into an identifier, a package name or a
fixture it produces a value that looks correct and is not — and no amount of
reading catches it, because there is nothing to see.

This is not hypothetical here: milestone 926 introduced a Cyrillic U+0430 into a
test fixture's comment. It was found by an explicit non-ASCII sweep, not by
review. This check exists so the next one fails the build instead.

## What is flagged

1. **Latin-confusable letters** from other scripts — Cyrillic and Greek
   characters whose glyphs are indistinguishable from ASCII letters.
2. **Invisible and zero-width characters** — they can hide inside an
   identifier or split a word with nothing rendered.
3. **Non-breaking space** — renders as a space, is not one. In shell or YAML
   it silently changes the parse.
4. **Fullwidth forms** - U+FF41 is not `a`.

## What is NOT flagged

Mathematical and typographic characters that are used deliberately and are
not confusable with any ASCII letter: DELTA, SIGMA, MU, PI, LAMBDA, em-dash,
arrows, check marks, box drawing, accented Latin in prose. The repository uses all of these
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

# Characters are declared by CODE POINT, never as literals.
#
# This file must contain no confusable character of its own, or it would
# flag itself on every run. The obvious alternative — excluding this file
# from its own scan — carves out exactly the kind of hole the check exists
# to close, and would let a real homoglyph hide in the one file nobody
# re-reads. Escapes cost a little readability and leave no exemption.
#
# Each entry is `codepoint: the ASCII character it imitates`.

# Cyrillic letters whose glyphs render as ASCII letters.
CYRILLIC_CONFUSABLES = {
    0x0430: "a", 0x0410: "A",
    0x0432: "b", 0x0412: "B",
    0x0435: "e", 0x0415: "E",
    0x0455: "s", 0x0405: "S",
    0x0456: "i", 0x0406: "I",
    0x0458: "j", 0x0408: "J",
    0x043A: "k", 0x041A: "K",
    0x043C: "m", 0x041C: "M",
    0x043D: "h", 0x041D: "H",
    0x043E: "o", 0x041E: "O",
    0x0440: "p", 0x0420: "P",
    0x0441: "c", 0x0421: "C",
    0x0442: "t", 0x0422: "T",
    0x0443: "y", 0x0423: "Y",
    0x0445: "x", 0x0425: "X",
    0x04BB: "h",
    0x04CF: "l",
    0x051B: "q",
    0x051D: "w",
}

# Greek letters whose glyphs render as ASCII letters.
#
# Deliberate omissions: DELTA (0x0394), SIGMA (0x03A3), MU (0x03BC),
# PI (0x03C0), LAMBDA (0x03BB), OMEGA (0x03A9). None is confusable with an
# ASCII letter, and all are used as notation in this repository. A check
# that flags legitimate notation trains people to ignore it.
GREEK_CONFUSABLES = {
    0x03BF: "o", 0x039F: "O",
    0x0391: "A",
    0x0392: "B",
    0x0395: "E",
    0x0396: "Z",
    0x0397: "H",
    0x0399: "I",
    0x039A: "K",
    0x039C: "M",
    0x039D: "N",
    0x03A1: "P",
    0x03A4: "T",
    0x03A5: "Y",
    0x03A7: "X",
    0x03B1: "a",
    0x03B3: "y",
    0x03B9: "i",
    0x03BD: "v",
    0x03C1: "p",
    0x03C5: "u",
    0x03C7: "x",
}

CONFUSABLE_LETTERS = {**CYRILLIC_CONFUSABLES, **GREEK_CONFUSABLES}

# Invisible, zero-width, and deceptive whitespace, by code point.
INVISIBLE = {
    0x00A0: "NO-BREAK SPACE",
    0x200B: "ZERO WIDTH SPACE",
    0x200C: "ZERO WIDTH NON-JOINER",
    0x200D: "ZERO WIDTH JOINER",
    0x200E: "LEFT-TO-RIGHT MARK",
    0x200F: "RIGHT-TO-LEFT MARK",
    0x2028: "LINE SEPARATOR",
    0x2029: "PARAGRAPH SEPARATOR",
    0x202A: "LEFT-TO-RIGHT EMBEDDING",
    0x202B: "RIGHT-TO-LEFT EMBEDDING",
    0x202C: "POP DIRECTIONAL FORMATTING",
    0x202D: "LEFT-TO-RIGHT OVERRIDE",
    0x202E: "RIGHT-TO-LEFT OVERRIDE",
    0x2060: "WORD JOINER",
    0xFEFF: "ZERO WIDTH NO-BREAK SPACE (BOM)",
}

# Binary and generated content: scanning these is noise, and a homoglyph in
# a PNG is not a homoglyph.
SKIP_SUFFIXES = (
    ".png", ".jpg", ".jpeg", ".gif", ".ico", ".pdf", ".zip", ".gz", ".tar",
    ".woff", ".woff2", ".ttf", ".otf", ".bin", ".so", ".dylib", ".a", ".rlib",
    ".deb", ".rpm", ".apk", ".ipk", ".whl", ".jar", ".class", ".wasm",
)


def classify(ch: str) -> str | None:
    cp = ord(ch)
    if cp < 0x80:
        return None  # plain ASCII, the overwhelmingly common case
    if cp in INVISIBLE:
        return f"invisible ({INVISIBLE[cp]})"
    if cp in CONFUSABLE_LETTERS:
        script = "Cyrillic" if cp in CYRILLIC_CONFUSABLES else "Greek"
        return f"{script} letter imitating ASCII {CONFUSABLE_LETTERS[cp]!r}"
    if 0xFF01 <= cp <= 0xFF5E:
        return f"fullwidth form imitating ASCII {chr(cp - 0xFEE0)!r}"
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
        "\nThese characters render as ASCII but are not ASCII. Replace each "
        "with the ASCII character it imitates.\n"
        "Mathematical notation and typography are NOT flagged and need no "
        "change - if you are seeing this, the character really is wrong.",
        file=sys.stderr,
    )
    return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
