"""Spec 1668 T010: apply each mutation to haskell.rs, run the spec's tests,
revert with copyfile + utime (never git), check with cmp, report."""
import os, shutil, subprocess, sys, filecmp, pathlib
repo = pathlib.Path(sys.argv[1]); src = repo / "waybill-cli/src/scan_fs/package_db/haskell.rs"
backup = pathlib.Path(sys.argv[2]); shutil.copyfile(src, backup); st = src.stat()
M = {
  "M1 freeze ignores members": ("for g in lock_governed_dirs(dir, members) {\n                        let slot = pins_by_dir.entry(g).or_default();\n                        for e in &entries {\n                            // Only",
                                "for g in lock_governed_dirs(dir, Vec::<PathBuf>::new().into_iter().chain(members.into_iter().take(0))) {\n                        let slot = pins_by_dir.entry(g).or_default();\n                        for e in &entries {\n                            // Only"),
  "M2 no glob matching": ("out.extend(candidates.iter().filter(|c| m.is_match(c.as_str())).cloned());\n            }\n            Err(_) => {\n                tracing::debug!(entry = %token",
                          "out.extend(candidates.iter().filter(|c| c.as_str() == dir_pattern && m.is_match(c.as_str())).cloned());\n            }\n            Err(_) => {\n                tracing::debug!(entry = %token"),
  "M3 .cabal entry not its dir": ("let dir_pattern = if entry.ends_with(\".cabal\") {", "let dir_pattern = if false && entry.ends_with(\".cabal\") {"),
  "M4 lock dir not governed": ("std::iter::once(lock_dir.to_path_buf()).chain(members)", "std::iter::empty().chain(members)"),
}
cmd = ["cargo", "nextest", "run", "-p", "waybill", "--color", "never", "--no-fail-fast", "-E",
       "test(s1668) | binary(haskell_freeze_members) | test(resolve_root) | test(member) | test(1663) | test(stack_lock)"]
def run(label):
    r = subprocess.run(cmd, cwd=repo, capture_output=True, text=True)
    s = [l for l in (r.stdout + r.stderr).splitlines() if "Summary" in l or l.strip().startswith("FAIL")]
    print(f"## {label}\nexit={r.returncode}\n" + "\n".join(s) + "\n", flush=True)
    return r.returncode
results = {}
for k, (a, b) in M.items():
    t = backup.read_text(); assert t.count(a) == 1, k
    src.write_text(t.replace(a, b, 1)); results[k] = run(k)
    shutil.copyfile(backup, src); os.utime(src)  # now: an old mtime hides the revert from cargo
    assert filecmp.cmp(src, backup, shallow=False), "revert failed"
results["control"] = run("control (unmutated)")
print("| mutation | exit | killed |\n|---|---|---|")
for k, v in results.items():
    print(f"| {k} | {v} | {'n/a' if k == 'control' else ('yes' if v else 'NO')} |")
