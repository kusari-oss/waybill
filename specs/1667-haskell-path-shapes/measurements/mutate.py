"""Spec 1667 T012: each mutation must fail at least one test.
Run from the repo root: python3 -I -B specs/1667-haskell-path-shapes/measurements/mutate.py
Reverts with a byte copy and checks with cmp (never git checkout)."""
import filecmp, shutil, subprocess, sys, tempfile, os

HP = "waybill-cli/src/scan_fs/package_db/haskell/hpack.rs"
MUTATIONS = [
    ("M1 root_relative returns the path unchanged", HP,
     '    up.chain(down).collect::<Vec<_>>().join("/")',
     '    { let _ = (up, down); p.display().to_string() }'),
    ("M2 no `..` above the root", HP,
     'let up = std::iter::repeat_n("..".to_string(), r.len() - common);',
     'let up = std::iter::repeat_n("..".to_string(), 0);'),
    ("M3 include entries not relativised", HP,
     '                "manifest": path(&r.manifest),',
     '                "manifest": r.manifest.display().to_string(),'),
    ("M4 subproject_dir drops the file test", "waybill-cli/src/generate/split.rs",
     "    if listed || scan_root.join(rel).is_file() {",
     "    if listed {"),
    ("M5 candidates joined to the canonical root again", "waybill-cli/src/cli/scan_cmd.rs",
     "report.dropped_evidence_paths.iter().map(PathBuf::from).collect();",
     "report.dropped_evidence_paths.iter().map(|p| canonical_root.join(p)).collect();"),
]
FILTER = "test(~s1667) | test(~amend_) | test(~root_dir_) | binary(haskell_path_shapes) | binary(project_discovery_haskell)"

def run():
    r = subprocess.run(["cargo", "nextest", "run", "-p", "waybill", "--bins", "--test", "haskell_path_shapes", "--test", "project_discovery_haskell",
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
with open("specs/1667-haskell-path-shapes/measurements/mutations.md", "w") as f:
    f.write("# Spec 1667 mutations (T012)\n\nFilter: `" + FILTER + "`\n\n| Mutation | Result | Failing tests |\n|---|---|---|\n")
    for name, res, failed, _ in rows:
        f.write(f"| {name} | {res} | {', '.join(t.split('::')[-1] for t in failed)} |\n")
    f.write(f"\nControl, unmutated: {summary[0] if summary else 'no summary'}; failing: {ctrl_failed or 'none'}\n")
