"""Spec 1650 T013 mutations (SC-006). Each one: back up every file it edits
with cp, apply its exact replacements, run its killing tests one cargo
invocation per filter, classify, restore with cp. A mutation that does not
compile is reported as such, never as killed; a filter that runs no test is
a harness error. Copied from spec 1649's runner; only the mutation table
differs.

usage: python3 -I -B mutate.py REPO OUTDIR [Ma,Mb,...]
"""
import pathlib
import shutil
import subprocess
import sys

REPO = pathlib.Path(sys.argv[1])
OUT = pathlib.Path(sys.argv[2])
RT = "waybill-cli/src/resolve/closure_resolved_tier.rs"
I11 = "waybill-cli/tests/corpus_harness_195/invariants/checks/closure_resolved_tier.rs"
EVAL = "        if c.artifact_presence == Some(ArtifactPresence::ExemptEvaluatedPackage) {"
BOOT = "        } else if c.extra_annotations.contains_key(ANN_BOOT_LIBRARY_COMPILER) {"


def unit(name):
    return ["--bin", "waybill", name]


def it(name):
    return ["--test", "closure_resolved_tier", name]


def inv(name):
    return ["--test", "public_corpus", "closure_resolved_tier::tests::" + name]


M = [
    ("Ma FR-002 removed (the evaluated package keeps design)",
     [(RT, EVAL, "        if false && c.artifact_presence == Some(ArtifactPresence::ExemptEvaluatedPackage) {")],
     [it("p2_"), it("r1_"), unit("s1650_evaluated_package")]),
    ("Mb FR-003 removed (compiler-supplied boot libraries keep design)",
     [(RT, BOOT, "        } else if false && c.extra_annotations.contains_key(ANN_BOOT_LIBRARY_COMPILER) {")],
     [it("b1_"), unit("s1650_compiler_supplied")]),
    ("Mc the design guard removed (any tier is re-tiered)",
     [(RT, "        if c.sbom_tier.as_deref() != Some(\"design\") {\n            continue;\n        }\n", "")],
     [unit("s1650_other_tiers")]),
    ("Md I11 clause (a) removed",
     [(I11, "        .filter(|c| is_design(c) && closure_resolved(c))", "        .filter(|c| is_design(c) && closure_resolved(c) && false)")],
     [inv("v1_"), inv("v2_")]),
    ("Me I11 clause (b) removed",
     [(I11, "    if !direct.is_empty() && direct.iter().all(|r| design_of(r)) {", "    if false && !direct.is_empty() && direct.iter().all(|r| design_of(r)) {")],
     [inv("v3_")]),
    ("Mf the evidence check removed (every design component re-tiered)",
     [(RT, BOOT, "        } else if true {")],
     [it("p3_"), it("b3_"), unit("s1650_design_without")]),
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
