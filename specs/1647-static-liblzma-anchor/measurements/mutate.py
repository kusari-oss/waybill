"""Spec 1647 T013 mutations (SC-004, research R7). Each one: back up every
file it edits with cp, apply its exact replacements, run its killing tests
one cargo invocation per filter, classify, restore with cp. A mutation that
does not compile is reported as such, never as killed; a filter that runs no
test is a harness error. Copied from spec 1646's runner; only the mutation
table differs.

usage: python3 -I -B mutate.py REPO OUTDIR [M1,M2,...]
"""
import pathlib
import shutil
import subprocess
import sys

REPO = pathlib.Path(sys.argv[1])
OUT = pathlib.Path(sys.argv[2])
VS = "waybill-cli/src/scan_fs/binary/version_strings.rs"
SC = "waybill-cli/src/scan_fs/binary/scan.rs"
IT = ["--test", "version_string_static"]


def unit(path):
    return ["--bin", "waybill", path]


def it(name):
    return IT + [name]


def anchored(name):
    return ["--test", "version_string_anchored", name]


M = [
    ("M1 undefined .symtab symbols anchor",
     [(SC, "            file.symbols()\n                .filter(|s| !s.is_undefined())\n", "            file.symbols()\n")],
     [it("s3_"), it("importer_undefined_in_both_tables")]),
    ("M2 the .symtab read is dropped",
     [(SC, "    anchor_symbols.extend(symtab_anchor_symbols);", "    let _ = symtab_anchor_symbols;")],
     [it("s1_"), it("s2_"), it("s5_")]),
    ("M3 the .strtab gate never matches",
     [(SC, 'memchr::memmem::find(d, b"lzma_version_")', 'memchr::memmem::find(d, b"lzma_version_\\x01")')],
     [it("s1_"), it("s2_")]),
    ("M4 an anchor in both tables is reported as .symtab-only",
     [(VS, "        s.table == SymbolTable::Symtab\n            && !anchor_symbols.iter().any(|d| d.name == s.name && d.table != SymbolTable::Symtab)",
       "        s.table == SymbolTable::Symtab")],
     [it("s6_"), unit("scan_fs::binary::version_strings::tests::s1647_rule_a_absent_when_dynsym_already_anchors")]),
    ("M5 a .dynsym anchor is labelled symtab",
     [(SC, "Some(AnchorSymbol { name: name.to_string(), table: SymbolTable::Dynsym, binding: binding_of(&s) })",
       "Some(AnchorSymbol { name: name.to_string(), table: SymbolTable::Symtab, binding: binding_of(&s) })")],
     [anchored("defined_anchor_symbol_anchors_elf_and_macho"), it("s6_")]),
]


def cargo(args):
    r = subprocess.run(["cargo", "test", "-p", "waybill", *args], cwd=REPO, capture_output=True, text=True)
    return r.returncode, r.stdout + r.stderr


def run(mid, edits, tests):
    key = mid.split()[0]
    files = sorted({f for f, _, _ in edits})
    texts = {f: (REPO / f).read_text() for f in files}
    for f, old, _ in edits:
        if texts[f].count(old) != 1:
            return f"{mid}: NOT APPLIED (anchor count {texts[f].count(old)} in {f} for {old.strip()[:60]!r})"
    backups = {}
    for f in files:
        backups[f] = OUT / f"{key}.{pathlib.Path(f).name}.orig"
        shutil.copyfile(REPO / f, backups[f])
    try:
        mutated = dict(texts)
        for f, old, new in edits:
            mutated[f] = mutated[f].replace(old, new)
        for f in files:
            (REPO / f).write_text(mutated[f])
        kills, logs = [], []
        for t in tests:
            code, log = cargo(t)
            logs.append(log)
            if any(line.startswith("error[E") for line in log.splitlines()):
                (OUT / f"{key}.log").write_text("\n".join(logs))
                return f"{mid}: COMPILE ERROR (not a kill)"
            if not any(l.startswith("test result:") for l in log.splitlines()):
                (OUT / f"{key}.log").write_text("\n".join(logs))
                return f"{mid}: HARNESS ERROR, a test command ran no tests ({t})"
            ran = [l for l in log.splitlines() if l.startswith("test ") and " ... " in l]
            if not ran:
                (OUT / f"{key}.log").write_text("\n".join(logs))
                return f"{mid}: HARNESS ERROR, filter {t[-1]} matched no test"
            failed = [l.split()[1] for l in ran if l.endswith("FAILED")]
            if code != 0 and failed:
                kills.extend(failed)
        (OUT / f"{key}.log").write_text("\n".join(logs))
        return f"{mid}: KILLED by {', '.join(kills)}" if kills else f"{mid}: SURVIVED"
    finally:
        for f in files:
            shutil.copyfile(backups[f], REPO / f)


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    only = set(sys.argv[3].split(",")) if len(sys.argv) > 3 else None
    results = [run(*m) for m in M if only is None or m[0].split()[0] in only]
    (OUT / "results.txt").write_text("\n".join(results) + "\n")
    print("\n".join(results))


if __name__ == "__main__":
    main()
