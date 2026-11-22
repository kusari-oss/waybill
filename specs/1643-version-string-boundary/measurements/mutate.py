"""Spec 1643 T014 mutations (SC-003, research R8). Each: back up with cp,
apply its exact replacements, run its killing tests, classify, restore with
cp. A mutation that does not compile is reported as such, never as killed.

usage: python3 -I -B mutate.py REPO OUTDIR [M1,M2,...]
"""
import pathlib
import shutil
import subprocess
import sys

REPO = pathlib.Path(sys.argv[1])
OUT = pathlib.Path(sys.argv[2])
VS = REPO / "waybill-cli/src/scan_fs/binary/version_strings.rs"
UNIT = ["--bin", "waybill"]
IT = ["--test", "version_string_boundary"]

ANCH = "            CuratedLibrary::Zlib | CuratedLibrary::Expat | CuratedLibrary::Ncurses => Boundary::Anchored,\n"
NUL_HEAD = "            CuratedLibrary::OpenSsl\n            | CuratedLibrary::BoringSsl\n"


def reclass(to_nul):
    """Move one anchored library into the NUL-only arm."""
    rest = [x for x in ("Zlib", "Expat", "Ncurses") if x != to_nul]
    return [
        (ANCH, "            " + " | ".join(f"CuratedLibrary::{x}" for x in rest) + " => Boundary::Anchored,\n"),
        (NUL_HEAD, f"            CuratedLibrary::{to_nul}\n            | CuratedLibrary::OpenSsl\n            | CuratedLibrary::BoringSsl\n"),
    ]


M = [
    ("M1 anchored accepts only NUL",
     [("            Boundary::Anchored => !is_identifier(prev),\n", "            Boundary::Anchored => prev == 0,\n")],
     [UNIT + ["version_strings::tests::s1643_anchored_patterns_accept"], IT + ["n2_"]]),
    ("M2 zlib classed NUL-only", reclass("Zlib"), [UNIT + ["version_strings::tests::s1643_every_variant"]]),
    ("M3 expat classed NUL-only", reclass("Expat"), [UNIT + ["version_strings::tests::s1643_every_variant"]]),
    ("M4 ncurses classed NUL-only", reclass("Ncurses"), [UNIT + ["version_strings::tests::s1643_every_variant"]]),
    ("M5 OpenSSL classed anchored",
     [(ANCH, "            CuratedLibrary::Zlib | CuratedLibrary::Expat | CuratedLibrary::Ncurses | CuratedLibrary::OpenSsl => Boundary::Anchored,\n"),
      (NUL_HEAD, "            CuratedLibrary::BoringSsl\n")],
     [UNIT + ["version_strings::tests::s1643_every_variant"], IT + ["u1_"]]),
    ("M6 identifier test removed",
     [("            Boundary::Anchored => !is_identifier(prev),\n", "            Boundary::Anchored => { let _ = prev; true }\n")],
     [UNIT + ["version_strings::tests::s1643_anchored_patterns_reject"],
      UNIT + ["version_strings::tests::s1643_census_agrees"], IT + ["z4_"]]),
    ("M7 zlib tests the byte before deflate, not before its space",
     [("                || (pos >= 1 && region[pos - 1] == b' ' && class.accepts(region, pos - 1))\n",
       "                || (pos >= 1 && region[pos - 1] == b' ' && class.accepts(region, pos))\n")],
     [UNIT + ["version_strings::tests::s1643_anchored_patterns_reject"], IT + ["z4_"]]),
    ("M8 the preceding-byte shortcut skips the anchored patterns",
     [("            lib.boundary() == Boundary::Anchored || (prev == b' ' && lib == CuratedLibrary::Zlib)\n",
       "            lib.boundary() == Boundary::NulOnly || (prev == b' ' && lib == CuratedLibrary::Zlib)\n")],
     [UNIT + ["version_strings::tests::s1643_census_agrees"], IT + ["n2_"]]),
]


def cargo(args):
    r = subprocess.run(["cargo", "test", "-p", "waybill", *args], cwd=REPO, capture_output=True, text=True)
    return r.returncode, r.stdout + r.stderr


def run(mid, edits, tests):
    text = VS.read_text()
    for old, _ in edits:
        if text.count(old) != 1:
            return f"{mid}: NOT APPLIED (anchor count {text.count(old)} for {old.strip()[:60]!r})"
    key = mid.split()[0]
    backup = OUT / (key + ".orig")
    shutil.copyfile(VS, backup)
    try:
        mutated = text
        for old, new in edits:
            mutated = mutated.replace(old, new)
        VS.write_text(mutated)
        kills, logs = [], []
        for t in tests:
            code, log = cargo(t)
            logs.append(log)
            if not any(l.startswith("test result:") for l in log.splitlines()):
                (OUT / (key + ".log")).write_text("\n".join(logs))
                return f"{mid}: HARNESS ERROR, a test command ran no tests ({t})"
            if any(line.startswith("error[E") for line in log.splitlines()):
                (OUT / (key + ".log")).write_text("\n".join(logs))
                return f"{mid}: COMPILE ERROR (not a kill)"
            failed = [l.split()[1] for l in log.splitlines() if l.startswith("test ") and l.endswith("FAILED")]
            if code != 0 and failed:
                kills.extend(failed)
        (OUT / (key + ".log")).write_text("\n".join(logs))
        return f"{mid}: KILLED by {', '.join(kills)}" if kills else f"{mid}: SURVIVED"
    finally:
        shutil.copyfile(backup, VS)


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    only = set(sys.argv[3].split(",")) if len(sys.argv) > 3 else None
    results = [run(*m) for m in M if only is None or m[0].split()[0] in only]
    (OUT / "results.txt").write_text("\n".join(results) + "\n")
    print("\n".join(results))


if __name__ == "__main__":
    main()
