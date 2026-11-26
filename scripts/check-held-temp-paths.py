#!/usr/bin/env python3
"""Reject a temporary file or directory dropped in the statement that takes its path.

    let out = tempfile::NamedTempFile::new().unwrap().path().to_path_buf();

The guard is dropped at the `;`, which deletes the file and keeps only its
name, so nothing reserves the path. Tests run under nextest (spec 1658),
one process per test and concurrently, and an unreserved output path is a
resource two tests can end up sharing (#1727, spec 1660). Bind the guard and
keep it alive until the output has been read:

    let out_dir = tempfile::tempdir().unwrap();
    let out = out_dir.path().join("out.json");

Allowed:
- `.keep()` and `.persist(`, which persist the file deliberately;
- `.path()` inside the constructor's own arguments, as in `new_in(tmp.path())`;
- a `// held-temp-path: <reason>` comment on the line before.

Run by scripts/pre-pr.sh and by CI, the same script in both, so they can't
drift (#1111). Standard library only. Run with `python3 -I -B`.

    python3 -I -B scripts/check-held-temp-paths.py              # scan the workspace
    python3 -I -B scripts/check-held-temp-paths.py --self-test  # built-in controls

Exit status: 0 when clean, 1 on violations, 2 on usage error.
"""

import pathlib
import re
import sys

ROOTS = ("waybill-cli", "waybill-common", "waybill-ebpf", "xtask")
MARKER = "held-temp-path:"

# Constructors whose result is a guard that deletes on drop. The match ends
# just before the constructor's opening parenthesis.
CTOR = re.compile(
    r"\b(?:tempfile::)?(?:NamedTempFile::new(?:_in)?|TempDir::new(?:_in)?|tempdir(?:_in)?)\s*(?=\()"
    r"|\b(?:tempfile::)?Builder::new\s*\(\s*\)(?:\s*\.\s*\w+\s*\([^;]*?\))*?\s*\.\s*(?:tempfile|tempdir)(?:_in)?\s*(?=\()"
)
# Calls that unwrap the constructor's Result without changing the guard.
UNWRAP = re.compile(r"\s*(?:\.\s*(?:unwrap|expect|unwrap_or_else)\s*\(|\?)")


def _skip_string(src: str, i: int) -> int:
    """`src[i]` is `"`; return the index after the closing quote."""
    i += 1
    while i < len(src):
        if src[i] == "\\":
            i += 2
            continue
        if src[i] == '"':
            return i + 1
        i += 1
    return i


def _match_paren(src: str, i: int) -> int:
    """`src[i]` is `(`; return the index after its matching `)`."""
    depth = 0
    while i < len(src):
        c = src[i]
        if c == '"':
            i = _skip_string(src, i)
            continue
        if c == "(":
            depth += 1
        elif c == ")":
            depth -= 1
            if depth == 0:
                return i + 1
        i += 1
    return i


def _chain_after(src: str, i: int) -> str:
    """The method chain from `i` to the end of the statement (depth-0 `;`, `,` or a closer)."""
    out = []
    depth = 0
    while i < len(src):
        c = src[i]
        if c == '"':
            j = _skip_string(src, i)
            out.append(src[i:j])
            i = j
            continue
        if c in "([{":
            depth += 1
        elif c in ")]}":
            if depth == 0:
                break
            depth -= 1
        elif c in ";," and depth == 0:
            break
        out.append(c)
        i += 1
    return "".join(out)


def violations(src: str):
    lines = src.splitlines()
    for m in CTOR.finditer(src):
        start = m.start()
        line_no = src.count("\n", 0, start) + 1
        # Skip matches inside comments on that line.
        line = lines[line_no - 1]
        col = start - (src.rfind("\n", 0, start) + 1)
        if "//" in line[:col]:
            continue
        args_end = _match_paren(src, m.end())
        chain = _chain_after(src, args_end)
        # Strip the Result-unwrapping calls, which keep the guard.
        rest = chain
        while True:
            u = UNWRAP.match(rest)
            if not u:
                break
            if u.group(0).strip() == "?":
                rest = rest[u.end():]
                continue
            close = _match_paren(rest, u.end() - 1)
            rest = rest[close:]
        rest = rest.lstrip()
        if not re.match(r"\.\s*path\s*\(\s*\)", rest):
            continue
        if re.search(r"\.\s*(?:keep|persist)\s*\(", chain):
            continue
        prev = lines[line_no - 2] if line_no >= 2 else ""
        if MARKER in prev:
            continue
        yield line_no, " ".join((m.group(0) + src[m.end():args_end] + chain).split())[:140]


def scan(root: pathlib.Path) -> int:
    found = 0
    for top in ROOTS:
        for p in sorted((root / top).rglob("*.rs")):
            if "target" in p.relative_to(root).parts:
                continue
            for line_no, stmt in violations(p.read_text(encoding="utf-8", errors="replace")):
                print(f"{p.relative_to(root)}:{line_no}: {stmt}")
                found += 1
    if found:
        print(
            f"\n{found} temporary guard(s) dropped in the statement that takes the path. "
            "Bind the guard and keep it alive until the output is read (see this script's docstring).",
            file=sys.stderr,
        )
        return 1
    return 0


MUST_FLAG = {
    "tests' exact shape": 'let out_path = tempfile::NamedTempFile::new()\n    .expect("tempfile")\n    .path()\n    .to_path_buf();',
    "unwrap, unqualified": "let p = NamedTempFile::new().unwrap().path().to_path_buf();",
    "TempDir joined": 'let p = TempDir::new().unwrap().path().join("x");',
    "tempdir() joined": 'let p = tempfile::tempdir().unwrap().path().join("out.json");',
    "Builder": 'let p = tempfile::Builder::new().suffix(".json").tempfile().unwrap().path().to_path_buf();',
    "? operator": "let p = NamedTempFile::new()?.path().to_owned();",
    "as an argument": 'run(NamedTempFile::new().unwrap().path(), "x");',
}
MUST_PASS = {
    "bound then used": 'let tmp = tempfile::NamedTempFile::new().expect("tempfile");\nlet out = tmp.path().to_path_buf();',
    "held TempDir": 'let out_dir = tempfile::tempdir().expect("tempdir");\nlet out = out_dir.path().join("out.json");',
    "path in the constructor's arguments": "let mut f = tempfile::NamedTempFile::new_in(tmp.path()).unwrap();",
    "deliberate keep": "let (_f, p) = NamedTempFile::new().unwrap().keep().unwrap();",
    "into_temp_path bound": "let p = NamedTempFile::new().unwrap().into_temp_path();",
    "marker": "// held-temp-path: the scan never writes here\nlet p = NamedTempFile::new().unwrap().path().to_path_buf();",
    "in a comment": "// let p = NamedTempFile::new().unwrap().path().to_path_buf();",
}


def self_test() -> int:
    ok = True
    for name, snippet in MUST_FLAG.items():
        hit = bool(list(violations(snippet)))
        print(f"{'ok  ' if hit else 'FAIL'} must flag: {name}")
        ok &= hit
    for name, snippet in MUST_PASS.items():
        hit = list(violations(snippet))
        print(f"{'ok  ' if not hit else 'FAIL'} must pass: {name}{'' if not hit else f' (flagged: {hit})'}")
        ok &= not hit
    print("self-test passed" if ok else "self-test FAILED")
    return 0 if ok else 1


def main(argv) -> int:
    if argv == ["--self-test"]:
        return self_test()
    if argv:
        print(__doc__, file=sys.stderr)
        return 2
    return scan(pathlib.Path.cwd())


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
