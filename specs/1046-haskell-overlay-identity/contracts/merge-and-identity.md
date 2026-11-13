# Contract: twin merge, Hackage-published identity, and VCS source

## CLI

| flag | effect |
|---|---|
| `--no-hackage-lookup` | No Hackage request. Candidates keep `pkg:generic` with C201 `hackage-unchecked-opted-out`. The merge still happens. |
| `--offline` | As above, with `hackage-unchecked-offline`. |

Neither flag changes anything for a scan without `--nix-closure`.

## Document rows

| row | change |
|---|---|
| C201 `waybill:closure-identity` | It gains five values: `hackage-published`, `hackage-unpublished`, `hackage-unchecked-offline`, `hackage-unchecked-opted-out` and `hackage-unreachable`. They apply only to a GHC-built member whose version the pinned set does not carry. `version-disagrees` keeps its meaning for every other member. |
| C221 `waybill:requirement-unsatisfied` (new) | A per-component JSON string, `{"requirement-ranges":[…],"version":"<v>"}`, with sorted keys. It is emitted only on a merged component whose version every declared range rejects. KEEP-NO-NATIVE (R7). Carried by CycloneDX `properties[]`, SPDX 2.3 annotation, and SPDX 3 annotation, through the `MikebomAnnotationCommentV1` envelope. A `SymmetricEqual` parity row. |
| A10 (VCS external reference) | No change to the row. New producer: closure source members, and the packages built from them. |
| A11 (distribution reference) | No change to the row. It is now also read from structured attributes. |

## Identity shapes

```text
published, VCS-built   pkg:hackage/slack-web@2.3.0.0?vcs_url=git%2Bhttps://<host>/<path>@<rev>   (exact encoding: R4 unit test)
published, tarball     pkg:hackage/<name>@<version>
otherwise              pkg:generic/<name>@<version>                                            (unchanged)
```

## Invariants

1. **No name-twins.** No document holds both a versionless `pkg:hackage/<n>` design-tier component and a merge-eligible build-tier `<n>` (FR-001, FR-004).
2. **No dangling edges.** No edge endpoint dangles in any format (FR-005).
3. **No Hackage identity for collisions.** No `pkg:hackage` identity is minted for a member that no GHC built (FR-003, R2).
4. **No Hackage identity for unpublished versions.** No `pkg:hackage` identity is minted for a version that Hackage answered 404 for, or that was not checked (FR-006a, FR-007).
5. **Byte-identical without the closure.** Scans without `--nix-closure` are byte-identical (FR-012).
