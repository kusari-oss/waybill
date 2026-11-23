"""Spec 1648 T015 mutations (SC-003, research R7). Each one: back up every
file it edits with cp, apply its exact replacements, run its killing tests
one cargo invocation per filter, classify, restore with cp. A mutation that
does not compile is reported as such, never as killed; a filter that runs no
test is a harness error. Copied from spec 1647's runner; only the mutation
table differs.

usage: python3 -I -B mutate.py REPO OUTDIR [M1,M2,...]
"""
import pathlib
import shutil
import subprocess
import sys

REPO = pathlib.Path(sys.argv[1])
OUT = pathlib.Path(sys.argv[2])
BM = "waybill-cli/src/scan_fs/binary/mod.rs"
DD = "waybill-cli/src/resolve/deduplicator.rs"
IT = ["--test", "binary_kind_technique"]
ARM1 = '"embedded-version-string" | "symbol-fingerprint" | "dynamic-linkage" | "embedded-source-path" => {'
ARM2 = '"cargo-auditable" | "elf-note-package" | "jdk-runtime-collapsed" | "python-stdlib-collapsed" => {'


def unit(path):
    return ["--bin", "waybill", path]


def it(name):
    return IT + [name]


M = [
    ("M1 dynamic-linkage decided PackageDatabase",
     [(BM, ARM1, '"embedded-version-string" | "symbol-fingerprint" | "embedded-source-path" => {'),
      (BM, ARM2, '"dynamic-linkage" | "cargo-auditable" | "elf-note-package" | "jdk-runtime-collapsed" | "python-stdlib-collapsed" => {')],
     [it("k1_"), it("k6_"), unit("scan_fs::binary::binary_evidence_kind_drift::s1648_each_kind_has_its_decided_technique")]),
    ("M2 embedded-source-path decided PackageDatabase",
     [(BM, ARM1, '"embedded-version-string" | "symbol-fingerprint" | "dynamic-linkage" => {'),
      (BM, ARM2, '"embedded-source-path" | "cargo-auditable" | "elf-note-package" | "jdk-runtime-collapsed" | "python-stdlib-collapsed" => {')],
     [it("k2_"), unit("scan_fs::binary::binary_evidence_kind_drift::s1648_each_kind_has_its_decided_technique")]),
    ("M3 cargo-auditable decided BinaryAnalysis",
     [(BM, ARM1, '"embedded-version-string" | "symbol-fingerprint" | "dynamic-linkage" | "embedded-source-path" | "cargo-auditable" => {'),
      (BM, ARM2, '"elf-note-package" | "jdk-runtime-collapsed" | "python-stdlib-collapsed" => {')],
     [it("k3_"), unit("resolve::deduplicator::tests::s1648_a_record_absorbs_the_newly_binary_kinds")]),
    ("M4 the merge gate reverted to spec 1646's two kinds",
     [(DD, "    if c.evidence_kind.as_deref().and_then(crate::scan_fs::binary::binary_kind_technique)\n        == Some(ResolutionTechnique::BinaryAnalysis)\n    {",
       '    if matches!(c.evidence_kind.as_deref(), Some("embedded-version-string" | "symbol-fingerprint")) {')],
     [unit("resolve::deduplicator::tests::s1648_a_record_absorbs_the_newly_binary_kinds")]),
    ("M5 a listed kind left without a decision",
     [(BM, ARM2, '"cargo-auditable" | "elf-note-package" | "jdk-runtime-collapsed" => {')],
     [unit("scan_fs::binary::binary_evidence_kind_drift::s1648_every_binary_kind_has_a_technique")]),
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
