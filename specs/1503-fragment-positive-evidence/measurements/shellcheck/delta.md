# Edge and role delta: `nix-closure-shellcheck` (FR-007)

Before is ac2bd28. After is this branch with US1 and US2. Both were read by the binary from the capture in `README.md`: roles from `debug closure-dump`, edges from the `s1367` accounting. Three runs of each on moat gave byte-identical output.

## Roles (unit: derivations)

| transition | rung | count |
|---|---|---|
| `fragment` → `package` | `default` | 15 |
| `fragment` → `scaffolding` | `script-name` | 11 |
| **total changed** | | **26** |

- Every changed member was `fragment` before. No other transition occurs. The check asserts this, and it aborts on any other.
- **SC-002:** 215 members are `fragment` after the change, and 215 of them are on the `live-bootstrap-builder` rung, so 0 lack the evidence.
- → `package` (`default`): `bootstrap-stage0-glibc-iconv-minimal-bootstrap`, `expand-response-params` ×6, `mirrors-list`, `nuke-refs` ×5, `remove-references-to`, `uname`
- → `scaffolding` (`script-name`): `add-flags.sh` ×2, `python-setup-hook.sh` ×3, `setup-hook.sh` ×3, `utils.bash` ×2, `utils.sh`

## Edges (unit: edges)

| | before | after | Δ |
|---|---|---|---|
| inputs.drvs edges, derivation level | 14,366 | 14,366 | +0 |
| from-unemitted | 2,186 | 2,148 | -38 |
| walked-through | 1,454 | 416 | -1,038 |
| to-unemitted-stop | 0 | 0 | +0 |
| direct | 10,726 | 11,802 | +1,076 |
| projected, derivation level | 11,337 | 12,390 | +1,053 |
| projected, identity level (shipped) | 10,224 | 11,274 | +1,050 |
| also in collapsed | 10,038 | 11,099 | +1,061 |
| added by walking through | 186 | 175 | -11 |
| collapsed edges not shipped | 3,605 | 2,544 | -1,061 |

### Attribution

- **Conservation.** The derivation-level total is unchanged (14,366). The bucket deltas sum to zero: -38 `from-unemitted`, -1,038 `walked-through` and +1,076 `direct`.
- **Why an edge changes bucket.** Only a reclassified member changes whether an edge stops or passes through.
  - A member that moved to `package` is now an edge endpoint, so an edge into it stops there (`walked-through` → `direct`).
  - The edges out of it now start at an endpoint (`from-unemitted` → `direct`).
  - A member that moved to `scaffolding` is walked through, as `fragment` was, so it changes no bucket.
- **Shipped edges.**
  - Edges added by walking through change by -11: walks that used to pass through a former fragment now stop at it.
  - Shipped edges also present in the collapse change by +1,061: edges into and out of the new endpoints.
  - The edge-by-edge check against the regenerated golden is in `../golden-review.md` (SC-003).
