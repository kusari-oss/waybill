# Contract — transitive runtime closure (milestone 985 / issue #962)

waybill's external interfaces are its **CLI surface** and its **emitted SBOM
documents**. This file states what this feature adds to each, and what must not
move. Contract numbers (C-n) are referenced by tasks and tests.

---

## CLI surface

### C-1 — the opt-out flag

```
--no-nixpkgs-haskell-closure
```

| | |
|---|---|
| default | absent — the closure **runs** (FR-016) |
| effect when present | closure suppressed; milestone 926 declared-dependency resolution continues |
| relationship to `--no-nixpkgs-haskell` | that flag disables the whole pass, including declared resolution; this one is the narrower sibling |
| output when present | **byte-identical** to pre-feature output for the same project and revision (FR-017) |

**Not permitted**: a mode where the closure runs but declared resolution does
not. The closure is defined over the declared set; without it there is nothing
to close over.

---

## Emitted documents

### C-2 — component origin, per component

Every Haskell component the resolver touched carries its origin.

| format | carrier |
|---|---|
| CycloneDX | `components[].properties[]` entry |
| SPDX 2.3 | package-scope `annotations[]` via the `MikebomAnnotationCommentV1` envelope |
| SPDX 3 | package-scope `Annotation` element |

- **Key**: `waybill:nixpkgs-component-origin`
- **Values**: `declared` | `transitive` — a closed set
- **Presence**: on declared components too, not only transitive ones (FR-006a)
- **Directionality**: `SymmetricEqual` — the same value in all three formats
- **Native-carrier audit**: none exists in any format; see research R3. Required
  by Principle V before adding a `waybill:` key.

### C-3 — closure summary, document scope

- **Key**: `waybill:nixpkgs-haskell-closure`
- **Shape**: a JSON object with deterministic key order
- **Fields**: `declared`, `transitive`, `unresolved` (reason → count),
  `relations-walked`
- **Presence**: emitted when the closure ran; **absent** when it did not, so a
  non-Haskell scan stays byte-identical (FR-015)
- **Directionality**: `SymmetricEqual`

Both C-2 and C-3 need a catalog row in `docs/reference/sbom-format-mapping.md`
**and** three parity extractors, or `every_catalog_row_has_an_extractor` and
`holistic_parity` fail. The row and the extractors land together; a doc row
ahead of emission code is a known way to redden the gate.

### C-4 — dependency edges

For each resolved relation, an edge from the parent package to the dependency.

| | |
|---|---|
| CycloneDX | an entry in the parent's `dependencies[].dependsOn[]` |
| SPDX 2.3 | `DEPENDS_ON` from parent to dependency |
| SPDX 3 | a `Relationship` element |

**Invariants**

- **C-4.1** Every endpoint names a component present in the document
  (invariant I2, FR-008). Enforced already by `document_integrity.rs` per-PR and
  by corpus layer 0 nightly — this feature must not be the thing that trips them.
- **C-4.2** The `from` is the **actual parent**, never the document root
  (FR-009).
- **C-4.3** If component identities are rewritten after edges are built, the
  endpoints are rewritten in the same step, via `apply_renames` (#981).
  Milestone 980 is what happens otherwise.

### C-5 — versionless components for unresolvable names

A name that cannot be resolved is emitted as a component with no version,
carrying `waybill:haskell-version-unresolved-reason` — the same key and closed
reason vocabulary milestone 926 uses for declared dependencies (FR-005/FR-005a).

No new reason values are introduced by this feature. If the closure needs one,
that is a deliberate vocabulary change requiring its own catalog treatment.

---

## What must not move

These are regression surfaces, each with an existing gate.

| # | Invariant | Gate |
|---|---|---|
| C-6 | A scan with no `flake.lock`, a moving reference, no Haskell dependencies, or `--no-nixpkgs-haskell` produces output byte-identical to pre-feature (FR-015) | `m926_no_flake_lock_leaves_the_document_unchanged`, SC-007 |
| C-7 | With `--no-nixpkgs-haskell-closure`, output is byte-identical to pre-feature (FR-017) | SC-009, checked against committed corpus goldens **before** regeneration |
| C-8 | Two scans of one project at one revision are byte-identical (FR-013) | `m926_two_scans_of_one_revision_are_byte_identical` |
| C-9 | Declared-dependency versions, hashes and reasons are unchanged by this feature | the m926 suite in full |
| C-10 | No dangling edge endpoint in any format | `document_integrity.rs`, corpus layer 0 |

---

## Verification obligations

Stated so tasks cannot quietly omit them.

1. **Every new assertion must be mutation-tested** — shown to fail when the
   behaviour it guards is reverted. A test that has never failed has not been
   shown to test anything.
2. **The corpus goldens must be regenerated through CI**, never locally (rule
   zero, `docs/development/refreshing-corpus-goldens.md`). The existing Haskell
   corpus target will change substantially; its diff must be attributed
   category by category before the goldens are accepted.
3. **SC-009 must be checked before the goldens are regenerated.** After
   regeneration the pre-feature comparison no longer exists.
4. **SC-002 is checked against the oracle**, not against waybill's own parse —
   `measurements/nix_closure_oracle.sh` on a project with a populated cache.
