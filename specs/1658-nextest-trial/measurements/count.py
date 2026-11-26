"""Count tests and per-binary time in a `cargo test` or `cargo nextest run` log.

Usage: python3 -I -B count.py LOG [--per-binary]

The runner is detected from the log: a nextest `Summary` line means nextest.
Prints one line, `runner passed failed ignored doctests binaries`, and with
`--per-binary` one `key<TAB>passed<TAB>seconds` line per binary, where key is
`lib:<crate>`, `bin:<name>` or `test:<name>` for both runners (crate names
normalised to underscores), so the two runners' outputs can be joined.
"""
import re
import sys

RUNNING = re.compile(r"^\s+Running (unittests src/(lib|main)\.rs|tests/(\S+)\.rs|\S+) \(.*?/deps/([A-Za-z0-9_\-]+)-[0-9a-f]+(?:\.exe)?\)")
DOCTESTS = re.compile(r"^\s+Doc-tests (\S+)")
RESULT = re.compile(r"^test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; \d+ measured; \d+ filtered out; finished in ([\d.]+)s")
NX_TEST = re.compile(r"^\s+(PASS|FAIL|LEAK|LEAK-FAIL|TIMEOUT|SIGSEGV|SIGABRT|SIGBUS|ABORT|FLAKY)\s+\[\s*([\d.]+)s\]\s+(?:\(\s*\d+/\d+\)\s+)?(\S+)\s+\S")
NX_SUMMARY = re.compile(r"^\s+Summary \[\s*[\d.]+s\] \d+ tests? run: (.*)$")


def field(text, word):
    m = re.search(r"(\d+) " + word, text)
    return int(m.group(1)) if m else 0


def norm(name):
    return name.replace("-", "_")


def nextest_key(binary_id):
    crate, _, rest = binary_id.partition("::")
    crate = norm(crate)
    if not rest:
        return f"lib:{crate}"
    if rest.startswith("bin/"):
        return f"bin:{rest[4:]}"
    return f"test:{rest}"


def parse(lines):
    if any(NX_SUMMARY.match(l) for l in lines):
        return parse_nextest(lines)
    return parse_cargo(lines)


def parse_cargo(lines):
    per, key, doc = {}, None, False
    totals = [0, 0, 0, 0]  # passed failed ignored doctests
    for l in lines:
        m = RUNNING.match(l)
        if m:
            doc = False
            kind = m.group(2)
            if kind == "lib":
                key = f"lib:{norm(m.group(4))}"
            elif kind == "main":
                key = f"bin:{m.group(4)}"
            elif m.group(3):
                key = f"test:{m.group(3)}"
            else:
                key = f"other:{m.group(4)}"
            continue
        if DOCTESTS.match(l):
            doc = True
            continue
        m = RESULT.match(l)
        if m:
            p, f, i, t = int(m.group(1)), int(m.group(2)), int(m.group(3)), float(m.group(4))
            if doc:
                totals[3] += p
                continue
            totals[0] += p
            totals[1] += f
            totals[2] += i
            per[key] = (p, t)
    return "cargo-test", totals, per


def parse_nextest(lines):
    per = {}
    passed = failed = ignored = 0
    for l in lines:
        m = NX_TEST.match(l)
        if m:
            status, secs, bid = m.group(1), float(m.group(2)), m.group(3)
            k = nextest_key(bid)
            p, t = per.get(k, (0, 0.0))
            per[k] = (p + (status == "PASS"), t + secs)
            continue
        m = NX_SUMMARY.match(l)
        if m:
            rest = m.group(1)
            passed, failed, ignored = field(rest, "passed"), field(rest, "failed"), field(rest, "skipped")
    return "nextest", [passed, failed, ignored, 0], per


def main():
    path = sys.argv[1]
    with open(path, encoding="utf-8", errors="replace") as fh:
        lines = fh.read().splitlines()
    runner, totals, per = parse(lines)
    print(runner, *totals, len(per))
    if "--per-binary" in sys.argv:
        for k in sorted(per):
            print(f"{k}\t{per[k][0]}\t{per[k][1]:.2f}")


if __name__ == "__main__":
    main()
