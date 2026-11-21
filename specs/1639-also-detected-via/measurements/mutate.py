"""Spec 1639 T026 mutations. Each: back up with cp, apply one exact
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

M = [
    ("M1", "resolve/deduplicator.rs",
     'also_detected::record("S1", best.purl.as_str(), &mut best.extra_annotations, &absorbed);',
     'let _ = &absorbed;',
     ["--bin", "waybill", "s1639_a_merge_records_the_losers_mechanism"]),
    ("M2", "scan_fs/package_db/cmake/fetches.rs",
     'crate::scan_fs::also_detected::record("S6", &subject, &mut entries[h].extra_annotations, &lost.extra_annotations);',
     'let _ = &subject;',
     ["--test", "also_detected_via", "a06_"]),
    ("M3", "scan_fs/also_detected.rs",
     "    set.remove(&own);\n",
     "",
     ["--bin", "waybill", "the_survivors_own_mechanism_is_never_recorded"]),
    ("M4", "scan_fs/also_detected.rs",
     "    let Some(own) = mechanism(survivor).map(str::to_string) else {\n        survivor.remove(ANN_ALSO_DETECTED_VIA);\n        return;\n    };",
     "    let own = mechanism(survivor).map(str::to_string).unwrap_or_default();",
     ["--bin", "waybill", "s1639_a_survivor_without_c55_records_nothing"]),
    ("M5", "scan_fs/binary/entry.rs",
     '"waybill:fingerprint-also-matched".to_string(),',
     '"waybill:also-detected-via".to_string(),',
     ["--bin", "waybill", "s1639_a_fingerprint_collision_is_c242_not_c56"]),
    ("M6", "parity/extractors/cdx.rs",
     'cdx_anno!(c56_cdx, "waybill:also-detected-via",         component);',
     'cdx_anno!(c56_cdx, "waybill-source-mechanism",          component);',
     ["--bin", "waybill", "also_detected_via_and_fingerprint_collisions_reach_all_three_formats"]),
    ("M7", "scan_fs/also_detected.rs",
     "Value::from(set.into_iter().collect::<Vec<_>>())",
     "Value::from(set.into_iter().rev().collect::<Vec<_>>())",
     ["--bin", "waybill", "values_are_sorted_and_unique"]),
    ("M8", "scan_fs/cmake_search_pair.rs",
     'super::also_detected::record("S9", c.purl.as_str(), &mut c.extra_annotations, a);',
     'let _ = a;',
     ["--test", "also_detected_via", "a10_"]),
    ("M9", "resolve/deduplicator.rs",
     "                if key == also_detected::ANN_ALSO_DETECTED_VIA {\n                    continue;\n                }\n",
     "",
     ["--bin", "waybill", "s1639_"]),
]


def run(mid, rel, old, new, test):
    path = SRC / rel
    text = path.read_text()
    if text.count(old) != 1:
        return f"{mid}: NOT APPLIED (anchor count {text.count(old)})"
    backup = OUT / f"{mid}.orig"
    shutil.copyfile(path, backup)
    try:
        path.write_text(text.replace(old, new))
        r = subprocess.run(["cargo", "test", "-p", "waybill", *test], cwd=REPO, capture_output=True, text=True)
        log = r.stdout + r.stderr
        (OUT / f"{mid}.log").write_text(log)
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
    results = [run(*m) for m in M]
    (OUT / "results.txt").write_text("\n".join(results) + "\n")
    print("\n".join(results))


if __name__ == "__main__":
    main()
