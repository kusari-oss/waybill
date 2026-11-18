# Audit: changed expectations and corpus movement

## Existing tests changed (T011)

| test | before | after | cause |
|---|---|---|---|
| `npm/bun_lock.rs::parse_bun_lock_workspace_edges_unchanged` (milestone 667's no-regression pin) | workspace root `depends` = bare names `@my/shared`, `@my/web` | their PURLs `pkg:npm/%40my/shared@0.5.0`, `pkg:npm/%40my/web@1.0.0` | Found while making W-2 pass. A bare name resolves in the depender's ecosystem; the root is `pkg:generic`, so the bun workspace root never had an edge. Measured on `main`: `bun sub op` has no edge out of `pkg:generic/waybill-fixture-mono`. This is the defect spec 1541 fixed for uv; the member list also moves from a `HashSet` to a sorted list. |
| `npm/bun_lock.rs::emits_workspace_shape` | same, sorted by the test | same PURLs, in the reader's own (now sorted) order | same |

The other suites T011 named (`scan_kotlin_dsl`, `scan_kmp_polyglot`, `dart_edge_cases`, `pip/uv_lock` unit tests) pass unchanged.

## Expectation corrected during implementation

- **W-6** expected C69 on a Kotlin workspace at the scan root. Milestone 127 FR-006 emits C69 only when the subject displaced a main module (`cyclonedx/metadata.rs:965`, "losers non-empty"). The Kotlin reader emits no main module, so nothing lost, and C69 is correctly absent. The test asserts absence; the contract row says why. uv and bun (W-4, W-5) displace their members and carry `workspace-root`.

## Probe (T012)

`workspace-roots.branch.out`: no stranded cell (the three `atroot plain` rows have no incoming edge because they are the subject), no `foreign-build-system`; `main` had 7 stranded (`workspace-roots.out`).

## Public corpus (T026)

`public-corpus.yml` with `repeat=2`: branch run 44231278845 and `main` run 44231285787, both success. All 21 targets printed `determinism: …: 2 scans, identical after masking in all 3 formats` at both refs, and the golden comparison passed, so **no golden changed** — as trace §1 predicted (no golden holds a workspace root), and no target has a link correlation.

## Quality corpus (T026)

`quality-corpus.yml`: branch run 44231282597, `main` run 44231289455; 18 targets each, no violations, no config errors. A sorted diff of every measured field except wall time moves one target:

| target | field | `main` | branch |
|---|---|---|---|
| `gradle-bitwarden-android` | edges | 150 | 151 |
| | sbom_bytes | 659,864 | 659,903 |

Packages, files, depth, flatness, completeness and the sbomqs score are unchanged.

**Traced (one variable):** the pinned commit (`d817f6b4`) scanned locally the harness's way (`--offline`, `--root-name`, empty `HOME`):
- branch binary: 151 edges; the one new edge is `gradle-bitwarden-android@d817f6b4bf7c → pkg:generic/Bitwarden@0.0.0`, the Kotlin DSL workspace root from `settings.gradle.kts`;
- the same binary with only mutation M1 applied (no workspace-root anchors): 150 edges, and `pkg:generic/Bitwarden@0.0.0` unreachable with **no** orphan reason. The masked documents differ only in that edge and the C105 count (234 → 233).

So the movement is FR-002 fixing #1601's defect on an unrelated repository: a Kotlin workspace root at the scan root, stranded under `--root-name`.
