"""Census of CMake fetch wrappers in a source tree (waybill spec 1539, #1518).

A fetch wrapper is a function() or macro() whose body calls
FetchContent_Declare, ExternalProject_Add, or another fetch wrapper. For each:
  * where it is defined, how many times, and whether the definitions differ;
  * its parameters, its cmake_parse_arguments calls, and the defaults its body
    sets for parsed variables;
  * how the caller's arguments reach the fetch call in its body (the
    forwarding shapes): ${ARGN}, ${ARGV}, ${<P>_UNPARSED_ARGUMENTS}, a parsed
    keyword re-passed by name (URL ${<P>_URL}), a parameter used as the name;
  * every call site: file, line, whether it passes URL / URL_HASH /
    GIT_REPOSITORY / GIT_TAG, whether those are literal, whether the name is
    literal, and the name waybill spec 1539 FR-005 would give a URL fetch.
Also: commands that are neither built-in fetches nor wrappers but pass a URL
or GIT_REPOSITORY keyword (wrappers defined outside the tree, or shapes this
census does not see: spec 1539 FR-010).

Reads files only; evaluates nothing. Usage: wrappers.py <root> <out.json>
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
_spec = importlib.util.spec_from_file_location("fetches", pathlib.Path(__file__).with_name("fetches.py"))
fetches = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(fetches)

FETCH = {"fetchcontent_declare", "externalproject_add"}
IDENTITY = ("URL", "URL_HASH", "GIT_REPOSITORY", "GIT_TAG")
# waybill's parse_version_from_url (cmake.rs).
VERSION_RE = re.compile(r"[-_/]v?([0-9]+\.[0-9]+(?:\.[0-9]+)?)")
ARCHIVE_EXT = re.compile(r"(\.tar\.(gz|bz2|xz|zst)|\.tgz|\.tbz2|\.txz|\.zip|\.tar|\.7z)$", re.I)


def words(args):
    """CMake argument splitting: quoted strings are one word, quotes removed;
    bracket arguments are one word. Records whether each word was quoted."""
    out, i, n = [], 0, len(args)
    while i < n:
        c = args[i]
        if c.isspace():
            i += 1
        elif c == '"':
            j, buf = i + 1, []
            while j < n and args[j] != '"':
                if args[j] == "\\" and j + 1 < n:
                    buf.append(args[j : j + 2])
                    j += 2
                else:
                    buf.append(args[j])
                    j += 1
            out.append(("".join(buf), True))
            i = j + 1
        elif c == "[" and fetches.BRACKET_OPEN.match(args, i):
            m = fetches.BRACKET_OPEN.match(args, i)
            close = "]" + m.group(1) + "]"
            j = args.find(close, m.end())
            j = n if j < 0 else j
            out.append((args[m.end() : j], True))
            i = j + len(close)
        elif c == "#":
            j = args.find("\n", i)
            i = n if j < 0 else j
        else:
            j = i
            depth = 0
            while j < n and not (args[j].isspace() and depth == 0):
                if args[j] == "(":
                    depth += 1
                elif args[j] == ")":
                    depth -= 1
                j += 1
            out.append((args[i:j], False))
            i = j
    return out


def parse_arguments(ws):
    """cmake_parse_arguments(<P> <opts> <one> <multi> <args...>) or
    (PARSE_ARGV <N> <P> <opts> <one> <multi>)."""
    t = [w for w, _ in ws]
    if t and t[0] == "PARSE_ARGV" and len(t) >= 6:
        argv, rest = t[1], t[2:6]
    elif len(t) >= 4:
        argv, rest = None, (t[0:4] + [""])[:4]
    else:
        return None
    split = lambda s: [x for x in s.split(";") if x] if s else []
    return {"prefix": rest[0], "parse_argv": argv, "options": split(rest[1]), "one": split(rest[2]),
            "multi": split(rest[3]), "forwarded": " ".join(t[4:]) if argv is None else None}


def file_name(url, version):
    """Spec 1539 FR-005: the archive filename's name when it parses as
    <name>-<version> for the version the URL gives."""
    base = url.rstrip("/").rsplit("/", 1)[-1].split("?", 1)[0]
    stem = ARCHIVE_EXT.sub("", base)
    if not version:
        return None, base
    m = re.match(r"^(.+?)[-_]v?" + re.escape(version) + r"$", stem)
    return (m.group(1) if m and m.group(1) else None), base


def main():
    files = sorted(fetches.cmake_files(ROOT))
    defs = collections.defaultdict(list)   # name_lower -> [definition]
    calls = []                             # every non-control command, with context
    for path in files:
        rel = str(path.relative_to(ROOT))
        try:
            text = path.read_text(errors="replace")
        except OSError:
            continue
        stack, current = [], None
        for name, args, line in fetches.commands(text):
            if name in ("function", "macro"):
                ws = words(args)
                d = {"name": ws[0][0] if ws else "", "kind": name, "file": rel, "line": line,
                     "params": [w for w, _ in ws[1:]], "body": [], "outer": current}
                stack.append(["fn", d])
                current = d
                continue
            if name in ("endfunction", "endmacro"):
                if stack and stack[-1][0] == "fn":
                    d = stack.pop()[1]
                    defs[d["name"].lower()].append(d)
                    current = d["outer"]
                continue
            if name == "if":
                stack.append(["if", None])
            elif name == "endif" and stack and stack[-1][0] == "if":
                stack.pop()
            elif name in ("foreach", "while"):
                stack.append(["loop", None])
            elif name in ("endforeach", "endwhile") and stack and stack[-1][0] == "loop":
                stack.pop()
            if current is not None:
                current["body"].append((name, args, line))
            calls.append({"name": name, "args": args, "file": rel, "line": line,
                          "in_function": current["name"] if current else None,
                          "in_loop": any(k == "loop" for k, _ in stack),
                          "conditional": any(k == "if" for k, _ in stack)})

    # Fixed point: a wrapper's body calls a fetch built-in or a wrapper.
    wrappers = set()
    while True:
        new = {n for n, ds in defs.items() if n not in wrappers
               and any(c in FETCH or c in wrappers for d in ds for c, _, _ in d["body"])}
        if not new:
            break
        wrappers |= new

    wrapper_rows = []
    for n in sorted(wrappers):
        for d in defs[n]:
            pa = [p for c, a, _ in d["body"] if c == "cmake_parse_arguments" for p in [parse_arguments(words(a))] if p]
            prefixes = {p["prefix"] for p in pa}
            inner = [(c, a, l) for c, a, l in d["body"] if c in FETCH or c in wrappers]
            shapes, sites = set(), []
            for c, a, l in inner:
                ws = words(a)
                text = " ".join(w for w, _ in ws)
                s = set()
                if re.search(r"\$\{ARGN\}", text):
                    s.add("argn")
                if re.search(r"\$\{ARGV\}", text):
                    s.add("argv")
                for p in prefixes:
                    if f"${{{p}_UNPARSED_ARGUMENTS}}" in text:
                        s.add("unparsed")
                    for i, (w, _) in enumerate(ws[1:], 1):
                        m = re.fullmatch(r"\$\{" + re.escape(p) + r"_([A-Za-z0-9_]+)\}", w)
                        if m and m.group(1) != "UNPARSED_ARGUMENTS":
                            prev = ws[i - 1][0]
                            s.add("named-keyword" if prev == m.group(1) else "parsed-variable")
                first = ws[0][0] if ws else ""
                if any(first == f"${{{q}}}" for q in d["params"]) or re.fullmatch(r"\$\{ARGV[0-9]+\}", first):
                    s.add("name-param")
                literal_identity = any(w in ("URL", "GIT_REPOSITORY") and i + 1 < len(ws) and "${" not in ws[i + 1][0]
                                       for i, (w, _) in enumerate(ws))
                if literal_identity:
                    s.add("literal-identity")
                other_vars = sorted({v for v in re.findall(r"\$\{([A-Za-z0-9_]+)\}", text)
                                     if v not in ("ARGN", "ARGV") and v not in d["params"]
                                     and not any(v.startswith(p + "_") for p in prefixes)})
                shapes |= s
                sites.append({"line": l, "calls": c, "shapes": sorted(s), "other_variables": other_vars,
                              "args": " ".join(text.split())[:400]})
            defaults = sorted({" ".join(a.split())[:160] for c, a, _ in d["body"] if c == "set"
                               and any(words(a) and words(a)[0][0].startswith(p + "_") for p in prefixes)})
            body_hash = hashlib.sha256("\n".join(f"{c}({' '.join(a.split())})" for c, a, _ in d["body"]).encode()).hexdigest()[:12]
            wrapper_rows.append({"name": d["name"], "kind": d["kind"], "file": d["file"], "line": d["line"],
                                 "params": d["params"], "parse_arguments": pa, "defaults_set": defaults,
                                 "inner_calls": sites, "shapes": sorted(shapes), "body_hash": body_hash,
                                 "nested_in": d["outer"]["name"] if d["outer"] else None})

    # A body that calls a fetch with literal arguments only forwards
    # nothing: it is a direct fetch in a function body (spec 1538), not a
    # wrapper (spec 1539 edge case).
    forwarding = {r["name"].lower() for r in wrapper_rows if set(r["shapes"]) - {"literal-identity"}}
    for r in wrapper_rows:
        r["forwards"] = bool(set(r["shapes"]) - {"literal-identity"})
    site_rows = []
    for c in calls:
        if c["name"] not in forwarding:
            continue
        ws = words(c["args"])
        kw = {}
        for i, (w, _) in enumerate(ws):
            if w in IDENTITY and i + 1 < len(ws) and w not in kw:
                kw[w] = ws[i + 1][0]
        url = kw.get("URL")
        version = None
        if url and "${" not in url:
            m = VERSION_RE.search(url)
            version = m.group(1) if m else None
        fname, base = file_name(url, version) if url and "${" not in url else (None, None)
        name = ws[0][0] if ws else ""
        site_rows.append({"wrapper": c["name"], "file": c["file"], "line": c["line"],
                          "in_function": c["in_function"], "in_loop": c["in_loop"], "conditional": c["conditional"],
                          "name": name, "name_literal": "${" not in name and "$<" not in name,
                          "keywords": {k: v for k, v in kw.items()},
                          "identity_literal": any(k in kw and "${" not in kw[k] for k in ("URL", "GIT_REPOSITORY")),
                          "url_version": version, "archive": base, "fr005_name": fname})

    other = collections.Counter()
    other_examples = {}
    builtin_like = FETCH | wrappers | {"set", "list", "string", "if", "elseif", "message", "file", "option",
                                       "cmake_parse_arguments", "foreach", "while", "function", "macro"}
    for c in calls:
        if c["name"] in builtin_like:
            continue
        ws = [w for w, _ in words(c["args"])]
        if any(w in ("URL", "GIT_REPOSITORY") and i + 1 < len(ws) for i, w in enumerate(ws)):
            other[c["name"]] += 1
            other_examples.setdefault(c["name"], f'{c["file"]}:{c["line"]}')

    dup = {n: [[r["file"], r["line"], r["body_hash"]] for r in wrapper_rows if r["name"].lower() == n]
           for n in wrappers if len(defs[n]) > 1}
    by_wrapper = collections.Counter(s["wrapper"] for s in site_rows)
    top_level = [s for s in site_rows if not (s["in_function"] and s["in_function"].lower() in wrappers)]
    summary = {
        "cmake_files": len(files),
        "functions_calling_a_fetch": len(wrappers),
        "functions_calling_a_fetch_without_forwarding": sorted(r["name"] for r in wrapper_rows if not r["forwards"]),
        "wrappers": len(forwarding),
        "wrapper_definitions": sum(len(defs[n]) for n in wrappers),
        "wrappers_defined_more_than_once": {n: len(defs[n]) for n in sorted(wrappers) if len(defs[n]) > 1},
        "wrappers_defined_more_than_once_with_differing_bodies":
            sorted(n for n in wrappers if len({r["body_hash"] for r in wrapper_rows if r["name"].lower() == n}) > 1),
        "shapes_by_wrapper": {r["name"] + "@" + r["file"] + ":" + str(r["line"]): r["shapes"] for r in wrapper_rows},
        "call_sites": len(site_rows),
        "call_sites_inside_a_wrapper_body": len(site_rows) - len(top_level),
        "call_sites_by_wrapper": dict(by_wrapper.most_common()),
        "top_level_sites_with_literal_identity": sum(1 for s in top_level if s["identity_literal"]),
        "top_level_sites_without_identity": sum(1 for s in top_level if not s["keywords"].get("URL") and not s["keywords"].get("GIT_REPOSITORY")),
        "top_level_sites_with_variable_identity": sum(1 for s in top_level if (s["keywords"].get("URL") or s["keywords"].get("GIT_REPOSITORY")) and not s["identity_literal"]),
        "top_level_sites_name_not_literal": sum(1 for s in top_level if not s["name_literal"]),
        "top_level_sites_in_loop": sum(1 for s in top_level if s["in_loop"]),
        "top_level_sites_conditional": sum(1 for s in top_level if s["conditional"]),
        "top_level_url_sites_with_url_hash": sum(1 for s in top_level if s["keywords"].get("URL") and s["keywords"].get("URL_HASH")),
        "distinct_literal_urls": len({s["keywords"]["URL"] for s in top_level if s["keywords"].get("URL") and "${" not in s["keywords"]["URL"]}),
        "fr005_named_from_archive": sum(1 for s in top_level if s["fr005_name"]),
        "fr005_fallback_to_bound_name": sum(1 for s in top_level if s["identity_literal"] and s["keywords"].get("URL") and not s["fr005_name"]),
        "other_commands_passing_url_or_repository": dict(other.most_common(40)),
        "other_command_examples": {k: other_examples[k] for k, _ in other.most_common(40)},
    }
    OUT.write_text(json.dumps({"summary": summary, "wrappers": wrapper_rows, "call_sites": site_rows,
                               "duplicates": dup}, indent=1))
    print("## CMake fetch-wrapper census\n")
    for k, v in summary.items():
        print(f"- **{k}**: `{json.dumps(v)[:900]}`")
    print("\n### Wrapper definitions\n")
    print("| wrapper | defined at | params | parse_arguments | shapes |")
    print("|---|---|---|---|---|")
    for r in wrapper_rows:
        pa = "; ".join(f'{p["prefix"]} argv={p["parse_argv"]} one={p["one"]} multi={p["multi"]}' for p in r["parse_arguments"])
        print(f'| `{r["name"]}` | `{r["file"]}:{r["line"]}` | `{" ".join(r["params"])}` | `{pa}` | {", ".join(r["shapes"])} |')
    print("\n### Top-level call sites\n")
    print("| wrapper | site | first argument | URL / repo | version | FR-005 name |")
    print("|---|---|---|---|---|---|")
    for s in top_level:
        ident = s["keywords"].get("URL") or s["keywords"].get("GIT_REPOSITORY") or ""
        print(f'| `{s["wrapper"]}` | `{s["file"]}:{s["line"]}` | `{s["name"]}` | `{ident[:90]}` | {s["url_version"]} | {s["fr005_name"]} |')


if __name__ == "__main__":
    main()
