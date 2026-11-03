#!/usr/bin/env python3
"""Apply one mutation, run suites, restore, and report — verifying the edit.

Milestone 1276, inherited verbatim from 1273. SC-006 requires every requirement be covered by a test that
fails when the behaviour is removed, mutation-tested rather than assumed.

# Why this script exists

Three mutations in this milestone reported **SURVIVED** without ever being
applied:

- M12's edit targeted `haskell.rs` when the code lives in `haskell/hpack.rs`.
- M32's search string did not match the file's line continuations.
- (And separately, three *caught* mutants were reported as
  `DID-NOT-COMPILE`, because the classifier read cargo's
  `error: test failed` — which means a test failed — as a build failure.)

A mutant that survives because it was never applied looks exactly like a
missing test, and is the more dangerous of the two: it invites you to write a
test for a hole that is not there, or to accept a gap that is. So this script
**refuses to report a verdict it cannot justify**: if the replacement does not
change the file, that is an ERROR, not a SURVIVED.

It also distinguishes a compile failure from a failing test, and names the
suites a verdict is relative to — because a mutation run scoped to one suite
can report a survivor the wider suite catches (M26).

# Usage

    mutate.py <label> <file> <find-file> <replace-file> <suite>...

`find-file` and `replace-file` hold the exact text, so quoting and line
continuations survive the shell. Exit status is 0 for CAUGHT, 1 for SURVIVED,
2 for an error that invalidates the run.
"""

import pathlib
import re
import shutil
import subprocess
import sys
import tempfile


def run_suites(suites):
    """Return (compile_failed, failing_test_names)."""
    out = []
    for suite in suites:
        r = subprocess.run(
            ["cargo", "test", "-p", "waybill", "--test", suite],
            capture_output=True,
            text=True,
        )
        out.append(r.stdout + r.stderr)
    blob = "\n".join(out)
    # `error: test failed` is cargo reporting a FAILING TEST, not a build
    # failure. Matching it as the latter is how three caught mutants were
    # first recorded as harness breakage.
    compile_failed = bool(
        re.search(r"^error\[", blob, re.M)
        or re.search(r"^error: could not compile", blob, re.M)
    )
    failing = re.findall(r"^test ([A-Za-z0-9_:]+) \.\.\. FAILED", blob, re.M)
    return compile_failed, failing


def main():
    if len(sys.argv) < 6:
        print(__doc__)
        return 2
    label, target, find_path, repl_path = sys.argv[1:5]
    suites = sys.argv[5:]

    path = pathlib.Path(target)
    find = pathlib.Path(find_path).read_text()
    repl = pathlib.Path(repl_path).read_text()

    original = path.read_text()
    backup = pathlib.Path(tempfile.mkdtemp()) / path.name
    backup.write_text(original)

    try:
        if find not in original:
            print(f"{label:<54} ERROR  the search text is not in {target} — "
                  f"the mutation was never applied, so there is no verdict")
            return 2
        mutated = original.replace(find, repl, 1)
        if mutated == original:
            print(f"{label:<54} ERROR  replacement changed nothing")
            return 2
        path.write_text(mutated)

        compile_failed, failing = run_suites(suites)
        if compile_failed:
            print(f"{label:<54} DID-NOT-COMPILE  reformulate; a build failure "
                  f"is not a caught mutant")
            return 2
        if failing:
            print(f"{label:<54} CAUGHT  {' '.join(failing)}")
            return 0
        print(f"{label:<54} SURVIVED  relative to: {', '.join(suites)}")
        return 1
    finally:
        shutil.copy(backup, path)


if __name__ == "__main__":
    sys.exit(main())
