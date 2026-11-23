"""Spec 1645 T016 mutations (SC-004, research R9). Each one: back up every
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
LI = "waybill-cli/src/scan_fs/library_identity.rs"
CP = "waybill-cli/src/generate/cpe.rs"
UNIT = ["--bin", "waybill", "scan_fs::binary::version_strings::tests::"]
IT = ["--test", "library_identity"]


def unit(path):
    return ["--bin", "waybill", path]


def it(name):
    return IT + [name]


def vb(name):
    return ["--test", "version_string_boundary", name]


M = [
    ("M1 ncurses has its slug only",
     [(VS, 'CuratedLibrary::Ncurses => &["ncurses", "ncursesw", "ncursest", "ncursestw"],', 'CuratedLibrary::Ncurses => &["ncurses"],')],
     [it("m1_"), unit("scan_fs::binary::version_strings::tests::s1645_names_and_slugs_round_trip")]),
    ("M2 any record name accepted",
     [(LI, ".filter(|&i| library.names().contains(&entries[i].name.as_str()))", ".filter(|_| true)")],
     [it("n2_"), unit("scan_fs::library_identity::tests::s1645_each_verdict")]),
    ("M3 version agreement is a plain string prefix",
     [(LI, "curated == record || record.strip_prefix(curated).is_some_and(|rest| rest.starts_with('.'))", "record.starts_with(curated)")],
     [it("n3_"), unit("scan_fs::library_identity::tests::s1645_versions_agree_at_a_dot")]),
    ("M4 two qualifying records merge with the first",
     [(LI, "(false, _) => Verdict::Ambiguous,", "(false, _) => Verdict::Merge,")],
     [it("n4_"), unit("scan_fs::library_identity::tests::s1645_each_verdict")]),
    ("M5 the LLVM suffix dropped",
     [(VS, "if let Some(v) = parse_llvm_version(tail) {", "if let Some(v) = parse_semver_triple(tail) {")],
     [it("m2_"), unit("scan_fs::binary::version_strings::tests::s1645_ncurses_keeps_its_patch_date")]),
    ("M6 edges to the renamed record not remapped",
     [(LI, "if let Some(p) = renamed.get(d.as_str()) {", 'if let Some(p) = renamed.get("\\u{0}never") {')],
     [it("e1_"), unit("scan_fs::library_identity::tests::s1645_merge_rewrites_the_pair_and_its_edges")]),
    ("M7 the ncurses patch date dropped",
     [(VS, "if let Some(v) = parse_ncurses_full(tail) {", "if let Some(v) = parse_ncurses_version(tail) {")],
     [it("m1_"), vb("b1_"), unit("scan_fs::binary::version_strings::tests::s1526_expat_and_ncurses_real_literals")]),
    ("M8 the ncurses CPE split removed",
     [(CP, "if let Some(split) = ncurses_split_cpes(component) {", "if let Some(split) = None::<Vec<Cpe>> {")],
     [it("m1_"), vb("b1_")]),
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
