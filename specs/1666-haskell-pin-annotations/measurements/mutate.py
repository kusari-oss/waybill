"""Spec 1666 T012: each mutation must fail at least one test.
Run from the repo root: python3 -I -B specs/1666-haskell-pin-annotations/measurements/mutate.py
Reverts with a byte copy and checks with cmp (never git checkout)."""
import filecmp, shutil, subprocess, sys, tempfile, os

HP = "waybill-cli/src/scan_fs/package_db/nix/haskell_packages/"
MUTATIONS = [
    ("M1 note is a fixed string", HP + "mod.rs",
     "                let note = disagreement_note(c);",
     '                let note = "the emitted version came from a project-local lockfile or freeze file and takes precedence".to_string();'),
    ("M2 no main-module skip", HP + "mod.rs",
     "if boot_libraries::is_boot_library(&c.name, &boot) || is_main_module(c) {",
     "if boot_libraries::is_boot_library(&c.name, &boot) {"),
    ("M3 never remove C172 on equality", HP + "twin_merge.rs",
     "    if rec.get(\"nixpkgs\").and_then(serde_json::Value::as_str) == Some(survivor.version.as_str()) {",
     "    if false {"),
    ("M4 never rewrite emitted", HP + "twin_merge.rs",
     "    rec.insert(\"emitted\".to_string(), serde_json::Value::from(survivor.version.clone()));",
     ""),
    ("M5 no hackage-freeze alternative", "waybill-cli/src/generate/orphan_reason.rs",
     'Some("hackage-stack-lock" | "hackage-freeze")',
     'Some("hackage-stack-lock")'),
    ("M6 no C174 subtraction", HP + "twin_merge.rs",
     "        *disagreements = disagreements.saturating_sub(self.c172_removed);",
     "        let _ = disagreements;"),
]
FILTER = "test(~s1666) | test(~spec1666) | test(~spec1663) | test(~twin_merge) | test(~m926) | test(~m1457) | binary(haskell_pin_annotations)"

def run():
    r = subprocess.run(["cargo", "nextest", "run", "-p", "waybill", "--bins", "--test", "haskell_pin_annotations",
                        "--no-fail-fast", "--color", "never", "-E", FILTER], capture_output=True, text=True)
    out = r.stdout + r.stderr
    failed = sorted({l.split()[-1] for l in out.splitlines() if l.strip().startswith("FAIL [")})
    summary = [l.strip() for l in out.splitlines() if l.strip().startswith("Summary")]
    return r.returncode, failed, summary, out

rows = []
bak = tempfile.mkdtemp()
for name, path, old, new in MUTATIONS:
    src = open(path).read()
    assert src.count(old) == 1, (name, "anchor not unique")
    keep = os.path.join(bak, os.path.basename(path))
    shutil.copy2(path, keep)
    open(path, "w").write(src.replace(old, new))
    try:
        code, failed, summary, out = run()
        if not summary:
            sys.stderr.write(out[-3000:])
    finally:
        # copyfile + utime, not copy2: copy2 restores the old mtime and cargo
        # then keeps the mutated binary (observed: the control ran M6).
        shutil.copyfile(keep, path)
        os.utime(path)
        assert filecmp.cmp(keep, path, shallow=False), "revert failed"
    rows.append((name, "killed" if failed else ("BUILD FAILED" if not summary else "SURVIVED"), failed, summary))
    print(rows[-1], flush=True)

code, ctrl_failed, summary, _ = run()
print("control (unmutated):", summary, ctrl_failed)
with open("specs/1666-haskell-pin-annotations/measurements/mutations.md", "w") as f:
    f.write("# Spec 1666 mutations (T012)\n\nFilter: `" + FILTER + "`\n\n| Mutation | Result | Failing tests |\n|---|---|---|\n")
    for name, res, failed, _ in rows:
        f.write(f"| {name} | {res} | {', '.join(t.split('::')[-1] for t in failed)} |\n")
    f.write(f"\nControl, unmutated: {summary[0] if summary else 'no summary'}; failing: {ctrl_failed or 'none'}\n")
