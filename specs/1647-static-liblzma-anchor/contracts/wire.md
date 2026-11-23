# Wire contract: a statically linked liblzma is versioned from the version it embeds

## SBOM documents

There is no new field, annotation, catalog row or technique (FR-006). A component versioned under FR-002 is identical in shape to spec 1644's liblzma match:
- `pkg:generic/liblzma@<version>`;
- `waybill:evidence-kind = embedded-version-string`, with the existing heuristic confidence;
- identity method `binary-analysis` (spec 1646);
- the existing CPE rows for `pkg:generic/liblzma`.

## Fixture cases (`waybill-cli/tests/version_string_static.rs`)

Each file is synthetic: `waybill-fixture-*` names, a `.rodata` region, and `.symtab`/`.strtab` with no `.dynsym` unless stated.

| Case | ELF type | `.symtab` | Region | Expected |
|---|---|---|---|---|
| S1 | exec | defines `lzma_version_string` | `\x005.4.5\x00` | `pkg:generic/liblzma@5.4.5` |
| S2 | exec | defines `lzma_version_number` | `\x005.4.5\x00` | `pkg:generic/liblzma@5.4.5` |
| S3 | exec | `lzma_version_string` **undefined** | `\x005.4.5\x00` | no liblzma component |
| S4 | exec | defines `lzma_version_string` | `\x005.4.5\x005.2.5\x00` | no liblzma component; census `rule_a.verdict = ambiguous` |
| S5 | exec | defines `lzma_version_string` | `GNU MP:\x005.4.5\x00` | neither gmp nor liblzma (spec 1644 FR-005a). **Red at the log-only commit**: there, as on `main`, it gives `pkg:generic/gmp@5.4.5`, and the census shows `current` with gmp `versioned` against `rule_a` with gmp and liblzma `ambiguous-shared` |
| S6 | dyn, `.dynsym` and `.symtab` both define it | | `\x005.4.5\x00` | `liblzma@5.4.5`, byte-identical to `main` |
| S7 | exec | `.symtab` holds only unrelated names | `\x005.4.5\x00` | no liblzma component, byte-identical to `main` |
| S8 | exec | none (stripped) | `\xFD7zXZ\x00` and `\x005.4.5\x00` | no liblzma component, and no census event: FR-004 was rejected (T010) |

Every case also runs `waybill sbom parity-check` over its three documents (SC-005).

## Census event `liblzma_static`

Target `waybill::version_census`, level DEBUG, with fields `path` and `liblzma_static` (a JSON string). For example:

```json
{"anchors":[{"binding":"global","name":"lzma_version_number","table":"symtab"},
            {"binding":"global","name":"lzma_version_string","table":"symtab"}],
 "current":[],
 "elf_type":"exec",
 "rule_a":[{"candidates":["5.4.5"],"library":"liblzma","verdict":"versioned","version":"5.4.5"}]}
```

- `current`, `rule_a` and `rule_b` each list every anchored library's verdict, so a library that a rule versions, and one it un-versions, are both visible.
- `rule_b` appears instead of `rule_a` on a stripped file at the log-only commit, with `magic` (the offset).
- **At the wired commit** (`541e2bff`, after FR-004 was rejected): `rule_b` and `magic` no longer exist. The event fires only for a binary anchored by `.symtab` alone, and `rule_a` equals `current`.
- `elf_type` is absent for a non-ELF binary. Rule B is not specific to ELF.
- Keys are sorted. Absent rules are omitted, not null.
- The existing `anchored` event keeps its fields. Each entry of `anchors` with `"kind":"symbol"` gains `"table"` and `"binding"`, and nothing else changes.
