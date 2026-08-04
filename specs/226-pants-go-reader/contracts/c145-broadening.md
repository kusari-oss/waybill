# Contract: C145 `waybill:pants-target` semantic broadening (m226)

**Row ID**: C145 (unchanged)
**Annotation key**: `waybill:pants-target` (unchanged)
**Row disposition**: **KEEP-NO-NATIVE** (unchanged)
**Directionality**: `SymmetricEqual` (unchanged)
**Order-sensitive**: `false` (unchanged)
**Value regex**: `^[^,\s]+(:[^,\s]+)?(,[^,\s]+(:[^,\s]+)?)*$` (unchanged)

---

## What changes

**Doc-only description update** to `docs/reference/sbom-format-mapping.md`
row C145 to record that milestone 226 also emits the annotation
on `pkg:golang/*` components enriched by the pants_go
enrichment pass.

**No code changes**:
- Extractor macros at `waybill-cli/src/parity/extractors/cdx.rs:867`,
  `spdx2.rs:622`, `spdx3.rs:682` are unchanged — they already
  match on annotation key regardless of ecosystem.
- Row registration at
  `waybill-cli/src/parity/extractors/mod.rs::EXTRACTORS`
  is unchanged — same `row_id`, same `label`, same 3-extractor
  triple.
- `parity::extractors::tests::every_catalog_row_has_an_extractor`
  remains green (no row_id count change).

## What does NOT change

- C145's row_id (stays `C145`)
- The annotation key `waybill:pants-target`
- The value shape (comma-sep, lex-sorted)
- The KEEP-NO-NATIVE disposition + rejected native-alternatives
  audit
- The 3 extractor macros
- The row's registration in the `EXTRACTORS` array

## Description-update wording

Append the following paragraph after the existing description
of C145 (which currently ends with "…for the m071 parity
gate."):

> **Also emitted by milestone 226 (feature `226-pants-go-reader`)**
> on `pkg:golang/*` components enriched by the pants_go
> enrichment pass. For those components, the value is the
> Pants target address(es) whose `go_binary` / `go_package` /
> `go_third_party_package` / `go_mod` declaration(s) own the
> Go module — either as an implicit `go_mod`-root owner
> (component's `source_path` lies under the `go_mod` BUILD
> file's directory) or an explicit `go_third_party_package(import_path=...)`
> or `go_binary(main=...)` / `go_package` match. Multi-owner
> merge rule and lex-sort semantics identical to the pants_shell
> case above. The extractor triple + parity semantics are
> unchanged — same
> `waybill_annotation/v1` envelope shape across CDX / SPDX 2.3 /
> SPDX 3.

## Rationale

The C145 wire signal — the annotation key + value shape —
is ecosystem-agnostic. m225's original description scoped the
semantic to "file-tier components emitted by the milestone-225
pants_shell reader", but this is prose documentation; the
extractor code has no such scope constraint. Splitting into
per-ecosystem catalog rows (C146 for Go, hypothetical C147 for
Docker, etc.) would violate the existing catalog's
one-row-per-annotation-key convention.

## Machine-verified invariants (post-broadening)

- `parity::extractors::tests::every_catalog_row_has_an_extractor`
  passes — same row_id count as pre-m226.
- `holistic_parity` tests unchanged (no new ecosystem-specific
  gates).
- All existing cross-format parity tests continue to pass — the
  annotation flows through the same extractor triple regardless
  of which reader / enrichment pass emitted it.

## Alternatives considered

- **Add sibling row C146 `waybill:pants-target-go` + separate
  extractor triple**: rejected. Wire signal is identical (same
  annotation key on the emitted component). Splitting would
  double the parity-work without adding operator value —
  operators grep for `waybill:pants-target`, not per-ecosystem
  variants.
- **Leave C145 description as-is and rely on the m226 spec for
  operator-facing documentation**: rejected. The
  `docs/reference/sbom-format-mapping.md` catalog is the
  canonical documentation surface for annotations; operators
  expect the row to describe every emission origin.

## Follow-ups

- If a future milestone adds `waybill:pants-target` emission on
  yet another PURL type (e.g., `pkg:docker/*` for a Pants
  Docker reader), the same doc-broadening pattern applies —
  append a new paragraph, no code changes needed.
