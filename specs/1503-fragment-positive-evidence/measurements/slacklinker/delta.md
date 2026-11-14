# Edge and role delta: `nix-closure-slacklinker` (FR-007)

Before is ac2bd28. After is this branch with US1 and US2. Both were read by the binary from the capture in `README.md`: roles from `debug closure-dump`, edges from the `s1367` accounting. Three runs of each on moat gave byte-identical output.

## Roles (unit: derivations)

| transition | rung | count |
|---|---|---|
| `fragment` → `package` | `default` | 20 |
| `fragment` → `scaffolding` | `script-name` | 15 |
| **total changed** | | **35** |

- Every changed member was `fragment` before. No other transition occurs. The check asserts this, and it aborts on any other.
- **SC-002:** 215 members are `fragment` after the change, and 215 of them are on the `live-bootstrap-builder` rung, so 0 lack the evidence.
- → `package` (`default`): `bin`, `bootstrap-stage0-glibc-iconv-minimal-bootstrap`, `cargo-deps-vendor`, `expand-response-params` ×6, `fetch-cargo-vendor-util`, `libbfd-plugin-api-header`, `mirrors-list`, `nuke-refs` ×5, `remove-references-to`, `replace-workspace-values`, `uname`
- → `scaffolding` (`script-name`): `add-flags.sh` ×2, `builder.pl`, `python-setup-hook.sh` ×3, `pythoncheck.sh` ×4, `setup-hook.sh` ×3, `utils.bash` ×2

## Edges (unit: edges)

| | before | after | Δ |
|---|---|---|---|
| inputs.drvs edges, derivation level | 16,657 | 16,657 | +0 |
| from-unemitted | 2,225 | 2,166 | -59 |
| walked-through | 1,706 | 468 | -1,238 |
| to-unemitted-stop | 0 | 0 | +0 |
| direct | 12,726 | 14,023 | +1,297 |
| projected, derivation level | 13,467 | 14,709 | +1,242 |
| projected, identity level (shipped) | 12,169 | 13,416 | +1,247 |
| also in collapsed | 11,952 | 13,235 | +1,283 |
| added by walking through | 217 | 181 | -36 |
| collapsed edges not shipped | 3,897 | 2,614 | -1,283 |

### Attribution

- **Conservation.** The derivation-level total is unchanged (16,657). The bucket deltas sum to zero: -59 `from-unemitted`, -1,238 `walked-through` and +1,297 `direct`.
- **Why an edge changes bucket.** Only a reclassified member changes whether an edge stops or passes through.
  - A member that moved to `package` is now an edge endpoint, so an edge into it stops there (`walked-through` → `direct`).
  - The edges out of it now start at an endpoint (`from-unemitted` → `direct`).
  - A member that moved to `scaffolding` is walked through, as `fragment` was, so it changes no bucket.
- **Shipped edges.**
  - Edges added by walking through change by -36: walks that used to pass through a former fragment now stop at it.
  - Shipped edges also present in the collapse change by +1,283: edges into and out of the new endpoints.
  - The edge-by-edge check against the regenerated golden is in `../golden-review.md` (SC-003).
