"""Spec 1652 SC-001(a): tabulate what each captured build tree records.

Usage: python3 -I -B analyze.py <run.sh output dir>

For every run (CMake version x generator x configuration) and every
dependency the probe declares, report which build-tree signals are present.
The question is which signals separate taken / not taken / incomplete /
no evidence, and whether that is stable across versions and generators.
"""
import json
import os
import sys

DEPS_FC = ["hdronly", "urldep", "neverused", "extra", "bdep", "mixedcase"]
DEPS_EP = ["extproj", "extgit", "ExtMixed"]


def listing(out, run):
    rows = {}
    with open(os.path.join(out, run + ".listing.tsv")) as f:
        for line in f:
            kind, size, path = line.rstrip("\n").split("\t", 2)
            rows[path] = (kind, int(size))
    return rows


def cache(out, run):
    vals = {}
    p = os.path.join(out, run, "CMakeCache.txt")
    with open(p) as f:
        for line in f:
            line = line.rstrip("\n")
            if not line or line[0] in "#/" or "=" not in line:
                continue
            key, val = line.split("=", 1)
            name = key.split(":", 1)[0]
            vals[name] = (key.split(":", 1)[1] if ":" in key else "", val)
    return vals


def dir_class(val, run):
    if val is None:
        return "-"
    if "pkgRedirects" in val[1]:
        return "redirect"
    if val[1].startswith("/work/prefix"):
        return "system"
    return "other:" + val[1]


def fc_signals(rows, cv, run, d):
    stamp = "./_deps/%s-subbuild/%s-populate-prefix/src/%s-populate-stamp/" % (d, d, d)
    return {
        "src": "./_deps/%s-src" % d in rows,
        "SOURCE_DIR": (d + "_SOURCE_DIR") in cv,
        "FC_SOURCE_DIR_entry": ("FETCHCONTENT_SOURCE_DIR_" + d.upper()) in cv,
        "_DIR": dir_class(cv.get(d + "_DIR"), run),
        "populate-done": (stamp + d + "-populate-done") in rows,
    }


def ep_signals(rows, d):
    stamp = "./%s-prefix/src/%s-stamp/" % (d, d)
    return {
        "prefix": ("./%s-prefix" % d) in rows,
        "stamp-dir": stamp.rstrip("/") in rows,
        "download": (stamp + d + "-download") in rows,
        "done": (stamp + d + "-done") in rows,
        "src-nonempty": any(p.startswith("./%s-prefix/src/%s/" % (d, d)) for p in rows),
    }


def compile_includes(out, run):
    p = os.path.join(out, run, "compile_commands.json")
    if not os.path.exists(p):
        return None
    with open(p) as f:
        cmds = json.load(f)
    incs = set()
    for c in cmds:
        args = c.get("command", "").split() or c.get("arguments", [])
        for i, a in enumerate(args):
            if a.startswith("-I"):
                incs.add(a[2:] or args[i + 1])
            elif a == "-isystem":
                incs.add(args[i + 1])
    return len(cmds), sorted(incs)


def main(out):
    runs = sorted(r[: -len(".listing.tsv")] for r in os.listdir(out) if r.endswith(".listing.tsv"))
    print("# Probe results\n")
    for name in ("versions.txt", "image.txt", "runs.txt"):
        print("## %s\n\n```\n%s```\n" % (name, open(os.path.join(out, name)).read()))
    # Signals per dependency, grouped so identical rows across versions and
    # generators collapse: key = (config, dep), value = set of signal tuples.
    table = {}
    sizes = {}
    for run in runs:
        ver, gen, config = run.split("-", 2)
        rows = listing(out, run)
        cv = cache(out, run)
        total = sum(s for k, s in rows.values() if k == "f")
        sizes[run] = (len(rows), total, (".cmake/api/v1/reply" in "".join(rows)))
        for d in DEPS_FC:
            sig = tuple(sorted(fc_signals(rows, cv, run, d).items()))
            table.setdefault((config, "FetchContent", d), {}).setdefault(sig, []).append(ver + "/" + gen)
        for d in DEPS_EP:
            sig = tuple(sorted(ep_signals(rows, d).items()))
            table.setdefault((config, "ExternalProject", d), {}).setdefault(sig, []).append(ver + "/" + gen)
    print("## Signals per configuration and dependency\n")
    print("One line per distinct signal set; `runs` lists which version/generator pairs gave it.\n")
    for (config, kind, d), variants in sorted(table.items()):
        for sig, where in variants.items():
            s = ", ".join("%s=%s" % kv for kv in sig)
            print("- **%s** %s `%s`: %s  (runs: %d/6%s)" % (
                config, kind, d, s, len(where),
                "" if len(where) == 6 else ": " + " ".join(where)))
    print("\n## Build-tree size\n")
    print("| run | entries | bytes (files) | file-API reply |\n|---|---|---|---|")
    for run, (n, b, api) in sizes.items():
        print("| %s | %d | %d | %s |" % (run, n, b, "yes" if api else "no"))
    print("\n## compile_commands.json\n")
    for run in runs:
        ci = compile_includes(out, run)
        if ci is None:
            print("- %s: absent" % run)
        else:
            print("- %s: %d entries; include dirs: %s" % (run, ci[0], ", ".join(ci[1])))


if __name__ == "__main__":
    main(sys.argv[1])
