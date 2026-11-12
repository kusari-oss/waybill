# Contract: C219 `waybill:ghc-boot-libraries`

**Scope:** document. **Encoding:** a JSON object serialized as a string, with keys sorted. This is the C202/C217 shape, kept identical in every format so no extractor has to compensate (the C19 lesson).

**Carriers:** CycloneDX `metadata.properties[]`; SPDX 2.3 document `annotations[]`; SPDX 3 document `Annotation`. **Parity:** `SymmetricEqual`, with an extractor in each format.

**Present when:** the closure tier ran (C184 present) **and** Haskell was in play. That means either the m926 pass ran (C174 present), **or** the closure names at least one GHC (a member's `nativeBuildInputs` holds a `ghc-X.Y.Z` or `ghc-binary-X.Y.Z` store path).

*Corrected at implementation (966-impl):* the second branch originally read "the m1275 identity pass examined Haskell members (C201 present)". C201 is not Haskell-specific: `closure_identity::reclassify` annotates every closure-appended `pkg:generic` member, and with no package set every member gets `package-set-unavailable`, so C201 is present on a closure with no Haskell at all and could not keep those documents byte-identical. "The closure names a GHC" is the fact the C201 proxy stood for.

The second branch exists for closure-only scans. On shellcheck's golden, m926 never runs (no C174), yet the identity pass annotates 2,045 components and every versionless boot component there is closure-emitted.

Otherwise the row is absent, which means "not applicable", so documents with no closure, or with a closure but no Haskell, are byte-identical (FR-011). It is built in `scan_cmd` from `ClosureCompilers` and the table date, never from `enrich`'s summary, which doesn't exist on a closure-only scan.

## Shape

```json
{
  "compilers": { "ghc-9.10.3": "in-table", "ghc-binary-9.8.4": "in-table" },
  "project": "ghc-9.10.3",
  "schema": 1,
  "system": "x86_64-linux",
  "table": "2026-11-12"
}
```

## Rules

- `compilers` holds every distinct GHC the closure's members were built with. Each value is `in-table` or `absent-from-table` (FR-008).
- `{}` means the closure ran and identified no GHC.
- `project` is the compiler research R2 assigns to the project's own declarations, or `null`.
- `table` is the embedded table's observation date, so a reader can tell how stale the data behind every C218 is.
- `system` is the closure's evaluation system, which decided platform-specific rows (research R4).

## Catalog row

KEEP-NO-NATIVE. No CDX 1.6, SPDX 2.3 or SPDX 3.0.1 construct records which compilers a build used, or which reference data a tool consulted. Existing rows were audited and rejected:
- C174 records what the nixpkgs enrichment did, and folding this in would make one row mean two things (the C202 rationale).
- C216 is the closure's scope decision.
- C217 is the evaluation policy.
