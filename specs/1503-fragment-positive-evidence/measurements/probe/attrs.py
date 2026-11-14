"""Attributes of the closure members `waybill debug closure-dump` places on
the last rung (spec 1503, research R1/R2/R7).

Roles come from the binary, never from this script: it only joins
closure-dump's rows to derivation attributes the reader does not parse
(`builder`, `NIX_MAIN_PROGRAM`, `isExecutable`, `stdenv`).

    python3 -I -B attrs.py <closure.json> <closure-dump.json> [--role fragment]

Prints one TSV row per member: drv, name, member_role, builder basename,
builder package, has-stdenv, has-main-program, is-executable.
"""
import json
import re
import sys


def attr(v, key):
    env = v.get("env") or {}
    sa = v.get("structuredAttrs") or {}
    return key in env or key in sa


def main(argv):
    closure = json.load(open(argv[1]))["derivations"]
    dump = json.load(open(argv[2]))
    role = argv[argv.index("--role") + 1] if "--role" in argv else "fragment"
    for r in dump["rows"]:
        if r.get("member_role") != role:
            continue
        key = r["drv"].rsplit("/", 1)[-1]
        v = closure.get(key) or closure.get(r["drv"])
        if v is None:
            print(f"MISSING\t{r['drv']}", file=sys.stderr)
            return 1
        builder = v.get("builder") or ""
        pkg = re.sub(r"^/nix/store/[a-z0-9]{32}-", "", builder).split("/", 1)[0]
        print("\t".join([
            key, r.get("name") or "", role,
            builder.rsplit("/", 1)[-1], pkg,
            str(attr(v, "stdenv")), str(attr(v, "NIX_MAIN_PROGRAM")),
            str(attr(v, "isExecutable")),
        ]))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
