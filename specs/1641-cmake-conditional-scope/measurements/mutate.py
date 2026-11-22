"""Spec 1641 T018 mutations (SC-006). Each: back up with cp, apply one exact
replacement, run its killing test, classify, restore with cp.

usage: python3 -I -B mutate.py REPO OUTDIR
"""
import pathlib
import shutil
import subprocess
import sys

REPO = pathlib.Path(sys.argv[1])
OUT = pathlib.Path(sys.argv[2])
SRC = REPO / "waybill-cli/src"
CD = "scan_fs/package_db/cmake/conditions.rs"
IT = ["--test", "cmake_conditional_scope"]
BIN = ["--bin", "waybill"]

M = [
    ("M01 platform atom", CD, "        if is_platform_var(name) {\n", "        if false && is_platform_var(name) {\n", IT + ["census_one_event"]),
    ("M02 platform comparison", CD, "if is_platform_var(l) || is_platform_var(r) {", "if false {", BIN + ["platform_predicates_and_their_negations"]),
    ("M03 superbuild guard", CD,
     "(Some(\"root\"), Some(\"current\")) | (Some(\"current\"), Some(\"root\")) => return Val::Standalone,",
     "(Some(\"root\"), Some(\"current\")) => return Val::Standalone,", BIN + ["the_superbuild_guard_holds"]),
    ("M04 option default", CD, "            Some((v, s)) => {", "            Some((_v, s)) => {\n                let v = &Eval::Unknown;", IT + ["t04_"]),
    ("M05 missing value is OFF", CD, "map_or(Eval::DoesNotHold, |v| constant(v)", "map_or(Eval::Unknown, |v| constant(v)", BIN + ["options_take_their_declared_default"]),
    ("M06 dependent option", CD, "let deps = d.depends.iter().fold(Eval::Holds, |acc, c| acc.and(plain.evaluate(c, &mut BTreeSet::new())));",
     "let deps = Eval::Holds;", BIN + ["a_dependent_option_follows"]),
    ("M07 CACHE BOOL", CD, "is_some_and(|t| t == \"BOOL\") && i == 2 =>", "is_some_and(|t| t == \"BOOL\") && i == 99 =>", BIN + ["options_take_their_declared_default"]),
    ("M08 literal set", CD, "None if args.len() == 2 => {", "None if args.len() == 99 => {", IT + ["census_one_event"]),
    ("M09 conflict", CD, "                        seen.0 = Eval::Unknown;\n", "", BIN + ["conflicting_defaults_are_unknown"]),
    ("M10 NOT", CD, "Val::E(e) => Val::E(e.not()),", "Val::E(e) => Val::E(e),", BIN + ["operators_follow_cmake_precedence"]),
    ("M11 AND", CD, "v = Val::E(v.eval().and(r.eval()));", "v = Val::E(v.eval().or(r.eval()));", IT + ["t05_"]),
    ("M12 OR", CD, "v = Val::E(v.eval().or(r.eval()));", "v = Val::E(v.eval().and(r.eval()));", BIN + ["operators_follow_cmake_precedence"]),
    ("M13 else", CD, "v = v.and(self.evaluate(e, atoms).not());", "v = v.and(self.evaluate(e, atoms));", IT + ["t07_"]),
    ("M14 elseif history", "scan_fs/package_db/cmake/blocks.rs", "                        earlier.push(c);\n", "                        let _ = c;\n", IT + ["t07e_"]),
    ("M15 nesting", CD, "        for b in branches {", "        for b in branches.iter().take(1) {", BIN + ["branches_read_else_and_elseif"]),
    ("M16 scope_files", "scan_fs/package_db/cmake/reach.rs",
     "                stack.extend(self.edges.get(&f).into_iter().flatten().map(|e| e.source.clone()));\n", "", IT + ["t09b_"]),
    ("M17 kept fetch in scope", "scan_fs/package_db/cmake/fetches.rs", "                if eval.in_scope() {", "                if false && eval.in_scope() {", IT + ["t01_"]),
    ("M18 C232 only with a scope", "scan_fs/package_db/cmake/fetches.rs",
     "if (entry.lifecycle_scope.is_some() || kept) && !conditions.is_empty() {",
     "if entry.lifecycle_scope.is_some() && !conditions.is_empty() {", IT + ["t01_"]),
    ("M19 one_per_purl keeps C232", "scan_fs/package_db/cmake/fetches.rs",
     "if (first.lifecycle_scope.is_some() || !any_unconditional) && !conds.is_empty() {",
     "if first.lifecycle_scope.is_some() && !conds.is_empty() {", IT + ["m1_"]),
    ("M20 R9 keys on C232", "scan_fs/package_db/cmake/fetches.rs",
     "                || (e.lifecycle_scope.is_none() && e.extra_annotations.contains_key(ANN_CMAKE_FETCH_CONDITIONS)))",
     "                || false)", IT + ["r1_"]),
    ("M21 searches follow the rule", "scan_fs/package_db/cmake/searches.rs", "} else if s.eval.in_scope() {", "} else if false {", IT + ["s1_a_package"]),
    ("M22 kept record records C236", "scan_fs/cmake_search_pair.rs",
     "if ps.iter().all(|p| p.conditional || !p.conditions.is_empty()) {", "if false {", IT + ["s3_"]),
    ("M23 unconditional outranks kept", "scan_fs/package_db/cmake/fetches.rs",
     "            None if unconditional(e) => 0,\n            None => 1,\n", "            None => 0,\n", IT + ["m2_"]),
    ("M24 kept outranks Optional", "scan_fs/package_db/cmake/fetches.rs",
     "            None => 1,\n            Some(LifecycleScope::Optional) => 2,\n",
     "            None => 2,\n            Some(LifecycleScope::Optional) => 1,\n", IT + ["m1_"]),
    ("M25 a function body stays Optional", "scan_fs/package_db/cmake/searches.rs",
     "} else if s.eval.in_scope() && !s.conditions.iter().any(|c| c.starts_with(\"function: \")) {",
     "} else if s.eval.in_scope() {", ["--test", "scan_cmake_tree_finds", "w17_"]),
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
