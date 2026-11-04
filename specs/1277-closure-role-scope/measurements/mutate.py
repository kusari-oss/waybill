#!/usr/bin/env python3
"""Milestone 1277 mutation runner.

Each mutant is a (file, old, new) replacement applied to a clean tree, after
which the named suites run and their failures are recorded. The tree is
restored with `git checkout --` between mutants, so a mutant cannot leak into
the next.

Run from the repo root with a clean working tree.
"""
import subprocess
import sys

CRS = "waybill-cli/src/resolve/closure_role_scope.rs"
CDX = "waybill-cli/src/generate/cyclonedx/builder.rs"
SPDX23 = "waybill-cli/src/generate/spdx/relationships.rs"

MUTANTS = [
    ("M1 build-tooling assigns nothing", CRS,
     '"build-tooling" => Some(Role::BuildTooling),',
     '"build-tooling" => None,'),
    ("M2 artifact-input is classified", CRS,
     '"artifact-input" | "both" => None,',
     '"artifact-input" | "both" => Some(Role::BuildTooling),'),
    ("M3 exemption deleted", CRS,
     "if compiler_supplied(component, boot_libraries) {",
     "if false && compiler_supplied(component, boot_libraries) {"),
    ("M3b exemption widened to all", CRS,
     "if compiler_supplied(component, boot_libraries) {",
     "if true || compiler_supplied(component, boot_libraries) {"),
    ("M4 unreferenced arm removed", CRS,
     '"unreferenced" => Some(Role::Unreferenced),',
     '"unreferenced" => None,'),
    ("M5 CDX emission moved inside the include_dev gate", CDX,
     """            if component
                .artifact_presence
                .is_some_and(|p| p.is_absent_from_artifact())
            {
                entry["scope"] = json!("excluded");
            }""",
     """            if self.config.include_dev
                && component
                    .artifact_presence
                    .is_some_and(|p| p.is_absent_from_artifact())
            {
                entry["scope"] = json!("excluded");
            }"""),
    ("M6 C203 emitted unconditionally", CRS,
     "    let presence = component.artifact_presence?;",
     "    let presence = component.artifact_presence.unwrap_or(ArtifactPresence::NotInArtifact);"),
    ("M7 SPDX 2.3 BUILD_TOOL_OF omitted", SPDX23,
     "                    kind: SpdxRelationshipType::BuildToolOf,",
     "                    kind: SpdxRelationshipType::Contains,"),
    ("M10 classification ALSO carried on lifecycle_scope (the design R1 rejected)", CRS,
     "        component.artifact_presence = Some(ArtifactPresence::NotInArtifact);",
     """        component.artifact_presence = Some(ArtifactPresence::NotInArtifact);
        component.lifecycle_scope =
            Some(waybill_common::resolution::LifecycleScope::Build);"""),
    ("M8 exemption ignored in SPDX 2.3 only", SPDX23,
     """            if !c
                .artifact_presence
                .is_some_and(|p| p.is_absent_from_artifact())
            {
                continue;
            }""",
     """            if c.artifact_presence.is_none() {
                continue;
            }"""),
]

SUITES = ["closure_role_scope", "public_corpus"]
UNIT = ["m1277"]


def run(cmd):
    return subprocess.run(cmd, shell=True, capture_output=True, text=True)


def restore():
    run("git checkout -- " + " ".join({CRS, CDX, SPDX23}))


def failures():
    """Names of failing tests across the suites, or BUILD if it won't compile."""
    out = []
    for s in SUITES:
        r = run(f"cargo test -p waybill --test {s} 2>&1")
        if "error[E" in r.stdout or "error: could not compile" in r.stdout:
            return ["BUILD-ERROR"]
        out += [l.split()[1] for l in r.stdout.splitlines()
                if l.startswith("test ") and l.rstrip().endswith("FAILED")]
    for f in UNIT:
        r = run(f"cargo test -p waybill --bin waybill {f} 2>&1")
        if "error[E" in r.stdout or "error: could not compile" in r.stdout:
            return ["BUILD-ERROR"]
        out += [l.split()[1] for l in r.stdout.splitlines()
                if l.startswith("test ") and l.rstrip().endswith("FAILED")]
    return out


def main():
    if run("git status --porcelain").stdout.strip():
        print("working tree is not clean; refusing to mutate")
        return 1
    print("| mutant | caught | by |")
    print("|---|---|---|")
    survivors = []
    for name, path, old, new in MUTANTS:
        src = open(path).read()
        if src.count(old) != 1:
            restore()
            print(f"| {name} | **N/A** | anchor matched {src.count(old)} times |")
            survivors.append((name, "anchor"))
            continue
        open(path, "w").write(src.replace(old, new, 1))
        fails = failures()
        restore()
        if fails:
            shown = ", ".join(sorted(set(f.split("::")[-1] for f in fails))[:3])
            print(f"| {name} | yes ({len(set(fails))}) | {shown} |")
        else:
            print(f"| {name} | **NO** | survived |")
            survivors.append((name, "survived"))
    print()
    print(f"caught {len(MUTANTS) - len(survivors)} of {len(MUTANTS)}")
    for n, why in survivors:
        print(f"  unresolved: {n} ({why})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
