# Contracts: milestone 164 — pnpm v9 multi-version edge disambiguation

**No new external contracts.**

Milestone 164 is a pure implementation fix at the pnpm-lock parser (`mikebom-cli/src/scan_fs/package_db/npm/pnpm_lock.rs`). The emitted SBOM's wire format (CDX 1.6 / SPDX 2.3 / SPDX 3.0.1) is byte-identical in SHAPE — only edge target PURLs change to reflect correct lockfile-declared versions.

Existing contracts unchanged:

- **CLI**: no new flags. `mikebom sbom scan` behaves identically from a CLI-argument perspective.
- **Emitted SBOM shape**: no new component types, no new annotation vocabularies, no new parity-catalog rows. Consumers reading milestone-163-era SBOMs read milestone-164-era SBOMs identically.
- **Parity catalog**: no new rows. Existing rows C1–C115 all applicable unchanged.
- **`mikebom:*` annotations**: no new annotations. FR-007 explicitly documents standards-native precedence — the fix reuses existing `name_to_purl` disambiguation infrastructure.
- **Tracing log convention**: extends the existing `pnpm-lock parsed` info-level log with 2 new fields (`multi_version_disambiguated_count`, `malformed_key_warn_count`) per FR-009. Backward-compat for regex consumers.

## SBOM consumer-observable delta

The only user-observable change is the CORRECTNESS of emitted `dependsOn` edge targets. Concrete example on `github.com/podman-desktop/podman-desktop`:

**Pre-164** (bug):
```json
{
  "ref": "pkg:npm/%40docsearch/react@3.9.0",
  "dependsOn": [
    "pkg:npm/%40algolia/autocomplete-core@1.19.8"   // ← WRONG
  ]
}
```

**Post-164** (correct):
```json
{
  "ref": "pkg:npm/%40docsearch/react@3.9.0",
  "dependsOn": [
    "pkg:npm/%40algolia/autocomplete-core@1.17.9"   // ← CORRECT
  ]
}
```

Consumer contract: none. Every consumer already reads `dependsOn` targets as opaque strings; milestone 164 just makes those strings correct. Existing vulnerability scanners, graph walkers, and license aggregators require zero code changes — they'll simply start returning correct results for pnpm-monorepo SBOMs.
