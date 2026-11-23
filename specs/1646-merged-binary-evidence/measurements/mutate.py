"""Spec 1646 T012 mutations (SC-004, research R8). Each one: back up every
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
DD = "waybill-cli/src/resolve/deduplicator.rs"
SM = "waybill-cli/src/scan_fs/mod.rs"
S2A = "waybill-cli/src/generate/spdx/annotations.rs"
UNIT = ["--bin", "waybill", "scan_fs::binary::version_strings::tests::"]
IT = ["--test", "merged_binary_evidence"]


def unit(path):
    return ["--bin", "waybill", path]


def it(name):
    return IT + [name]


def vb(name):
    return ["--test", "version_string_boundary", name]


M = [
    ("M1 absorbed methods not applied at S1",
     [(DD, "            apply_absorbed(&mut best, methods);", "            let _ = methods;")],
     [it("b1_"), it("b2_")]),
    ("M2 a banner match keeps PackageDatabase",
     [(SM, "crate::resolve::deduplicator::BINARY_KINDS.contains(&k)", 'k == "symbol-fingerprint"')],
     [it("b3_"), vb("b1_")]),
    ("M3 a fingerprint match keeps PackageDatabase",
     [(SM, "crate::resolve::deduplicator::BINARY_KINDS.contains(&k)", 'k == "embedded-version-string"')],
     [it("f1_")]),
    ("M4 absorbed methods not deduplicated by technique",
     [(DD, "if m.technique != own && !seen {", "if m.technique != own {")],
     [unit("resolve::deduplicator::tests::s1646_one_method_per_technique_and_chains_carry"), it("f2_")]),
    ("M5 SPDX methods emitted for a single method",
     [(S2A, "if !c.evidence.also.is_empty() {", "if true {")],
     [it("r1_"), it("b3_"), unit("generate::spdx::annotations::tests::s1646_d1_methods_only_when_two")]),
    ("M6 the binary-side condition removed",
     [(DD, "    if !is_binary_side(best) && !is_binary_side(other) {\n        return Vec::new();\n    }\n", "")],
     [unit("resolve::deduplicator::tests::s1646_a_record_into_a_record_absorbs_nothing"), it("r1_")]),
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
