"""Spec 1651 T017 mutations (SC-006). Each one: back up every file it edits
with cp, apply its exact replacements, run its killing tests one cargo
invocation per filter, classify, restore with cp. A mutation that does not
compile is reported as such, never as killed; a filter that runs no test is
a harness error. Copied from spec 1650's runner; only the mutation table
differs.

usage: python3 -I -B mutate.py REPO OUTDIR [Ma,Mb,...]
"""
import pathlib
import shutil
import subprocess
import sys

REPO = pathlib.Path(sys.argv[1])
OUT = pathlib.Path(sys.argv[2])
ED = "waybill-cli/src/edit/mod.rs"
CDX = "waybill-cli/src/edit/cdx.rs"
CE = "waybill-cli/src/cli/edit.rs"


def unit(name):
    return ["--bin", "waybill", name]


def it(name):
    return ["--test", "sbom_edit_retier", name]


M = [
    ("Ma the run-local set removed (sequential relabelling)",
     [(ED, "                    if relabelled.contains(id) {\n                        continue;\n                    }\n", "")],
     [it("r2_")]),
    ("Mb the marker not written (CycloneDX)",
     [(CDX, "        if !props.iter().any(|p| named(p, super::RETIERED_FROM)) {", "        if false && !props.iter().any(|p| named(p, super::RETIERED_FROM)) {")],
     [it("r1_"), unit("s1651_set_tier_keeps_the_first_marker")]),
    ("Mc changed miscounted",
     [(ED, "                        changed += 1;", "                        changed += 2;")],
     [it("r1_"), it("r2_")]),
    ("Md the type never recomputed",
     [(ED, "            adapter.set_document_type(&phases, &types);", "            let _ = (&phases, &types);")],
     [it("t1_")]),
    ("Me the type always recomputed (asserted types overwritten)",
     [(ED, "        if adapter.document_type_is(&before_phases, &before_types) {", "        if true || adapter.document_type_is(&before_phases, &before_types) {")],
     [it("t2_")]),
    ("Mf completeness downgraded by a retier",
     [(ED, "                retiered += changed;", "                retiered += changed;\n                if changed > 0 {\n                    adapter.downgrade_graph_completeness();\n                }")],
     [it("c1_")]),
    ("Mg TO validation removed",
     [(CE, """    let to = crate::generate::lifecycle_phases::SbomType::parse_str(to).map_err(|_| {""",
           """    let to = crate::generate::lifecycle_phases::SbomType::parse_str(to).or(Ok::<_, ()>(crate::generate::lifecycle_phases::SbomType::Build)).map_err(|_| {""")],
     [it("refusals_"), unit("s1651_parse_retier")]),
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
