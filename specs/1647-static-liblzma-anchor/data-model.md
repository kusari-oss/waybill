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

Emitted for an ELF file when at least one holds:
- **Rule A**: `.symtab` defines an anchor symbol that `.dynsym` does not;
- **Rule B** (log-only commit; removed if FR-004 is rejected): the region holds `FD 37 7A 58 5A 00`, and no table defines an anchor symbol.

| Field | Meaning |
|---|---|
| `elf_type` | `exec`, `dyn`, `rel` or `other` |
| `anchors` | every `AnchorSymbol`: `{name, table, binding}` |
| `magic` | the magic's offset in the region, or absent (log-only commit) |
| `rule_a` | the liblzma verdict with `.symtab` anchors included: `{verdict, version?, candidates}`, or absent when rule A does not apply |
| `rule_b` | the liblzma verdict the magic would give, in the same shape, or absent (log-only commit) |

The verdict is spec 1644's: `versioned`, `ambiguous`, `ambiguous-shared` or `no-candidate`.

## States across commits

| Commit | `.symtab` anchors versioned? | Magic evaluated? | Output vs `main` |
|---|---|---|---|
| Log-only | no (census only) | yes (census only) | byte-identical |
| Wired | yes | no, unless FR-004 is adopted | changes only where rule A's verdict is `versioned` |
