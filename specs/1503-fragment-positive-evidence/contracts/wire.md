# Wire contract: `fragment` requires evidence of a live-bootstrap function

No new key, value, field or annotation. Every wire change is a value or count change on an existing row.

## C206 `waybill:closure-member-role`

- **Values.** Unchanged: the five frozen values (FR-003).
- **Behaviour.**
  - A member reaching the last rung is `fragment` only if its builder is `kaem`.
  - Otherwise it is `package` if its `buildCommand` writes `nix-support/setup-hook`.
  - Otherwise it is `scaffolding` if its name ends in `.sh`, `.bash` or `.pl`.
  - Otherwise it is `package` (data-model rungs 5–8).
- **Carriers.** Unchanged in all three formats, and the parity row is unchanged (FR-008).
- **Catalog row** (`docs/reference/sbom-format-mapping.md`):
  - The evidence-order sentence ends with the four new rungs instead of "else `fragment`".
  - The `package`, `scaffolding` and `fragment` definitions are rewritten as in data-model.md.
  - "`fragment` is now said only of live-bootstrap functions" becomes a sentence that is true of all five regenerated goldens (FR-011).

## C184 `waybill:nix-closure`

Keys and shape are unchanged. Five values change because they are read back from the document:
- `member-roles`;
- `edge-ineligible-components`;
- `edges`;
- `identifiers.wanted`;
- `licenses.wanted`.

The last two change because members that become `package` join the coverage population (research R3).

## Dependency edges

- **New stopping points.** Each member that moves from `fragment` to `package` becomes a stopping point of the projection. Its consumers gain an edge to it, and edges that used to pass through it now start at it.
- **No effect from `scaffolding`.** A member that moves from `fragment` to `scaffolding` changes no edge, because both roles are walked through.
- **Unchanged.** No edge type, provenance or data type changes.

## `waybill:orphan-reason`

No new value. A member that becomes `package` no longer carries `closure-member-not-an-edge-endpoint`.

## Not on the wire

`waybill debug closure-dump` rows gain `member_role_rung` (research R6). Debug output is not a format contract.

## Docs

- **`docs/reference/nix-evaluation.md`:** the role table (definitions and counts), the evidence-order paragraph, and the #1502/#1511 sentences.
- **`CHANGELOG.md`:** one `Changed` entry, not adjacent to another unreleased entry's lines.
