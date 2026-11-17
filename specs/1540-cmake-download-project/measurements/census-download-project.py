"""Census of DownloadProject's download_project() in a source tree (waybill
spec 1540, #1594).

For every CMakeLists.txt and *.cmake file under <root> (skipping .git):
  * each definition of a function or macro named download_project: its file,
    its cmake_parse_arguments prefix and keyword lists (resolving literal
    set() lists in the body), the template it configure_file()s, whether that
    template exists beside it, and a hash of the file, so vendored copies
    that differ show up;
  * each call: file, line, PROJ, URL / URL_HASH / GIT_REPOSITORY / GIT_TAG,
    whether those are literal, whether the tag looks like a branch rather
    than a release or a commit, and whether the call sits under an if(),
    in a loop, or in a function body.

Reads files only; evaluates nothing. Usage: download_project.py <root> <out.json>
"""
import collections
import hashlib
import importlib.util
import json
import pathlib
import re
import sys

ROOT = pathlib.Path(sys.argv[1]).resolve()
OUT = pathlib.Path(sys.argv[2])
_here = pathlib.Path(__file__)
_spec = importlib.util.spec_from_file_location("fetches", _here.with_name("fetches.py"))
fetches = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(fetches)
_spec = importlib.util.spec_from_file_location("wrappers", _here.with_name("wrappers.py"))
wrappers = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(wrappers)

KEYS = ("PROJ", "URL", "URL_HASH", "GIT_REPOSITORY", "GIT_TAG")
SHA = re.compile(r"^[0-9a-f]{7,40}$")
RELEASE = re.compile(r"^v?\d+(\.\d+)*([-._][0-9A-Za-z.]+)?$|^(rocm|release)[-_].*\d", re.I)


def tag_kind(tag):
    if tag is None:
        return None
    if "${" in tag:
        return "variable"
    if SHA.match(tag):
        return "commit"
    if RELEASE.match(tag):
        return "release-like"
    return "branch-like"


