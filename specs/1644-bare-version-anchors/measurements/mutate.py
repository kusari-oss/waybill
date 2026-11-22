"""Spec 1644 T015 mutations (SC-003, research R9). Each one: back up every
file it edits with cp, apply its exact replacements, run its killing tests
one cargo invocation per filter, classify, restore with cp. A mutation that
does not compile is reported as such, never as killed; a filter that runs no
test is a harness error. Adapted from spec 1643's runner, with a file per
edit because M3 edits scan.rs.

usage: python3 -I -B mutate.py REPO OUTDIR [M1,M2,...]
"""
import pathlib
import shutil
import subprocess
import sys

REPO = pathlib.Path(sys.argv[1])
OUT = pathlib.Path(sys.argv[2])
VS = "waybill-cli/src/scan_fs/binary/version_strings.rs"
SC = "waybill-cli/src/scan_fs/binary/scan.rs"
UNIT = ["--bin", "waybill", "scan_fs::binary::version_strings::tests::"]
IT = ["--test", "version_string_anchored"]


def unit(name):
    return UNIT[:2] + [UNIT[2] + name]


def it(name):
    return IT + [name]


FOUND = "        if !anchors.is_empty() {\n            found.push((lib, anchors));\n"

M = [
    ("M1 bzip2 versioned without its anchor",
     [(VS, FOUND, "        if !anchors.is_empty() || lib == CuratedLibrary::Bzip2 {\n            found.push((lib, anchors));\n")],
     [it("n1_"), unit("s1644_no_anchor_no_verdict")]),
    ("M2 GMP versioned without its anchor",
     [(VS, FOUND, "        if !anchors.is_empty() || lib == CuratedLibrary::Gmp {\n            found.push((lib, anchors));\n")],
     [it("n2_"), unit("s1644_no_anchor_no_verdict")]),
    ("M3 an imported symbol anchors liblzma",
     [(SC, "                .filter(|s| !s.is_undefined())\n                .filter_map(|s| s.name().ok())\n                .filter(|n| ANCHOR_SYMBOLS.contains(n))\n",
           "                .filter_map(|s| s.name().ok())\n                .filter(|n| ANCHOR_SYMBOLS.contains(n))\n"),
      (SC, "                .filter(|s| s.is_global() && !s.is_undefined())\n", "                .filter(|s| s.is_global())\n")],
     [it("n3_"), it("n9_"), it("imported_or_unlisted")]),
    ("M4 the first of two candidates taken",
     [(VS, "            _ => Verdict::Ambiguous,\n", "            _ => Verdict::Versioned(versions[0].to_string()),\n")],
     [it("n4_"), unit("s1644_two_candidates_are_ambiguous")]),
    ("M5 a shared candidate versions both libraries",
     [(VS, ".find(|(j, _)| *j != i && cands[*j].iter().any(|o| o.string == c.string))",
           ".find(|(j, _)| *j != i && *j == usize::MAX && cands[*j].iter().any(|o| o.string == c.string))")],
     [it("n5_"), unit("s1644_a_shared_candidate")]),
    ("M6 a GMP candidate need not be the whole string",
     [(VS, "CuratedLibrary::Gmp => parse_dotted_triple(s).filter(|t| t.len() == s.len()).map(str::to_string),",
           "CuratedLibrary::Gmp => parse_dotted_triple(s).map(str::to_string),")],
     [unit("s1644_candidate_boundaries")]),
    ("M7 the xz grammar accepts a commit suffix",
     [(VS, 'matches!(&s[t.len()..], b"" | b"alpha" | b"beta")', 's[t.len()..].iter().all(|b| b.is_ascii_graphic())')],
     [it("n7_"), unit("s1644_anchor_with_no_candidate")]),
    ("M8 a space counts as a binary boundary",
     [(VS, "    (0x20..=0x7e).contains(&b) || matches!(b, b'\\t' | b'\\n' | b'\\r')\n",
           "    (0x21..=0x7e).contains(&b) || matches!(b, b'\\t' | b'\\n' | b'\\r')\n")],
     [unit("s1644_candidate_after_binary_data")]),
    ("M9 the census amendment reverted: only NUL may precede a candidate",
     [(VS, "        let k = seg.iter().rposition(|b| !is_text(*b)).map_or(0, |i| i + 1);\n",
           "        let k = if seg.iter().all(|b| is_text(*b)) { 0 } else { seg.len() };\n")],
     [it("g1_"), unit("s1644_candidate_after_binary_data")]),
]


def cargo(args):
    r = subprocess.run(["cargo", "test", "-p", "waybill", *args], cwd=REPO, capture_output=True, text=True)
    return r.returncode, r.stdout + r.stderr


def run(mid, edits, tests):
    key = mid.split()[0]
    files = sorted({f for f, _, _ in edits})
    texts = {f: (REPO / f).read_text() for f in files}
    for f, old, _ in edits:
        if texts[f].count(old) != 1:
            return f"{mid}: NOT APPLIED (anchor count {texts[f].count(old)} in {f} for {old.strip()[:60]!r})"
    backups = {}
    for f in files:
        backups[f] = OUT / f"{key}.{pathlib.Path(f).name}.orig"
        shutil.copyfile(REPO / f, backups[f])
    try:
        mutated = dict(texts)
        for f, old, new in edits:
            mutated[f] = mutated[f].replace(old, new)
        for f in files:
            (REPO / f).write_text(mutated[f])
        kills, logs = [], []
        for t in tests:
            code, log = cargo(t)
            logs.append(log)
            if any(line.startswith("error[E") for line in log.splitlines()):
                (OUT / f"{key}.log").write_text("\n".join(logs))
                return f"{mid}: COMPILE ERROR (not a kill)"
            if not any(l.startswith("test result:") for l in log.splitlines()):
                (OUT / f"{key}.log").write_text("\n".join(logs))
                return f"{mid}: HARNESS ERROR, a test command ran no tests ({t})"
            ran = [l for l in log.splitlines() if l.startswith("test ") and " ... " in l]
            if not ran:
                (OUT / f"{key}.log").write_text("\n".join(logs))
                return f"{mid}: HARNESS ERROR, filter {t[-1]} matched no test"
            failed = [l.split()[1] for l in ran if l.endswith("FAILED")]
            if code != 0 and failed:
                kills.extend(failed)
        (OUT / f"{key}.log").write_text("\n".join(logs))
        return f"{mid}: KILLED by {', '.join(kills)}" if kills else f"{mid}: SURVIVED"
    finally:
        for f in files:
            shutil.copyfile(backups[f], REPO / f)


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    only = set(sys.argv[3].split(",")) if len(sys.argv) > 3 else None
    results = [run(*m) for m in M if only is None or m[0].split()[0] in only]
    (OUT / "results.txt").write_text("\n".join(results) + "\n")
    print("\n".join(results))


if __name__ == "__main__":
    main()
