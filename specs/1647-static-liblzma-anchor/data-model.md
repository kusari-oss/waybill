# Data model: a statically linked liblzma is versioned from the version it embeds

Every entity is internal and per binary. None reaches an SBOM document (FR-006).

## AnchorSymbol (replaces `BinaryScan.anchor_symbols: Vec<String>`)

| Field | Type | Meaning |
|---|---|---|
| `name` | `String` | One of `ANCHOR_SYMBOLS` (`lzma_version_string`, `lzma_version_number`) |
| `table` | `SymbolTable` | `Dynsym`, `Symtab`, `MachoSymtab` or `PeExport` |
| `binding` | `Binding` | `Global`, `Weak`, `Local` or `Unknown`. ELF reports its binding; Mach-O externals are `Global`; PE exports are `Unknown` |

Rules:
- Only **defined** symbols are recorded (FR-002). An undefined entry in any table is never an `AnchorSymbol`.
- One name defined in both `.dynsym` and `.symtab` gives two entries. The verdict depends only on whether at least one exists, so the duplicate changes nothing. The census reports both.
- `SymbolTable` and `Binding` are matched exhaustively, with no wildcard arm.

`anchored_verdicts` keys on the names alone, as on `main`. Its `Anchor::Symbol` gains `table` and `binding`, which are used only in the census JSON.

## LiblzmaStatic (census only, DEBUG `waybill::version_census`)

Emitted for a binary of any format when at least one holds:
- **Rule A** (ELF only): `.symtab` defines an anchor symbol that `.dynsym` does not;
- **Rule B** (log-only commit; removed if FR-004 is rejected): the region holds `FD 37 7A 58 5A 00`, and no table defines an anchor symbol.

| Field | Meaning |
|---|---|
| `elf_type` | `exec`, `dyn`, `rel` or `other`; absent for a non-ELF binary |
| `anchors` | every `AnchorSymbol`: `{name, table, binding}` |
| `magic` | the magic's offset in the region; absent when there is none, and after rule B is removed |
| `rule_a` | with `.symtab` anchors included, the verdict of **every** anchored library: a list of `{library, verdict, version?, candidates}`. Absent when rule A does not apply |
| `rule_b` | the same list as the magic would give it; absent when rule B does not apply, and after it is removed |
| `current` | the same list as production computes it on this commit, so a rule's effect is `rule_a` (or `rule_b`) compared with `current`: a library newly versioned, or one losing its version |

**At the wired commit**, the event fires only for a binary with a `.symtab` anchor that no other table defines. `rule_a` equals `current`, and `rule_b` and `magic` are gone (FR-004 rejected). `BinaryScan` has no separate `.symtab` list: the `.symtab` anchors are part of `anchor_symbols`.

The verdict is spec 1644's: `versioned`, `ambiguous`, `ambiguous-shared` or `no-candidate`.

## States across commits

| Commit | `.symtab` anchors versioned? | Magic evaluated? | Output vs `main` |
|---|---|---|---|
| Log-only | no (census only) | yes (census only) | byte-identical |
| Wired | yes | no: FR-004 rejected (T010) | changes only where an anchored library's verdict changes: liblzma newly versioned, or GMP losing a shared candidate |
