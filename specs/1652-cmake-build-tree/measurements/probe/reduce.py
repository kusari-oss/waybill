"""Spec 1652 T003: reduce one probe build tree to a committed test fixture.

Usage: python3 -I -B reduce.py <run.sh output dir> <run name> <fixture dir>

Writes <fixture dir>/source/ (the probe's app sources) and <fixture dir>/build/
holding only what waybill reads (research R9):
- every CMakeCache.txt (including _deps/*-subbuild ones, which the reader
  must ignore), compile_commands.json and file-API reply JSON, with absolute
  paths replaced by placeholders the tests substitute:
  @BUILD@ (the build dir), @SRC@ (the app source dir), @PREFIX@ (the install
  prefix of the "system" configuration) and @DEPS@ (the local dependency dir);
- every directory of the build tree, so _deps/<lc>-src/ and *-stamp/ exist,
  each holding an empty `.fixture-keep` so git keeps it;
- every file under a *-stamp/ directory, as an empty file (only names are
  read).
"""
import os
import shutil
import sys

KEEP = ".fixture-keep"


def rewrite(text, run):
    for old, new in (
        ("/work/b/" + run, "@BUILD@"),
        ("/probe/project/app", "@SRC@"),
        ("/work/prefix-" + run.split("-")[0], "@PREFIX@"),
        ("/work/deps", "@DEPS@"),
    ):
        text = text.replace(old, new)
    return text


def main(out, run, fixture):
    here = os.path.dirname(os.path.abspath(__file__))
    if os.path.exists(fixture):
        shutil.rmtree(fixture)
    shutil.copytree(os.path.join(here, "project", "app"), os.path.join(fixture, "source"))
    build = os.path.join(fixture, "build")
    os.makedirs(build)
    with open(os.path.join(out, run + ".listing.tsv")) as f:
        rows = [line.rstrip("\n").split("\t", 2) for line in f]
    for kind, _size, path in rows:
        rel = path[2:] if path.startswith("./") else path
        if not rel or rel == ".":
            continue
        dest = os.path.join(build, rel)
        if kind == "d":
            # Object-file and CMake-internal trees add nothing the reader uses.
            if "/CMakeFiles/" in "/" + rel + "/" and not rel.endswith("pkgRedirects"):
                continue
            os.makedirs(dest, exist_ok=True)
            open(os.path.join(dest, KEEP), "w").close()
        elif kind == "f" and "-stamp/" in rel:
            os.makedirs(os.path.dirname(dest), exist_ok=True)
            open(dest, "w").close()
    meta = os.path.join(out, run)
    for root, _dirs, files in os.walk(meta):
        for name in files:
            src = os.path.join(root, name)
            rel = os.path.relpath(src, meta)
            if name in ("configure.log", "build.log") or "-stamp" in rel:
                continue
            dest = os.path.join(build, rel)
            os.makedirs(os.path.dirname(dest), exist_ok=True)
            with open(src, encoding="utf-8", errors="surrogateescape") as f:
                text = f.read()
            with open(dest, "w", encoding="utf-8", errors="surrogateescape") as f:
                f.write(rewrite(text, run))
    total = sum(os.path.getsize(os.path.join(r, n)) for r, _d, fs in os.walk(fixture) for n in fs)
    count = sum(len(fs) for _r, _d, fs in os.walk(fixture))
    print("%s: %d files, %d bytes" % (fixture, count, total))


if __name__ == "__main__":
    main(*sys.argv[1:4])
