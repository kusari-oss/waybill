"""Census of installed test declarations in the TheRock SDK fixture (spec 1533, research R1/R3/R4).

Usage: python3 -I -B test-census.py <fixture-clone>

  <fixture-clone>  a clone of kusari-sandbox/waybill-fixture-therock-sdk at or after
                   capture 5ded2fd (the first capture holding CTestTestfile.cmake)

Reads the captured CTestTestfile.cmake and exported-targets files plus
listing/all-files.txt. This is a measuring script, not a model of the reader:
waybill's own output is the authority (SC-001..SC-004).
"""

import collections
import json
import os
import posixpath
import re
import sys

clone = sys.argv[1]
root = os.path.join(clone, "sdk")

kinds, sizes = {}, {}
for line in open(os.path.join(clone, "listing", "all-files.txt")):
    parts = line.rstrip("\n").split(" ", 2)
    if len(parts) == 3:
        path = parts[2].split(" -> ")[0]
        path = path[2:] if path.startswith("./") else path
        kinds[path] = parts[0]
        sizes[path] = int(parts[1]) if parts[1].isdigit() else 0

INTERPRETERS = {"python", "python3", "bash", "sh", "perl", "env"}
TEST_DIRS = {"test", "tests", "unittests", "test_plugins", ".upstream-tests"}
TOKEN = re.compile(r'\[(=*)\[(.*?)\]\1\]|"((?:[^"\\]|\\.)*)"|([^\s()"#]+)', re.S)


def commands(text):
    text = re.sub(r"(?m)#(?!\[=*\[).*$", "", text)
    for m in re.finditer(r"([A-Za-z_][A-Za-z0-9_]*)\s*\((.*?)\)\s*(?=\n|$|[A-Za-z_])", text, re.S):
        args = [t.group(2) if t.group(2) is not None else t.group(3) if t.group(3) is not None else t.group(4)
                for t in TOKEN.finditer(m.group(2))]
        yield m.group(1).lower(), args


def test_command(args):
    if args and args[0] == "NAME":
        return args[args.index("COMMAND") + 1:] if "COMMAND" in args else []
    return args[1:]


def locate(dir_rel, cmd):
    """(outcome, root-relative path or None, form)."""
    if not cmd:
        return "unresolved", None, "empty"
    word, rest = cmd[0], cmd[1:]
    if posixpath.basename(word) in INTERPRETERS:
        script = next((a for a in rest if not a.startswith("-")), None)
        if script is None:
            return "unresolved", None, "interpreter-without-script"
        word, form = script, "interpreter:" + posixpath.basename(cmd[0])
    else:
        form = "direct"
    if "${" in word or "$<" in word:
        return "unresolved", None, form + "/variable"
    if word.startswith("/"):
        return "unresolved", None, form + "/absolute"
    if "/" not in word:
        return "unresolved", None, form + "/bare-name"
    p = posixpath.normpath(posixpath.join(dir_rel, word))
    if p.startswith(".."):
        return "outside-root", None, form + "/relative"
    if p not in kinds:
        return "not-shipped", p, form + "/relative"
    return "resolved", p, form + "/relative"


ctest_files = sorted(os.path.relpath(os.path.join(d, f), root)
                     for d, _, fs in os.walk(root) for f in fs if f == "CTestTestfile.cmake")
per_file, outcomes, forms, other_cmds = {}, collections.Counter(), collections.Counter(), collections.Counter()
declared = {}
for rel in ctest_files:
    d = posixpath.dirname(rel)
    n = collections.Counter()
    for name, args in commands(open(os.path.join(root, rel), errors="replace").read()):
        if name != "add_test":
            if name not in ("set_tests_properties",):
                other_cmds[name] += 1
            continue
        outcome, path, form = locate(d, test_command(args))
        outcomes[outcome] += 1
        forms[form] += 1
        n[outcome] += 1
        if path and outcome == "resolved":
            declared.setdefault(path, "ctest-declaration")
    per_file[rel] = dict(n)

# Exported targets of a test package: base name ends in -tests/_tests/-test/_test.
SUFFIX = re.compile(r"[-_]tests?$", re.I)
targets = [os.path.relpath(os.path.join(d, f), root) for d, _, fs in os.walk(root) for f in fs
           if re.search(r"((Targets|Exports)|-targets)(-[A-Za-z]+)?\.cmake$", f)]
test_packages = sorted({posixpath.dirname(t) for t in targets
                        if SUFFIX.search(re.sub(r"((Targets|Exports)|-targets)(-[A-Za-z]+)?\.cmake$", "", posixpath.basename(t)))})
raise_by_dir = collections.defaultdict(int)
for t in targets:
    if posixpath.dirname(t) in test_packages:
        txt = open(os.path.join(root, t), errors="replace").read()
        raise_by_dir[posixpath.dirname(t)] = max(raise_by_dir[posixpath.dirname(t)],
                                                 len(re.findall(r'get_filename_component\(_IMPORT_PREFIX "\$\{_IMPORT_PREFIX\}" PATH\)', txt)))
for t in targets:
    d = posixpath.dirname(t)
    if d not in test_packages:
        continue
    prefix = d
    for _ in range(raise_by_dir[d]):
        prefix = posixpath.dirname(prefix)
    for loc in re.findall(r'IMPORTED_LOCATION(?:_[A-Z]+)? "([^"]+)"', open(os.path.join(root, t), errors="replace").read()):
        p = posixpath.normpath(loc.replace("${_IMPORT_PREFIX}", prefix)).lstrip("/")
        if p in kinds:
            declared[p] = "exported-targets-test-package"


def under_test_dir(p):
    return any(part in TEST_DIRS for part in p.split("/")[:-1])


regular = {p for p, k in kinds.items() if k == "-"}
by_rule = collections.defaultdict(lambda: [0, 0])
for p in regular:
    rule = declared.get(p) or ("test-directory" if under_test_dir(p) else None)
    if rule:
        by_rule[rule][0] += 1
        by_rule[rule][1] += sizes[p]

# Name-pattern candidates (FR-009), files directly in bin/ not already covered above.
CANDIDATES = {
    "test_*": r"^test_", "*-test": r"-test$", "*_test": r"_test$", "*-tests": r"-tests$", "*_tests": r"_tests$",
    "*_gtest": r"_gtest$", "*_gtest.data": r"_gtest\.data$", "benchmark_*": r"^benchmark_", "*-bench": r"-bench$",
    "*-tune": r"-tune$",
}
bin_files = sorted(p for p in regular if posixpath.dirname(p) == "bin")
candidates = {}
for label, rx in CANDIDATES.items():
    hits = [p for p in bin_files if re.search(rx, posixpath.basename(p))]
    covered = [p for p in hits if p in declared]
    candidates[label] = {
        "files": len(hits), "bytes": sum(sizes[p] for p in hits),
        "already_declared": len(covered),
        "only_by_name": sorted(posixpath.basename(p) for p in hits if p not in declared),
    }

print(json.dumps({
    "ctest_files": len(ctest_files),
    "ctest_add_test": dict(outcomes),
    "ctest_forms": dict(forms),
    "ctest_other_commands": dict(other_cmds),
    "ctest_per_file": per_file,
    "test_packages": test_packages,
    "scoped_regular_files_by_rule": {k: {"files": v[0], "bytes": v[1]} for k, v in sorted(by_rule.items())},
    "name_pattern_candidates": candidates,
}, indent=1, sort_keys=True))
