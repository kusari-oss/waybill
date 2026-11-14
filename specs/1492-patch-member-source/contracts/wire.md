# Wire contract: Locally built patches are sources, not fragments

No new key, value, field or annotation. The wire changes are value and count changes on existing rows.

## C206 `waybill:closure-member-role`

- **Values:** unchanged, the five frozen values.
- **Behaviour:** a member listed in another closure derivation's `patches` attribute carries `source` whatever its name, unless the content hash already made it `source` (FR-001, FR-010).
- **Carriers:** unchanged in all three formats. The parity row is unchanged (FR-006).
- **Catalog row** (`docs/reference/sbom-format-mapping.md`), three edits:
  - The evidence-order sentence gains the `patches` rung before `env.text`.
  - "a member another derivation lists in `patches` … is `source`" moves out of the version-name rung's description.
  - "A member outside the rung keeps its role (#1502 tracks local patches that are `fragment`)" is removed.

## C184 `waybill:nix-closure`

The keys and shape are unchanged. `member-roles`, `edge-ineligible-components` and `edges` change value, because they are read back from the document.

## Dependency edges

Each reclassified member becomes a stopping point of the projection. Its consumers gain an edge to it. Edges that previously passed through it now start at it. No edge type, provenance or data type changes.

## `waybill:orphan-reason`

No new value. A reclassified member no longer carries `closure-member-not-an-edge-endpoint`.

## Docs

- **`docs/reference/nix-evaluation.md`:**
  - the role table's `fragment` count;
  - the evidence-order paragraph;
  - the "A member outside the rung keeps its role… (#1502)" sentence.
- **`CHANGELOG.md`:** one `Changed` entry, not adjacent to another unreleased entry's lines.
