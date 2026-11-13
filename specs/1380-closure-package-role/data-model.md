# Data model: spec 1380 (#1374)

## MemberRole (C206), extended

| value | graph endpoint | assigned when (in order) |
|---|---|---|
| `source` | yes | 1. The member has a content hash (a fixed-output fetch), as before. 3a–3c: the version rung would fire, and E1, E2 or the source-name rule holds (new). |
| `scaffolding` | no | 2. `env.text` is present, as before. |
| `check` | yes | 3d. The version rung would fire, and the name has a `-get-version-` or `-simple-program-` segment followed by a version (new). |
| `package` | yes | 3e. The version rung fires and none of 3a–3d holds. |
| `fragment` | no | 4. Nothing else. |

- **Rung 3** is today's `name_encodes_a_version` test. Its sub-rungs, 3a to 3e, only split its population (research R5).
- **3a, E1:** the member's output basename is in `PatchOutputs`.
- **3b, E2:** `phases` is exactly `unpackPhase patchPhase installPhase`.
- **3c, source-name rule:** the name ends `-source`, `-src-<v>` or `-vendor`.

**Invariant.** Old role → new role is `package` → {`source`, `check`, `package`}, and nothing else. Every other member's role is unchanged, so the set of endpoint members is unchanged, and so is the graph.

## PatchOutputs

A set of store basenames: every store path listed in any closure derivation's `patches` attribute (`env` string or `structuredAttrs` list). It is built once per closure and read by `classify`. Its only effect is to make rung 3a fire.

## Identity basis

Unchanged: a member that reaches `source` by rung 3 keeps its recipe-hash identity (spec FR-003). Only rung 1 produces a content-hash identity.

## C184 `member-roles`

The map is seeded from `MemberRole::ALL`, so it gains `"check": <n>`, which is zero on a closure with no checks. `edge-ineligible` is unchanged, because `check` is an endpoint.
