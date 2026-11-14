# Edge and role delta on the pinned moat closure (FR-005, SC-001, SC-002)

**Input.** The x86_64-linux `nix-closure-moat` closure:
- captured in public-corpus run 43535531153;
- sha256 `898fcdd38aa235840cc2a76b9ee4048ef826a853ce8abe19efdfb3076c8744ab`;
- 1,749 derivations.

**How it was read.** Through the reader itself, never re-modelled:
- `s1367_edge_accounting_over_a_real_closure` →
  `edge-accounting-{before,after}.txt`;
- `waybill debug closure-dump` → `closure-dump-{before,after}.json` and
  `patch-members-{before,after}.tsv`.

**When.** "Before" is the branch base, before any code change. "After" is
this feature. Each "after" figure was produced 3 times with byte-identical
output.

## Roles (unit: derivations, from closure-dump)

| C206 | before | after |
|---|---|---|
| `fragment` | 245 | **240** |
| `source` (no `pname`) | 553 | **558** |
| `package`, `check`, `scaffolding` | 23, 46, 267 | unchanged |
| `.patch`-named members that are `fragment` | 5 | **0** |

**SC-001: exactly 5 derivations move, all `fragment` → `source`.** They are
research R1's five:
- `001-fix-rpath.patch`;
- `catalog-legacy-uris.patch` (two derivations, `36dpg26b…` and `i5blxpry…`);
- `fix-build-time-run-tcl.patch`;
- `remove-hatch-plugins.patch`.

All five now report `edge_endpoint: true`. No other role count changes.

**SC-002: 0 members listed in `patches` are `fragment`.** Before the change
there were 5.

The C184 component counts (unit: components) are checked against the
regenerated golden in `golden-review.md`. The expected values are
`member-roles.fragment` 245 → 240, `member-roles.source` 506 → 511 and
`edge-ineligible-components` 512 → 507.

## Edges (unit: edges)

| | before | after | Δ |
|---|---|---|---|
| `inputs.drvs`, derivation level | 10,342 | 10,342 | 0 |
| ↳ `from-unemitted` | 2,195 | 2,184 | **−11** |
| ↳ `walked-through` | 802 | 797 | **−5** |
| ↳ `to-unemitted-stop` | 0 | 0 | 0 |
| ↳ `direct` | 7,345 | 7,361 | **+16** |
| `from-unemitted` source members | 512 | 507 | **−5** |
| projected, derivation level | 7,761 | 7,772 | +11 |
| **projected, identity level** (C184 `edges`) | **6,875** | **6,891** | **+16** |
| ↳ also in the collapse | 6,689 | 6,705 | +16 |
| ↳ added by walking through | 186 | 186 | 0 |
| collapsed edges not projected | 2,972 | 2,956 | −16 |

### Attribution

Every changed bucket is accounted for by the 5 members:

- **−5 `walked-through`.** Each of the 5 has one consumer edge into it, so 5
  edges stop being walked through and become `direct`.
- **−11 `from-unemitted`, and −5 source members.** The 11 edges out of the 5
  patches (to what built each patch file) start at an endpoint now. That makes
  them `direct`, and the 5 patches drop out of the non-endpoint sources.
- **+16 `direct`.** 5 + 11 = 16. Conservation holds: the derivation-level
  total is unchanged.
- **+16 shipped identity edges.** The counts are consistent with no
  previously shipped edge being lost.
  - "Added by walking through" is unchanged at 186.
  - "Also in the collapse" rose by exactly the shipped delta.
  - The edges the walk-through used to produce (consumer → patch-builder)
    would therefore all still be shipped, because each consumer reaches those
    builders directly as well.
  - *Counts cannot rule out one edge lost and another gained. The per-edge
    list is checked against the regenerated golden in `golden-review.md`
    (T021).*

Every derivation-level change is incident to one of the 5 reclassified
members. No edge changed for any other reason (SC-003, pending the
edge-by-edge check in T021).