def main():
    files = sorted(fetches.cmake_files(ROOT))
    defs, calls = [], []
    for path in files:
        rel = str(path.relative_to(ROOT))
        try:
            text = path.read_text(errors="replace")
        except OSError:
            continue
        stack, fn = [], []
        body = None
        for name, args, line in fetches.commands(text):
            ws = [w for w, _ in wrappers.words(args)]
            if name in ("function", "macro"):
                fn.append(ws[0] if ws else "")
                if ws and ws[0].lower() == "download_project":
                    body = {"file": rel, "line": line, "kind": name, "sets": {}, "parse": None, "configure_file": [],
                            "file_sha256": hashlib.sha256(text.encode()).hexdigest()[:16]}
                continue
            if name in ("endfunction", "endmacro"):
                if body is not None and fn and fn[-1].lower() == "download_project":
                    defs.append(body)
                    body = None
                if fn:
                    fn.pop()
                continue
            if name == "if":
                stack.append("if")
            elif name == "endif" and stack and stack[-1] == "if":
                stack.pop()
            elif name in ("foreach", "while"):
                stack.append("loop")
            elif name in ("endforeach", "endwhile") and stack and stack[-1] == "loop":
                stack.pop()
            if body is not None:
                if name == "set" and ws:
                    body["sets"].setdefault(ws[0], []).append(ws[1:])
                elif name == "cmake_parse_arguments":
                    body["parse"] = ws
                elif name == "configure_file" and ws:
                    body["configure_file"].append(ws[0])
                continue
            if name == "download_project":
                kw = {}
                for i, w in enumerate(ws):
                    if w in KEYS and i + 1 < len(ws) and w not in kw:
                        kw[w] = ws[i + 1]
                calls.append({"file": rel, "line": line, "keywords": kw,
                              "identity_literal": any(k in kw and "${" not in kw[k] for k in ("URL", "GIT_REPOSITORY")),
                              "tag_kind": tag_kind(kw.get("GIT_TAG")),
                              "conditional": "if" in stack, "in_loop": "loop" in stack,
                              "in_function": fn[-1] if fn else None})

    for d in defs:
        p = d["parse"] or []
        resolved = {}
        if p and p[0] != "PARSE_ARGV" and len(p) >= 4:
            d["prefix"] = p[0]
            for label, w in zip(("options", "one", "multi"), p[1:4]):
                m = re.fullmatch(r"\$\{([A-Za-z0-9_]+)\}", w)
                if m:
                    vals = d["sets"].get(m.group(1))
                    resolved[label] = [x for x in vals[0] if x] if vals and len(vals) == 1 else ("UNRESOLVED" if vals else [])
                else:
                    resolved[label] = [x for x in w.split(";") if x]
        d["keywords"] = resolved
        d["template"] = d["configure_file"][0] if d["configure_file"] else None
        name = d["template"].rsplit("/", 1)[-1] if d["template"] else None
        d["template_name"] = name
        d["template_beside"] = bool(name) and (ROOT / pathlib.Path(d["file"]).parent / name).is_file()
        d["shape"] = d.get("prefix") == "DL_ARGS" and "PROJ" in (resolved.get("one") or []) and name == "DownloadProject.CMakeLists.cmake.in"
        del d["sets"]

    summary = {
        "cmake_files": len(files),
        "definitions": len(defs),
        "definitions_with_shape": sum(1 for d in defs if d["shape"]),
        "distinct_definition_files": len({d["file_sha256"] for d in defs}),
        "keyword_lists": dict(collections.Counter(json.dumps(d["keywords"], sort_keys=True) for d in defs)),
        "templates_beside_definition": sum(1 for d in defs if d["template_beside"]),
        "calls": len(calls),
        "calls_with_literal_identity": sum(1 for c in calls if c["identity_literal"]),
        "calls_without_identity": sum(1 for c in calls if not c["keywords"].get("URL") and not c["keywords"].get("GIT_REPOSITORY")),
        "calls_with_variable_identity": sum(1 for c in calls if (c["keywords"].get("URL") or c["keywords"].get("GIT_REPOSITORY")) and not c["identity_literal"]),
        "tag_kinds": dict(collections.Counter(c["tag_kind"] for c in calls)),
        "calls_conditional": sum(1 for c in calls if c["conditional"]),
        "calls_in_loop": sum(1 for c in calls if c["in_loop"]),
        "calls_in_function": sum(1 for c in calls if c["in_function"]),
        "distinct_identities": len({(c["keywords"].get("GIT_REPOSITORY") or c["keywords"].get("URL"), c["keywords"].get("GIT_TAG")) for c in calls}),
    }
    OUT.write_text(json.dumps({"summary": summary, "definitions": defs, "calls": calls}, indent=1))
    print("## download_project census\n")
    for k, v in summary.items():
        print(f"- **{k}**: `{json.dumps(v)[:600]}`")
    print("\n### Definitions\n")
    print("| file | shape | prefix | keywords | template | beside | sha |")
    print("|---|---|---|---|---|---|---|")
    for d in defs:
        print(f'| `{d["file"]}:{d["line"]}` | {d["shape"]} | {d.get("prefix")} | `{json.dumps(d["keywords"])}` | `{d["template_name"]}` | {d["template_beside"]} | {d["file_sha256"]} |')
    print("\n### Calls\n")
    print("| site | PROJ | identity | tag | tag kind | hash | cond | loop | function |")
    print("|---|---|---|---|---|---|---|---|---|")
    for c in calls:
        k = c["keywords"]
        ident = k.get("GIT_REPOSITORY") or k.get("URL") or ""
        print(f'| `{c["file"]}:{c["line"]}` | `{k.get("PROJ")}` | `{ident}` | `{k.get("GIT_TAG")}` | {c["tag_kind"]} | {"yes" if k.get("URL_HASH") else "no"} | {c["conditional"]} | {c["in_loop"]} | {c["in_function"]} |')


if __name__ == "__main__":
    main()
