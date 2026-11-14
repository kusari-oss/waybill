# Edge and role delta: `nix-closure-openai` (FR-007)

Before is ac2bd28. After is this branch with US1 and US2. Both were read by the binary from the capture in `README.md`: roles from `debug closure-dump`, edges from the `s1367` accounting. Three runs of each on moat gave byte-identical output.

## Roles (unit: derivations)

| transition | rung | count |
|---|---|---|
| `fragment` → `package` | `default` | 27 |
| `fragment` → `package` | `setup-hook-package` | 37 |
| `fragment` → `scaffolding` | `script-name` | 3 |
| **total changed** | | **67** |

- Every changed member was `fragment` before. No other transition occurs. The check asserts this, and it aborts on any other.
- **SC-002:** 0 members are `fragment` after the change, and 0 of them are on the `live-bootstrap-builder` rung, so 0 lack the evidence.
- → `package` (`default`): `bootstrap-stage-xgcc-stdenv-linux` ×2, `bootstrap-stage0-glibc-iconv-bootstrapFiles`, `bootstrap-stage0-stdenv-linux`, `bootstrap-stage1-stdenv-linux` ×2, `bootstrap-stage2-stdenv-linux` ×2, `bootstrap-stage3-stdenv-linux` ×2, `bootstrap-stage4-stdenv-linux` ×2, `bootstrap-tools`, `expand-response-params` ×4, `mirrors-list`, `nuke-references` ×5, `remove-references-to`, `stdenv-linux` ×2, `uname`
- → `package` (`setup-hook-package`): `autoreconf-hook` ×4, `die-hook` ×2, `ensure-newer-sources-hook`, `find-xml-catalogs-hook`, `install-shell-files`, `make-binary-wrapper-hook`, `make-shell-wrapper-hook` ×2, `pypa-build-hook.sh` ×2, `pypa-install-hook` ×2, `pytest-check-hook`, `python-catch-conflicts-hook`, `python-imports-check-hook.sh`, `python-namespaces-hook.sh`, `python-output-dist-hook`, `python-remove-bin-bytecode-hook`, `python-remove-tests-dir-hook`, `python-runtime-deps-check-hook.sh` ×2, `setup-debug-info-dirs-hook`, `setuptools-build-hook`, `tcl-package-hook`, `unittest-check-hook`, `update-autotools-gnu-config-scripts-hook` ×7, `wrap-python-hook`
- → `scaffolding` (`script-name`): `python-setup-hook.sh` ×3

## Edges (unit: edges)

| | before | after | Δ |
|---|---|---|---|
| inputs.drvs edges, derivation level | 10,041 | 10,041 | +0 |
| from-unemitted | 293 | 23 | -270 |
| walked-through | 3,420 | 270 | -3,150 |
| to-unemitted-stop | 0 | 0 | +0 |
| direct | 6,328 | 9,748 | +3,420 |
| projected, derivation level | 58,556 | 10,016 | -48,540 |
| projected, identity level (shipped) | 48,336 | 9,478 | -38,858 |
| also in collapsed | 5,883 | 9,223 | +3,340 |
| added by walking through | 42,453 | 255 | -42,198 |
| collapsed edges not shipped | 3,620 | 280 | -3,340 |

### Attribution

- **Conservation.** The derivation-level total is unchanged (10,041). The bucket deltas sum to zero: -270 `from-unemitted`, -3,150 `walked-through` and +3,420 `direct`.
- **Why an edge changes bucket.** Only a reclassified member changes whether an edge stops or passes through.
  - A member that moved to `package` is now an edge endpoint, so an edge into it stops there (`walked-through` → `direct`).
  - The edges out of it now start at an endpoint (`from-unemitted` → `direct`).
  - A member that moved to `scaffolding` is walked through, as `fragment` was, so it changes no bucket.
- **Shipped edges.**
  - Edges added by walking through change by -42,198: walks that used to pass through a former fragment now stop at it.
  - Shipped edges also present in the collapse change by +3,340: edges into and out of the new endpoints.
  - The edge-by-edge check against the regenerated golden is in `../golden-review.md` (SC-003).
