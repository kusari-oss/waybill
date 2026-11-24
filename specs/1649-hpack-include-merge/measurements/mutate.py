"""Spec 1649 T017 mutations (SC-005). Each one: back up every file it edits
with cp, apply its exact replacements, run its killing tests one cargo
invocation per filter, classify, restore with cp. A mutation that does not
compile is reported as such, never as killed; a filter that runs no test is
a harness error. Copied from spec 1648's runner; only the mutation table
differs.

usage: python3 -I -B mutate.py REPO OUTDIR [Ma,Mb,...]
"""
import pathlib
import shutil
import subprocess
import sys

REPO = pathlib.Path(sys.argv[1])
OUT = pathlib.Path(sys.argv[2])
PR = "waybill-cli/src/scan_fs/package_db/haskell/hpack/prep.rs"
HP = "waybill-cli/src/scan_fs/package_db/haskell/hpack.rs"
IT = ["--test", "hpack_include_merge"]


def unit(name):
    return ["--bin", "waybill", name]


def it(name):
    return IT + [name]


M = [
    ("Ma merge precedence reversed (last merge wins)",
     [(PR, "            for src in sources {", "            for src in sources.into_iter().rev() {")],
     [it("m3_"), unit("s1649_fold_first_merge_wins")]),
    ("Mb explicit-over-merge reversed",
     [(PR, "        if !into.contains_key(&k) {\n            into.insert(k, v);\n        }", "        into.insert(k, v);")],
     [it("m4_"), unit("s1649_fold_explicit_beats_merge")]),
    ("Mc the rename also rewrites a non-<< key",
     [(PR, '        if !opaque && parts.key == Some("<<") {', '        if !opaque && parts.key.is_some_and(|k| k.contains("<<")) {')],
     [unit("s1649_rename_touches_only_line_keys")]),
    ("Mc2 the rename ignores block scalars and quoted strings",
     [(PR, '        if !opaque && parts.key == Some("<<") {', '        if parts.key == Some("<<") {')],
     [unit("s1649_rename_touches_only_line_keys")]),
    ("Md includes resolved relative to the manifest",
     [(PR, '        let display = lexical_join(file.parent().unwrap_or(Path::new("")), include);',
           '        let display = lexical_join(self.manifest.parent().unwrap_or(Path::new("")), include);')],
     [it("i2_"), unit("s1649_splice_nested_include")]),
    ("Me the root bound removed",
     [(PR, "            Some(root) if canonical.starts_with(root) => {}\n            _ => return Err(UnresolvedReason::OutsideRoot),",
           "            _ => {}")],
     [it("u2_"), unit("s1649_bound_outside_root")]),
    ("Mf the depth bound removed",
     [(PR, "        if chain.len() > MAX_INCLUDE_DEPTH {", "        if false && chain.len() > MAX_INCLUDE_DEPTH {")],
     [it("u4_"), unit("s1649_bound_depth")]),
    ("Mg names lowercased again (FR-009)",
     [(HP, "    Some((\n        name.to_string(),", "    Some((\n        name.to_lowercase(),")],
     [it("l1_")]),
    ("Mh package:qualifier kept whole (FR-009)",
     [(HP, "                Some((pkg, qual)) if !pkg.is_empty() && !qual.is_empty() => {",
           "                Some((pkg, qual)) if false && !pkg.is_empty() && !qual.is_empty() => {")],
     [it("l2_")]),
    ("Mi a single-mapping when: read as nothing (FR-010)",
     [(HP, "        Some(When::One(w)) => vec![w],", "        Some(When::One(_)) => Vec::new(),")],
     [it("l3_")]),
    ("Mjk both partial guards removed: the fold drops a tagged merge and an unresolved include is not counted",
     [(PR, "                .filter(|(k, val)| is_merge_key(k) && !matches!(val, Value::Tagged(_)))",
           "                .filter(|(k, _)| is_merge_key(k))"),
      (HP, "    if strip_tagged(&mut value) || any_unresolved {", "    if strip_tagged(&mut value) {")],
     [it("u3_"), it("u4_")]),
    ("Mj only the fold guard removed (expected to survive: the unresolved count still marks it partial)",
     [(PR, "                .filter(|(k, val)| is_merge_key(k) && !matches!(val, Value::Tagged(_)))",
           "                .filter(|(k, _)| is_merge_key(k))")],
     [it("u3_"), it("u4_")]),
    ("Mk only the unresolved count removed (expected to survive: strip_tagged still finds the tag)",
     [(HP, "    if strip_tagged(&mut value) || any_unresolved {", "    if strip_tagged(&mut value) {")],
     [it("u1_"), it("u2_"), it("u3_"), it("u4_"), it("u5_")]),
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
