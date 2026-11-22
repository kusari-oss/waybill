"""Spec 1642 T016 mutations (SC-006). Each: back up with cp, apply one exact
replacement, run its killing test, classify, restore with cp.

usage: python3 -I -B mutate.py REPO OUTDIR [M1,M2,...]
"""
import pathlib
import shutil
import subprocess
import sys

REPO = pathlib.Path(sys.argv[1])
OUT = pathlib.Path(sys.argv[2])
SRC = REPO / "waybill-cli/src"
PM = "scan_fs/package_db/pip/mod.rs"
DA = "scan_fs/declared_attach.rs"
SP = "resolve/scope_propagation.rs"
SC = "cli/scan_cmd.rs"
SF = "scan_fs/mod.rs"
IT = ["--test", "requirements_owner"]
BIN = ["--bin", "waybill"]

M = [
    ("M1 drop R2's source-path swap", PM,
     "    if existing.sbom_tier.as_deref() == Some(\"deployed\") || existing.source_path == synthesized.source_path {",
     "    if true || existing.sbom_tier.as_deref() == Some(\"deployed\") || existing.source_path == synthesized.source_path {",
     IT + ["o3_"]),
    ("M2 apply R2 to installed entries", PM,
     "    if existing.sbom_tier.as_deref() == Some(\"deployed\") || existing.source_path == synthesized.source_path {",
     "    if existing.source_path == synthesized.source_path {",
     IT + ["o8_"]),
    ("M3 drop the lockfile path", PM,
     "        existing.extra_source_paths.insert(0, old);\n",
     "        let _ = old;\n",
     IT + ["o3_"]),
    ("M4 drop the non-runtime key", DA,
     "            let cand = (rank, non_runtime, purl.to_string());",
     "            let cand = (rank, false && non_runtime, purl.to_string());",
     BIN + ["s1642_a_runtime_candidate"]),
    ("M5 drop the anchor seeds", SC,
     "                &added_seeds,\n                &anchor_seeds,\n",
     "                &added_seeds,\n                &std::collections::HashSet::new(),\n",
     IT + ["a1_"]),
    ("M6 drop the declaring-file filter", SF,
     "            declared.root_anchors.difference(&declared.runtime_root_anchors).cloned().collect();",
     "            Vec::new();",
     IT + ["a3_"]),
    ("M7 drop the anchor's own-scope filter", SP,
     "        .filter(|c| wanted.contains(c.purl.as_str()))\n        .filter(|c| is_runtime_seed_candidate(c, roots))\n",
     "        .filter(|c| wanted.contains(c.purl.as_str()))\n        .filter(|_| roots.len() < usize::MAX)\n",
     BIN + ["s1642_a_non_runtime_anchor"]),
    ("M8 own by the first path only", DA,
     "        .find(|p| relative(p, scan_root, canonical_root).is_some_and(|rel| canonical_root.join(rel).is_dir()))",
     "        .find(|p| relative(p, scan_root, canonical_root).is_some_and(|rel| false && canonical_root.join(rel).is_dir()))",
     BIN + ["s1642_a_main_module_owns_the_directory"]),
]


def run(mid, rel, old, new, test):
    path = SRC / rel
    text = path.read_text()
    if text.count(old) != 1:
        return f"{mid}: NOT APPLIED (anchor count {text.count(old)})"
    key = mid.split()[0]
    backup = OUT / (key + ".orig")
    shutil.copyfile(path, backup)
    try:
        path.write_text(text.replace(old, new))
        r = subprocess.run(["cargo", "test", "-p", "waybill", *test], cwd=REPO, capture_output=True, text=True)
        log = r.stdout + r.stderr
        (OUT / (key + ".log")).write_text(log)
        if any(line.startswith("error[E") for line in log.splitlines()):
            return f"{mid}: COMPILE ERROR (not a kill)"
        failed = [l for l in log.splitlines() if l.startswith("test ") and l.endswith("FAILED")]
        ran = [l for l in log.splitlines() if l.startswith("test result:")]
        if r.returncode != 0 and failed:
            return f"{mid}: KILLED by {', '.join(f.split()[1] for f in failed)}"
        return f"{mid}: SURVIVED ({ran[-1] if ran else 'no test result line'})"
    finally:
        shutil.copyfile(backup, path)


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    only = set(sys.argv[3].split(",")) if len(sys.argv) > 3 else None
    results = [run(*m) for m in M if only is None or m[0].split()[0] in only]
    (OUT / "results.txt").write_text("\n".join(results) + "\n")
    print("\n".join(results))


if __name__ == "__main__":
    main()
