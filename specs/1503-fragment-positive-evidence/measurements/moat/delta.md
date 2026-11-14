# Edge and role delta: `nix-closure-moat` (FR-007)

Before is ac2bd28. After is this branch with US1 and US2. Both were read by the binary from the capture in `README.md`: roles from `debug closure-dump`, edges from the `s1367` accounting. Three runs of each on moat gave byte-identical output.

## Roles (unit: derivations)

| transition | rung | count |
|---|---|---|
| `fragment` → `package` | `default` | 15 |
| `fragment` → `scaffolding` | `script-name` | 10 |
| **total changed** | | **25** |

- Every changed member was `fragment` before. No other transition occurs. The check asserts this, and it aborts on any other.
- **SC-002:** 215 members are `fragment` after the change, and 215 of them are on the `live-bootstrap-builder` rung, so 0 lack the evidence.
- → `package` (`default`): `bootstrap-stage0-glibc-iconv-minimal-bootstrap`, `expand-response-params` ×6, `mirrors-list`, `nuke-refs` ×5, `remove-references-to`, `uname`
- → `scaffolding` (`script-name`): `add-flags.sh` ×2, `python-setup-hook.sh` ×3, `setup-hook.sh` ×3, `utils.bash` ×2

## Edges (unit: edges)

| | before | after | Δ |
|---|---|---|---|
| inputs.drvs edges, derivation level | 10,342 | 10,342 | +0 |
| from-unemitted | 2,184 | 2,146 | -38 |
| walked-through | 797 | 216 | -581 |
| to-unemitted-stop | 0 | 0 | +0 |
| direct | 7,361 | 7,980 | +619 |
| projected, derivation level | 7,772 | 8,368 | +596 |
| projected, identity level (shipped) | 6,891 | 7,489 | +598 |
| also in collapsed | 6,705 | 7,314 | +609 |
| added by walking through | 186 | 175 | -11 |
| collapsed edges not shipped | 2,956 | 2,347 | -609 |

### Attribution

- **Conservation.** The derivation-level total is unchanged (10,342). The bucket deltas sum to zero: -38 `from-unemitted`, -581 `walked-through` and +619 `direct`.
- **Why an edge changes bucket.** Only a reclassified member changes whether an edge stops or passes through.
  - A member that moved to `package` is now an edge endpoint, so an edge into it stops there (`walked-through` → `direct`).
  - The edges out of it now start at an endpoint (`from-unemitted` → `direct`).
  - A member that moved to `scaffolding` is walked through, as `fragment` was, so it changes no bucket.
- **Shipped edges.**
  - Edges added by walking through change by -11: walks that used to pass through a former fragment now stop at it.
  - Shipped edges also present in the collapse change by +609: edges into and out of the new endpoints.
  - The edge-by-edge check against the regenerated golden is in `../golden-review.md` (SC-003).
