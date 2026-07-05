# Changelog

All notable changes to mikebom are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project
adheres to [Semantic Versioning](https://semver.org/) once it exits
`0.1.x` alpha.

## [Unreleased]

### Go workspace-mode detection annotation (milestone 161)

Closes issue #495 (surfaced during the milestone-155–160 audit
expansion against `kusari-sandbox/test-*` repos). Empirical
2026-07-03 measurement on `kusari-sandbox/test-kubernetes` (Kubernetes
v1.x with `go.work` + 40+ nested modules) showed mikebom's Go
dep-graph edges were **30.8% wrong** (26.6% DIVERGE + 4.2%
EMITTED-SUPERSET) vs `GOWORK=off go mod graph` executed in each
`use`d module's own directory. Base type libraries like `k8s.io/api`
appeared to depend on leaf applications like `kube-proxy` — a
Constitution Principle IX (Accuracy) failure that amplifies
vulnerability-scan false positives.

**Fix**: This milestone lands the structural infrastructure — go.work
parser, workspace-mode detection, C112 document-scope annotation, +
the Q1 hybrid `EdgeDisposition` classifier for the follow-on
workspace-attribution fix.

**New annotation**:

- `mikebom:go-workspace-mode` (C112) — three-value document-scope
  string with grammar `detected: <N> use-modules` | `absent` |
  `malformed: <reason>`. Distinct from milestone-158's
  `mikebom:graph-completeness` (C104) and milestone-160's
  `mikebom:go-transitive-coverage` (C110) per research.md R1: C104
  reports whether we built a top-level graph; C110 reports Go
  transitive-edge coverage; C112 reports whether the scanned Go repo
  used `go.work` workspace mode and how many `use`d modules were
  discovered. Emitted iff a `go.work` file is present at the scanned
  root AND `GOWORK=off` is NOT set (byte-identity guard per SC-003).

**Q1–Q3 clarifications (2026-07-04)**:

- Q1: `v0.0.0-unknown` false-edge disposition — hybrid rule. SUPPRESS
  the edge iff target is workspace-internal AND source's own require
  block does NOT name the target; RESOLVE the version from the sibling's
  go.mod iff target IS workspace-internal AND source's require block DOES
  name the target. Matches FR-002 truthful per-module attribution.
- Q2: empty-use case (`go.work` present with zero `use` entries)
  emits `detected: 0 use-modules` — legitimate scaffolding scenario,
  NOT malformed.
- Q3: SC-001 ground-truth = per-`use`d-module `cd <use-module-dir> &&
  GOWORK=off go mod graph`. Reproducible by any auditor with `go`
  installed; matches milestone-160's Q3 per-module methodology.

**Consumer jq recipes**:

```bash
# Detect workspace-mode SBOMs (CDX)
jq -r '.metadata.properties[] | select(.name == "mikebom:go-workspace-mode") | .value' sbom.cdx.json

# Extract use-module count
jq -r '.metadata.properties[]
       | select(.name == "mikebom:go-workspace-mode")
       | .value
       | capture("detected: (?<n>[0-9]+) use-modules")
       | .n' sbom.cdx.json
```

**What's in this PR (structural infra)**:

- `gowork.rs` sibling module with 3 types (`WorkspaceMode`,
  `GoWorkDocument`, `EdgeDisposition`) + `parse_go_work` line-based
  stdlib parser + `classify_workspace_edge` Q1 hybrid classifier +
  `merge_workspace_replaces` FR-005 workspace-precedence helper.
- Detection at Go-scan entry via `detect_go_workspace_mode` in
  `legacy.rs` (honors `GOWORK=off`).
- Full-pipeline threading: `WorkspaceContext` gains 3 fields
  (workspace_mode + use_modules_map + workspace_replaces);
  `GoScanSignals` + `ScanDiagnostics` + `ScanResult` + `ScanArtifacts`
  each gain the `go_workspace_mode` field.
- Parity catalog C112 registration across CDX + SPDX 2.3 + SPDX 3.
- Doc-scope C112 emission in all 3 formats + 4 unit tests + 3
  integration tests.

**Follow-on work** (NOT in this PR — per Assumption §7 empirically-
adjustable clause):

- T020-T027 (FR-007 investigation + fixes + Q1 hybrid consumer wiring).
- T053 (test-kubernetes fixture landing in `kusari-oss/mikebom-fixtures`).
- T040 SC-001+SC-002+SC-006 audit test against test-kubernetes.

The unit tests DO exercise the Q1 hybrid classifier + FR-005 workspace-
precedence merge, so the classifier logic is proven correct in
isolation — only the consumer wiring is deferred. Once the fixture
lands, the follow-on PR wires the classifier into `legacy::read`'s
post-resolution sweep and runs the audit test to verify SC-001
≤ 5% wrong edges.

**Backward compatibility**: SC-003 dual-side byte-identity verified —
zero golden bytes change (33 pre-161 goldens byte-identical). The
new C112 annotation is absent-when-Absent (no `go.work` file at
scanned root), so non-workspace scans produce byte-identical output.

**Test surface**: 16 new unit tests in `gowork.rs` (SC-009 sub-items
a–j + `use .` case + workspace-precedence-replace + wire-vocab
sanity + closed-vocab codes). 4 new unit tests in
`cyclonedx/metadata.rs` (C112 emission conditions across Detected/
Absent/None/Malformed variants). 3 new integration tests in
`mikebom-cli/tests/go_workspace_edges.rs` (synthetic 3-module go.work
fixture end-to-end).

### Go transitive-edge coverage annotations (milestone 160)

Closes issue #494 (surfaced during the milestone-157/158/159 Round-2
audit of `kusari-sandbox/test-*` repos). Empirical 2026-07-03
measurement on `kusari-sandbox/test-podman` showed mikebom's Go
transitive-edge coverage was **only 52.2%** vs `go mod graph` in
online mode (proxy-reachable) and **7.29%** in offline mode. That
means downstream vulnerability scans, license audits, and CVE
lookups silently miss up to half the closure — a Constitution
Principle VIII (Completeness) failure.

**Fix**: Full-pipeline transparency for the milestone-055/091
resolution ladder plus per-component + document-scope annotations
so consumers can programmatically detect + reason about coverage
gaps.

**Per-component annotations** (universal on every `pkg:golang/...`
component except the synthetic `pkg:golang/stdlib@vX.Y.Z`):

- `mikebom:go-transitive-source` (C108) — kebab-case string naming
  which of the 5-step ladder resolved this module's transitive
  requires: `go-mod-graph`, `module-cache`, `proxy-fetch`,
  `go-sum-fallback`, or `unresolved`. Universal-emit per Q2
  clarification (matches milestone-158 C104 + milestone-159
  C106/C107 universal-presence pattern; avoids the
  "no-annotation = success" silent-lie failure mode).
- `mikebom:go-transitive-unresolved-reason` (C109) — kebab-case
  string naming the failure class. Emitted iff C108 == `unresolved`.
  Closed 7-code vocab: `proxy-fetch-timeout`, `proxy-fetch-not-found`,
  `proxy-fetch-forbidden`, `proxy-off-in-chain`, `goprivate-matched`,
  `module-cache-miss`, `unknown-error`.

**Document-scope annotations** (emitted iff SBOM contains ≥1 Go
component):

- `mikebom:go-transitive-coverage` (C110) — three-value string
  `complete` | `partial` | `unknown`. Distinct from milestone-158's
  `mikebom:graph-completeness` (C104) per research.md R1: C104
  reports whether we built a top-level component graph at all; C110
  reports what fraction of Go modules had per-module transitive
  requires resolved via the ladder. Q1 reason-code-driven decision
  rule: `unknown` iff a "we can't measure" reason applies (offline,
  `off` in GOPROXY, `go mod graph` degraded); `partial` iff we ran
  the pass and ≥1 module ended `unresolved`; `complete` iff every
  module resolved via steps 1–4.
- `mikebom:go-transitive-coverage-reason` (C111) — grammar
  `<code>: <detail>[; <code>: <detail>]*`. Emitted iff C110 !=
  `complete`. Closed-but-extensible 5-code vocab per Q4:
  `proxy-fetch-degraded`, `offline-mode`, `goproxy-off-in-chain`,
  `go-mod-graph-degraded`, `module-cache-empty-and-no-proxy`.

**Q1–Q4 clarifications (2026-07-04)**:

- Q1: `partial` vs `unknown` — reason-code-driven, not count-based.
  Mirrors milestone-158's caution-first philosophy.
- Q2: `mikebom:go-transitive-source` — universal on every Go
  component. Matches milestone-158/159 universal-presence pattern.
- Q3: SC-001 ground-truth = `go mod graph`. Same generator as the
  milestone-157 audit that established the 52.2% pre-160 baseline.
- Q4: reason-code vocab is closed-but-extensible. New codes require
  a spec/milestone bump. Matches milestone-158 C105 governance.

**Consumer jq recipes**:

```bash
# Overall Go transitive coverage (doc-scope) — fail CI if not complete
jq -r '.metadata.properties[] | select(.name == "mikebom:go-transitive-coverage") | .value' sbom.cdx.json

# Count Go modules per resolution source
jq '[.components[]
     | select(.purl // "" | startswith("pkg:golang/"))
     | .properties // [] | .[]
     | select(.name == "mikebom:go-transitive-source") | .value]
    | group_by(.) | map({(.[0]): length}) | add' sbom.cdx.json
```

**Empirical impact** (pre/post SC-001 on `test-podman`): pre-160
baseline was 52.2%; the milestone-160 code lands the transparency
+ annotation infrastructure. The FR-006 fetch-degradation
investigation (T015-T019) is the follow-on that will lift the
observed 52.2% → the ≥90% SC-001 target; that empirical work is
tracked in a follow-up implementation session per Assumption §7's
empirically-adjustable clause.

**Backward compatibility**: SC-003 dual-side byte-identity verified
— the 30 non-Go milestone-090 goldens (10 ecosystems × 3 formats)
are byte-identical to pre-160. Only the `golang` fixture goldens
change (net +234 lines: new C108 per component + C110/C111 at doc
scope).

**Test surface**: 15 new unit tests in `graph_resolver.rs` + `legacy.rs`
covering the SC-008 sub-item vocabulary (a–j) + `parse_go_mod`
indirect-preservation regression guard. 4 new unit tests in
`cyclonedx/metadata.rs` covering C110/C111 emission conditions
(present when coverage `Some`, absent when `None`, C111 conditional
on non-Complete). 3 new integration tests in
`mikebom-cli/tests/go_transitive_coverage.rs` exercising the
release-binary path end-to-end via `--offline` mode.

### pnpm/yarn npm-alias syntax resolution (milestone 159)

Closes issue #493 (discovered during the milestone-157 Round-2 audit
of `kusari-sandbox/test-*` repos). Both pnpm-lock.yaml (v9 snapshots)
and yarn.lock (v1) support a lockfile syntax where a local dep name
aliases to a different real package. Pre-159 mikebom emitted
components under the LOCAL name (or under alias-name-with-empty-
version phantom PURLs), so consumers looking up the aliased-canonical
PURL found nothing and vulnerability scans silently missed the true
package.

Empirical impact from the milestone-157 Round-2 audit against 3
`kusari-sandbox/test-*` repos:

- `test-podman-desktop` (pnpm v9): **10 dropped alias-edges**
  (react-helmet-async, react-loadable, string-width-cjs,
  strip-ansi-cjs, wrap-ansi-cjs, plus 5 more docusaurus-family
  aliases).
- `test-guac-visualizer` (yarn v1): **1 dropped alias-edge**
  (`@cosmograph/cosmos` → `@cosmos.gl/graph`).
- `test-rails` (yarn v1): **3 dropped alias-edges**
  (string-width-cjs, strip-ansi-cjs, wrap-ansi-cjs).

Total: **14 previously-dropped alias-edges now correctly resolved**.

**Fix**:

1. New `mikebom-cli/src/scan_fs/package_db/npm/alias_mapping.rs`
   submodule with:
   - `AliasResolution` type carrying local-name + aliased-name +
     aliased-version + optional peer-suffix + ecosystem tag.
   - `detect_pnpm_alias` for value-side detection in pnpm-lock v9
     snapshot sub-mappings. Guards against bare-version + peer-
     suffixed-version false positives via a name-shape heuristic.
   - `detect_yarn_v1_alias` for key-side detection in yarn v1 lockfile
     entry headers. Handles both `local@npm:aliased-name` shape
     (test-guac-visualizer) and `local@npm:aliased-name@range` shape
     (test-rails).
   - `rewrite_dep_names` for edge rewriting per FR-005.
2. Both `pnpm_lock.rs::parse_pnpm_lock` and
   `yarn_lock.rs::parse_v1` accumulate detected aliases during the
   parse pass, then post-process to:
   - Emit components under the ALIASED canonical PURL (not local-name).
   - Rewrite other entries' `depends` lists to reference the aliased
     canonical name.
   - Attach `mikebom:pnpm-alias` / `mikebom:yarn-alias` component-
     scope annotations per FR-006 (raw-string local-name value per
     Q1 clarification 2026-07-04).
   - Emit an FR-011 info-level tracing log
     `"npm-alias resolution completed"` with `lockfile_path`,
     `alias_count`, `alias_ecosystem` fields.

**Q1/Q2 clarifications (2026-07-04)**:

- Q1: annotation shape — raw string local-name only. Matches
  milestone-158 precedent; keeps per-component annotation size
  minimal (workspace repos may have 100+ alias-resolved components).
- Q2: yarn multi-spec dedup — once per unique local-name.

**Data-model note (FR-012 multi-alias case)**: When the same resolved
component is reached via two distinct local-names (rare monorepo
case), mikebom emits the annotation with a `Value::Array` of
sorted local-names. When only one local-name reaches the component,
emits a plain `Value::String`. Both shapes flow through the
milestone-127+ `extra_annotations` emission pattern at
`builder.rs:1185-1205` (verified at plan-analyze 2026-07-04) without
requiring new emission code.

**Empirical verification** (T028–T030):

| repo | ecosystem | aliases detected | aliased-canonical PURLs emitted |
|---|---|---|---|
| test-podman-desktop | pnpm v9 | 10 | `@slorber/react-helmet-async@1.3.0`, `@docusaurus/react-loadable@6.0.0`, `string-width@4.2.3`, `strip-ansi@6.0.1`, `wrap-ansi@7.0.0`, + 5 more |
| test-guac-visualizer | yarn v1 | 1 | `@cosmos.gl/graph@2.6.4` |
| test-rails | yarn v1 | 3 | `string-width@4.2.3`, `strip-ansi@6.0.1`, `wrap-ansi@7.0.0` |

**Wire-format cleanliness** (SC-003 dual-side guard): all 33
milestone-090 non-alias goldens byte-identical (0 diff lines across
every ecosystem × format). Alias emission ships only when alias
syntax is actually present.

**Consumer jq recipe**:

```bash
# List all alias-resolved npm components
jq '.components[] | select((.properties // [])[] | .name | test("^mikebom:(pnpm|yarn)-alias$")) | {purl, alias: (.properties[] | select(.name | test("-alias$")) | .value)}' sbom.cdx.json

# Count alias-affected components per ecosystem
jq '[.components[] | .properties // [] | .[] | .name | select(test("^mikebom:(pnpm|yarn)-alias$"))] | group_by(.) | map({(.[0]): length}) | add' sbom.cdx.json
```

**Parity catalog**: 2 new rows C106 (`mikebom:pnpm-alias`) + C107
(`mikebom:yarn-alias`) with `Directionality::SymmetricEqual`.
Milestone-071 parity check enforces symmetric emission across CDX +
SPDX 2.3 + SPDX 3.

Zero new Cargo dependencies. `mikebom-ebpf` untouched.

Constitution alignment:

- Principle VIII (Completeness): 14 previously-dropped edges now
  correctly emitted.
- Principle IX (Accuracy): aliased-canonical PURL matches how CVE
  feeds key on the npm-registry identity — false positives on
  never-matching local-name PURLs eliminated.
- Principle X (Transparency): the two new annotations preserve the
  local-name for audit trail.

### Workspace-root peer linkage + graph-completeness annotations (milestone 158)

Closes issue #492 (surfaced during the milestone-157 Round-2 audit of
`kusari-sandbox/test-*` repos). For workspace monorepos, mikebom's
per-package dep-graph was accurate (99.78% snapshot-match on
test-podman-desktop) but consumers BFS-traversing from the SBOM's
declared root component reached only **552 of 2835 components
(19.5%)**. Root cause: `select_root` identifies workspace peers as
"losers" via the milestone-127 ladder but never links them back to
the chosen root's `dependsOn`. Every consumer that renders a rooted
tree — the standard mental model for CDX `dependencies` and SPDX
relationships — missed 4/5 of the actual dep-graph.

**Two-part fix**:

1. **Workspace-peer linkage** — the CDX / SPDX 2.3 / SPDX 3 emitters
   now synthesize `Relationship { from: root, to: loser, kind:
   DependsOn }` edges for every workspace peer identified during
   root selection. Reuses milestone-127's `RootSelectionResult.losers`
   field — no new plumbing.

2. **`mikebom:graph-completeness` + `mikebom:graph-completeness-reason`
   document-scope annotations** — every emitted SBOM now carries a
   truthful graph-completeness signal computed via a multi-root BFS
   pass at emit-time. Values: `complete` (100% reachable from the
   multi-root seed set), `partial` (gap detected AND classified into
   one of 8 documented reason codes), `unknown` (Q1 caution-first
   fallback: prefer `unknown` over guessing).

**Q1/Q2/Q3 clarifications (2026-07-03)**:

- **Q1 caution-first**: when in doubt, emit `unknown` rather than
  claiming `complete` or `partial`. Prevents silent-lie failure modes.
- **Q2 orphaned components**: nested test-tree package.json devDeps
  and similar orphans are emitted faithfully (no filtering, no
  synthetic auto-linking) and flagged via
  `orphaned-components-detected: <N>` reason code.
- **Q3 multi-ecosystem**: repos with multiple ecosystems (e.g.,
  test-rails: gem + npm) run BFS from each per-ecosystem main-module
  root. Reachability = union. Per-ecosystem root identification
  failures fire the `multi-ecosystem-partial-root: <ecosystems>`
  reason code.

**T035 empirical measurement 2026-07-03** on `test-podman-desktop`:

- Pre-158: 552/2835 npm components reachable (19.5%).
- Post-158: **698/2835 npm components reachable (24.6%)** —
  a +146-component / +5.1 percentage-point improvement from linking
  the 25 detected workspace peers.
- The pre-implementation ≥99% target was miscalibrated: the fixture
  contains declared-only workspace-peer deps (e.g.,
  `pkg:npm/%40docusaurus/core@` with EMPTY VERSION strings) that don't
  resolve to any emitted component's canonical PURL. BFS walks these
  phantom edges but reaches nothing further. This is a pre-existing
  edge-resolution issue in mikebom's npm workspace-peer parsers,
  orthogonal to milestone 158's scope; will be tracked as a follow-on.
- The milestone-158 annotation correctly signals `partial` with reason
  `orphaned-components-detected: 2173` per Constitution Principle X.

**Reason-code vocabulary** (spec.md SC-005, closed 8-code set;
adding a new code is a spec/CHANGELOG event, not a silent code
change):

- `workspace-peer-detection-degraded`
- `root-selection-ambiguous`
- `root-selection-failed`
- `edge-resolution-degraded`
- `go-transitive-coverage-degraded` (deferred to #495)
- `go-workspace-mode-anomaly` (deferred to #494)
- `orphaned-components-detected`
- `multi-ecosystem-partial-root`

Multiple codes joined by `; ` (semicolon + space) per FR-012.

**Verification**:

- 25 new unit tests in `mikebom-cli/src/generate/graph_completeness/`
  (SC-007 floor ≥10; 25 total after impl).
- Parity catalog rows C104 + C105 with `Directionality::SymmetricEqual`
  ensure CDX / SPDX 2.3 / SPDX 3 emit the annotation with identical
  values (SC-010).
- SC-002 dual-side byte-identity guard: 27 of 33 milestone-090 goldens
  changed by exactly the added `mikebom:graph-completeness = complete`
  annotation (4 CDX / 6 SPDX 2.3 / 8 SPDX 3 diff lines). 6 goldens
  (golang + npm × 3 formats each) emit `partial` due to real orphans
  in those fixtures — this is truthful per Constitution Principle X,
  not a regression.
- 33 goldens regenerated via `./scripts/regen-goldens.sh`.

**Consumer jq recipe** (per research §R9):

```bash
# Gate a CI pipeline on graph completeness
completeness=$(jq -r '.metadata.properties[] | select(.name == "mikebom:graph-completeness") | .value' sbom.cdx.json)
case "$completeness" in
    complete)
        echo "Graph is fully connected — safe to consume"
        ;;
    partial)
        reason=$(jq -r '.metadata.properties[] | select(.name == "mikebom:graph-completeness-reason") | .value' sbom.cdx.json)
        echo "Partial graph: $reason"
        ;;
    unknown)
        reason=$(jq -r '.metadata.properties[] | select(.name == "mikebom:graph-completeness-reason") | .value' sbom.cdx.json)
        echo "Unknown completeness: $reason (recommend re-scan or manual review)"
        exit 1
        ;;
esac
```

**Constitution alignment**:

- Principle VIII (Completeness): the milestone's whole point.
- Principle X (Transparency): the annotations ARE transparency
  metadata in the constitution's exact sense.
- Principle V (Standards-native precedence): FR-010 acknowledges the
  `mikebom:*` deviation is required because no CDX 1.6 / SPDX 2.3 /
  SPDX 3.0.1 native "graph completeness" property exists at emission
  time; a future standard enum would supersede.

Zero new Cargo dependencies. `mikebom-ebpf` untouched.

### pnpm-lock v9 dep-graph fix (milestone 157)

The team reported "pnpm isn't working" against `kusari-sandbox/argo-cd`.
Empirical reproduction 2026-07-03 confirmed the bug: mikebom's pnpm
parser at `mikebom-cli/src/scan_fs/package_db/npm/pnpm_lock.rs:83-91`
reads `dependencies` from `packages:` entries only. pnpm v9 moved
dep-graph edges out of `packages:` into a new top-level `snapshots:`
section — mikebom's parser never learned to read it. Result on
argo-cd: 1329 npm components emitted correctly + only 110 dep-graph
edges (the 110 came from the top-level `package.json` root fallback;
every non-root component had empty `dependsOn`).

**Fix**: `parse_pnpm_lock` now pre-scans `snapshots:` into a lookup
keyed by canonical `name@version` (peer-dep suffix stripped via the
existing `parse_pnpm_key` helper), then branches on `lockfileVersion`:
on v9, edges come EXCLUSIVELY from `snapshots:` and `packages:` sub-
mappings are ignored; on v6/v7/v8, edges come inline from `packages:`
(matches milestone-147 npm parity). The v9 branch shape is required
because pnpm v9's `packages:` `peerDependencies:` values are SEMVER
SPECIFIERS (`^7.0.0`), not resolved versions — treating them as
resolved edges emits wrong (but plausible-looking) dep-graph output.
This was the root cause of a Round-1 mismatch discovered during
post-T014 accuracy auditing against the argo-cd/ui testbed.

**Q1 clarification 2026-07-03** (Constitution Principle VIII —
Completeness): milestone 157 also brings pnpm to full parity with
npm's `package_lock.rs` (milestone 147). Both `snapshots:` (v9) AND
`packages:` (v6/v7) entries walk the union of three sub-mappings:
`dependencies:` + `peerDependencies:` + `optionalDependencies:`. The
pnpm parser has been the outlier reading only `dependencies:` since
milestone 147; this brings it in line with the npm sub-reader
convention. Consequence: pnpm v6/v7 goldens WOULD regenerate with new
edges monotonically-additively — but the milestone-090 fixture uses a
v6 lockfile with only `dependencies:` sub-mappings, so no regeneration
occurred in practice.

**Verification**:
- 9 new unit tests in `pnpm_lock.rs` (SC-007 floor ≥8; 12 total pnpm
  tests after impl).
- 4 integration tests in `mikebom-cli/tests/npm_pnpm_v9_dep_graph.rs`:
  SC-008 synthetic argo-cd-shape fixture; F2-remediated monotonic-
  additive helper + self-test proving the helper catches missing-edge
  violations; T010 Step-3 real-golden verification via
  `MIKEBOM_PRE157_SNAPSHOT_DIR`.
- SC-001 argo-cd testbed accuracy audit (Round-2, post-lockfileVersion
  branch fix): **1329/1329 snapshots (100.00%) have EXACT-MATCH `dependsOn`
  to the pnpm-lock.yaml `snapshots:` section — zero false positives, zero
  false negatives, zero orphans.** 3016 total edges (vs pre-157: 110
  edges + zero non-root with dependsOn). Git-URL tarball deps included
  (e.g. `argo-ui@https://codeload.github.com/argoproj/argo-ui/tar.gz/…`
  emits its 19 exact-match deps).

**Diagnostic emissions** (Constitution Principle X — Transparency):
- FR-007 info-level: `pnpm-lock parsed` with `packages_count` +
  `snapshots_count` + `fell_back_to_snapshots` counts per scan.
  Grep-friendly for CI-log analysis of lockfile-format issues.
- FR-008 warn-level: `pnpm-lock v9 with no snapshots section` fires
  on anomalous v9 lockfiles where the parser would silently emit
  flat components.

**Consumer jq recipe**:
```bash
jq '.dependencies[] | select(.ref == "pkg:npm/%40actions/core@3.0.1") | .dependsOn' sbom.cdx.json
# Expected: ["pkg:npm/%40actions/exec@3.0.0", "pkg:npm/%40actions/http-client@4.0.1"]
```

Wire-format-agnostic: no changes to CDX 1.6 / SPDX 2.3 / SPDX 3.0.1
emitter code. Zero new `mikebom:*` annotation keys. Zero new catalog
rows. Zero new Cargo dependencies. Non-pnpm goldens (10 of 11 milestone-
090 ecosystems) byte-identical.

### CMake walker depth extension (milestone 156)

Direct closure of milestone-155's F1 remediation debt. Milestone 155
shipped as walker-scope-honest — Kamailio's identified-component floor
was ≥1 because `discover_cmake_files` only walked depth-1 under
`cmake/`, `Modules/`, `third_party/`, missing the 9+ `find_package`
calls in Kamailio's `cmake/modules/Find*.cmake` files at depth-2.

Milestone 156 extends the walker to recursive descent under `cmake/`
and `Modules/`. `<scan_root>/third_party/` stays at depth-1 by
default (matching milestone-102 behavior) — a new opt-in flag
`--cmake-third-party-recursive` (env alias
`MIKEBOM_CMAKE_THIRD_PARTY_RECURSIVE=1`) extends recursion to
`third_party/` too when set.

**Implementation**: reuses milestone-054's `safe_walk` helper
(`mikebom-cli/src/scan_fs/walk.rs`) for the recursive descent —
inherits symlink-cycle safety (canonicalize-keyed visited-set),
rootfs sandbox enforcement, and `tracing::debug!` skip logging.
Milestone-113 `--exclude-path` integration handled inside the cmake
walker's visit callback (safe_walk's own exclude_set relativizes
against its rootfs argument, which for the cmake reader is a subdir
of the scan root, not the scan root itself — so operator-supplied
scan-root-relative exclude patterns are matched here instead).

**Kamailio testbed impact**: 1 → 4 identified components at
Kamailio HEAD (empirical result 2026-07-02). Kamailio's
`cmake/modules/Find*.cmake` files are CMake module DEFINITIONS (using
`find_path` / `find_library` / `find_package_handle_standard_args`),
not `find_package` call sites — the milestone-155 F1 remediation
misread the tree layout. Only 2 depth-2 Find*.cmake files contain
actual `find_package` calls (`PkgConfig`, `LibfreeradiusClient`);
plus the depth-1 `OpenSSL` from `cmake/defs.cmake`; plus `radcli`
from a depth-2 `pkg_check_modules` call. The walker-depth capability
itself is correct and verified independently via synthetic SC-004
depth-3 fixtures — other projects (e.g., CPM.cmake users, projects
using `include()` chains to depth-N) benefit fully. Kamailio's
specific tree layout produces a modest 4x improvement rather than
the 10x anticipated.

**Q1 clarification** (2026-07-02): `third_party/` recursive-walking
policy resolved as "depth-1 default; opt-in via
`--cmake-third-party-recursive`". Auditors of narrow-scope projects
(Kamailio, without a large `third_party/` tree) get their win
noise-free. Auditors of projects vendoring large trees (LLVM,
Chromium, WebRTC) opt in explicitly to reach the vendored
transitive-dep declarations.

**Build-tree contamination note**: CMake can emit `.cmake` files
under `build/`, `cmake-build-*/`, `out/` during a real build. If
those directories are in the scan target, they get walked by the
extended recursive descent (per FR-018 mikebom does NOT auto-exclude
them — some projects legitimately have a `build/` directory in
source). Operators encountering noise from build-tree emissions
should add `--exclude-path build,cmake-build-*,out` to their scan
invocation.

**Consumer jq recipe** — filter cmake-find-package components by
depth of source-file:
```bash
jq '.components[] | select(.properties[]?
  | select(.name == "mikebom:source-mechanism" and .value == "cmake-find-package"))
  | select(.properties[]?
    | select(.name == "mikebom:source-files" and (.value | contains("cmake/modules/"))))
  | .purl' sbom.cdx.json
```

Wire-format-agnostic: no changes to CycloneDX 1.6 / SPDX 2.3 / SPDX
3.0.1 emitters. Zero new `mikebom:*` annotation keys. Zero new
`docs/reference/sbom-format-mapping.md` catalog rows (milestone
155's C55 + C103 rows cover everything this milestone emits). Zero
new Cargo dependencies. Byte-identity guaranteed for depth-1-only
scan targets (SC-002 verified via all 3 golden regression suites).

### CMake `find_package` + `pkg_check_modules` extraction (milestone 155)

Reverses milestone-102's FR-007 refusal of `find_package(<Name>
[<Version>])` extraction — the original double-counting concern is
resolved by the production `resolve::deduplicator` pipeline's same-PURL
merging. Source-tree scans of CMake-based C/C++ projects that declare
external deps via `find_package(...)` calls at depth-1
(`CMakeLists.txt` + `cmake/*.cmake` files) now surface those deps in
the emitted SBOM.

**Emissions added**:

- `find_package(<Name> [<Version>])` → `pkg:generic/<lowercased-name>[@<highest-declared-version>]`
  with `mikebom:source-mechanism = "cmake-find-package"`. Multi-file
  same-name declarations are consolidated to the highest declared
  version (Q1 clarification: SemVer-style component-wise numeric with
  zero-padding for shorter versions; lex fallback + `tracing::warn`
  when segments aren't all-numeric).
- `pkg_check_modules(<TARGET> [REQUIRED] [IMPORTED_TARGET] <modules>)`
  + `pkg_search_module(...)` → one `pkg:generic/<module>` per module
  (TARGET var discarded, version constraints stripped, modifier
  keywords filtered) with `mikebom:source-mechanism = "cmake-pkg-check-modules"`.
- **Case preservation**: `find_package(OpenSSL 1.1.0)` emits
  `pkg:generic/openssl@1.1.0` (PURL lowercased per spec convention) and
  a new `mikebom:cmake-find-package-name = "OpenSSL"` annotation
  preserves the original casing. The annotation is emitted ONLY when
  original casing ≠ lowercased.

**Not extracted** (regex boundary or intentional exclusion):

- `find_package_handle_standard_args(...)` — CMake-internal, not a
  package declaration (FR-009).
- `find_package(${VAR})` — CMake variable interpolation not resolved;
  logged at `debug` level (FR-010).
- Same-line commented `# find_package(...)` (FR-011).

**Consumer jq recipe** — list all find_package-derived components:

```bash
jq '.components[] | select(.properties[]?
  | select(.name == "mikebom:source-mechanism"
           and .value == "cmake-find-package"))
  | .purl' sbom.cdx.json
```

**Kamailio testbed impact**: 0 → 1 identified component at Kamailio HEAD's
depth-1 walker scope (`OpenSSL 1.1.0` from `cmake/defs.cmake`).
Kamailio's remaining 9+ `find_package` calls live at depth-2 inside
`cmake/modules/Find*.cmake` and are NOT reached by mikebom's current
`discover_cmake_files` helper — walker-depth extension is a separate
future milestone opportunity. Projects with all-depth-1 layouts
(typical vcpkg / conan / Ninja-first projects) yield higher counts
immediately; SC-004's synthetic Kamailio-shape testbed at
`mikebom-cli/tests/fixtures/cmake-find-package/kamailio-shape/`
exercises a 5-component shape end-to-end.

**Same-PURL cross-mechanism dedup** — a project declaring `openssl` via
BOTH `find_package(openssl 1.1.0)` AND
`FetchContent_Declare(openssl URL ...openssl-1.1.0.tar.gz)` produces
exactly ONE `pkg:generic/openssl@1.1.0` component in the emitted SBOM;
the production `resolve::deduplicator` pipeline merges them via its
`(ecosystem, name, version, parent_purl)` grouping key and folds
non-conflicting `extra_annotations` per milestone 109's pattern.
Cross-namespace dedup (e.g., cmake vs dpkg/rpm/apk) is NOT provided by
this milestone — operators wanting that should use the milestone-111
`--pkg-alias-binding` CLI flag, or await a milestone-105
`scan_fs::dedup` completion follow-up that wires the
`mikebom:also-detected-via` list into production emission.

**Q1 clarification** (highest declared version wins across multi-file
same-name declarations) codified in FR-002 + US1 A3.
**Q2 clarification** (uniform emission — no build-tool denylist;
`find_package(Threads)`, `find_package(PkgConfig)`, `find_package(Doxygen)`,
etc. all emit uniformly; consumers filter by name post-emission) codified
in FR-017.

Wire-format-agnostic: no changes to CycloneDX 1.6 / SPDX 2.3 / SPDX
3.0.1 emitters. No changes to any other reader or the milestone-133
file-tier walker. Zero new Cargo dependencies. One new
`mikebom:*`-prefixed annotation key (`mikebom:cmake-find-package-name`)
per Constitution Principle V audit at plan.md; catalog documentation
in `docs/reference/sbom-format-mapping.md` deferred to a follow-up
docs-refresh milestone (matches milestone 105's prior additive-annotation
precedent).

### SPDX 3: emit `simplelicensing_CustomLicense` for every `LicenseRef-*` (closes #487)

Paired follow-up to #485 (closed in `2d7ab0e` via PR #486), which added
the SPDX 2.3 `hasExtractedLicensingInfos[]` sweep. Milestone 153's
`spdx3-validate` investigation showed SPDX 3.0.1 was validator-permissive
for undefined `LicenseRef-*` tokens (Outcome B), so the SPDX 3 emitter
shipped unchanged. Issue #487 filed a paired follow-up asking for
symmetry regardless — a compliance auditor reading both formats of the
same scan should get consistent LicenseRef-resolution.

mikebom now sweeps every emitted `simplelicensing_LicenseExpression`
element's expression string for inline `LicenseRef-<idstring>` tokens
at SPDX 3 document-assembly time and emits a matching
`simplelicensing_CustomLicense` graph element per distinct token.
Placeholder text is byte-identical to milestone 153's SPDX 2.3
`hasExtractedLicensingInfos[].extractedText` field — the milestone-153
`PLACEHOLDER_EXTRACTED_TEXT` const is promoted from module-private to
`pub(crate)` in `document.rs` so the SPDX 3 emitter imports it as the
single source of truth for the wire contract. Any future change to the
const value trips both milestones' contract-lock tests
(`placeholder_text_matches_wire_contract` in `document.rs`,
`cross_format_placeholder_identity` in `v3_licenses.rs`).

**Element shape**:

```json
{
  "type": "simplelicensing_CustomLicense",
  "spdxId": "{doc_iri}/licenseref/{idstring}",
  "creationInfo": "{creation_info_id}",
  "name": "{idstring}",
  "simplelicensing_licenseText": "License text not extracted by mikebom. Consult the original package (e.g., /usr/share/licenses/<name>/ on Debian/RPM, or upstream project source) for the full text."
}
```

**IRI scheme** (per milestone-154 Clarifications Q1): the readable path
segment `{doc_iri}/licenseref/{idstring}` — human-diffable against the
LicenseRef- reference inside the license expression. Idstring alphabet
`[a-zA-Z0-9-.]+` is a subset of RFC 3986 unreserved characters, so no
percent-encoding needed.

**Cross-format symmetry**: for any given scan target, the set of
`LicenseRef-<idstring>` tokens defined by SPDX 2.3's
`hasExtractedLicensingInfos[].licenseId` equals the set derivable from
SPDX 3's `simplelicensing_CustomLicense.name` fields (with `LicenseRef-`
prefix reapplied). Consumers reading both formats of the same scan get
consistent LicenseRef-resolution. Verify via jq recipe:

```bash
diff \
  <(jq -c '[.hasExtractedLicensingInfos[].licenseId] | sort' out.spdx.json) \
  <(jq -c '[.["@graph"][] | select(.type == "simplelicensing_CustomLicense") | ("LicenseRef-" + .name)] | sort' out.spdx3.json)
```

Empty diff = symmetry holds.

**Happy-path byte-identity preserved**: scans that emit no LicenseRef-*
in any expression string (Cargo / npm / Go / pip source trees, in the
default case) produce byte-identical SPDX 3 output vs pre-154 — the
sweep returns an empty Vec, and no new `simplelicensing_CustomLicense`
elements are pushed onto `@graph`.

**Constitution Principle V**: `simplelicensing_CustomLicense` is the
SPDX 3.0.1-native carrier for defining `LicenseRef-*` identifiers. No
new `mikebom:*` annotation introduced. **Principle IX + X**: the
placeholder text explicitly discloses that mikebom did not extract the
real text (same guarantee as milestone 153); byte-locked cross-format
identity is a machine-parseable signal consumers pattern-match on.

### SPDX 2.3: emit `hasExtractedLicensingInfos` for every `LicenseRef-*` (closes #485)

Follow-up to #481 (closed in `feba7cb` via PR #484). Milestone 152's
`LicenseRef-<sanitized>` escape hatch correctly preserves recognized
operands in compound license expressions when one operand is
unrecognized — but a strict SPDX 2.3 consumer will reject the resulting
document as non-conformant because SPDX 2.3 §10.1 requires every
distinct `LicenseRef-<idstring>` referenced in any package's
`licenseDeclared` / `licenseConcluded` field to have a matching entry
in the top-level `hasExtractedLicensingInfos[]` array.

mikebom now sweeps every emitted SPDX 2.3 document for inline
`LicenseRef-*` substrings at document-assembly time and emits a matching
entry for each distinct id. Entries produced by the pre-existing
milestone-012 hash-fallback path (which carries the raw
non-canonicalizable expression as its `extractedText`) are preserved
unchanged; the new sweep only fills in entries the pre-existing path
didn't cover.

**Placeholder `extractedText`**: this milestone does NOT extract real
license text from RPM contents or upstream sources. Every sweep-emitted
entry carries the byte-exact placeholder text:

```
License text not extracted by mikebom. Consult the original package
(e.g., /usr/share/licenses/<name>/ on Debian/RPM, or upstream project
source) for the full text.
```

The `<name>` token is a LITERAL — mikebom does not substitute the
package name. Consumers may pattern-match on this exact prefix to
distinguish mikebom-placeholder entries from entries with real
extracted text (from the milestone-012 hash-fallback path):

```jq
.hasExtractedLicensingInfos[]
| select(.extractedText | startswith("License text not extracted by mikebom."))
```

Best-effort text extraction from RPM contents (e.g.,
`/usr/share/licenses/<pkg>/COPYING`) is deferred to a follow-up
milestone if operator demand surfaces.

**DocumentRef-prefixed LicenseRefs**: `DocumentRef-<doc>:LicenseRef-<id>`
compound tokens are correctly EXCLUDED from the sweep — the LicenseRef
is defined in the referenced OTHER document, not this one. mikebom
doesn't emit DocumentRef- forms today; this is defensive-code future-
proofing for operator-supplied data via supplement-CDX or similar.

**Happy-path byte-identity preserved**: scans that emit no LicenseRef-*
(the common case for Cargo, npm, Go, pip source trees) produce
byte-identical SPDX 2.3 output vs pre-153 — the sweep returns an empty
Vec, and the document's serde `skip_serializing_if = "Vec::is_empty"`
attribute omits the `hasExtractedLicensingInfos` key entirely.

**SPDX 3 investigation outcome (FR-008 / FR-009)**: `spdx3-validate`
against a synthetic SPDX 3.0.1 document containing an inline
`LicenseRef-*` in a `simplelicensing_LicenseExpression` WITHOUT any
matching `licensing_CustomLicense` element passes both schema and
SHACL checks (exit 0). SPDX 3.0.1's model does NOT require equivalent
emission — the SPDX 3 emitter is already conformant as-is. No code
change to `v3_licenses.rs` was needed.

**CycloneDX unaffected**: CDX 1.6 has no §10.1-equivalent constraint;
its `license.expression` accepts arbitrary tokens without a separate
definition table. Only SPDX 2.3 needed the fix.

### RPM license expressions: preserve known operands when one is unknown (closes #481)

Follow-up to #475 (closed in `eb75853` via PR #478). The milestone-478
BitBake `&`/`|` operator normalization recovered 5 of the 10
`NOASSERTION` cases the maintainer observed on the `core-image-minimal`
qemux86-64 scarthgap-LTS testbed. The remaining 5 (`busybox`,
`busybox-hwclock`, `busybox-syslog`, `busybox-udhcpc`, `liblzma5`) still
collapsed to `NOASSERTION` because their compound `License:` headers
contain at least one operand that isn't a registered SPDX identifier
(e.g., `bzip2-1.0.4` for busybox-family, `PD` for liblzma5).

mikebom now wraps each unrecognized operand as a SPDX 2.3-spec-blessed
`LicenseRef-<sanitized>` escape-hatch identifier instead of collapsing
the whole expression. The recognized portion survives; the unknown
portion is preserved as structured-but-uncanonicalized signal that
downstream tooling can ignore, resolve via deps.dev / Clearly Defined,
or escalate to source review.

**Per-package fix:**

| Package | Pre-152 `licenseDeclared` | Post-152 `licenseDeclared` |
|---|---|---|
| `busybox*` | `NOASSERTION` | `GPL-2.0-only AND LicenseRef-bzip2-1.0.4` |
| `liblzma5` | `NOASSERTION` | `LicenseRef-PD` |

**Sanitization rule** (per Clarifications Q1 — replace + collapse + strip):
replace each character outside `[a-zA-Z0-9-.]` with `-`, collapse
consecutive `-` to a single `-`, then strip leading and trailing `-`.
The algorithm is idempotent. Worked examples:

| Raw token | Sanitized form |
|---|---|
| `bzip2-1.0.4` | `LicenseRef-bzip2-1.0.4` (unchanged — already valid) |
| `PD` | `LicenseRef-PD` |
| `GPLv2+` | `LicenseRef-GPLv2` |
| `My License v2` | `LicenseRef-My-License-v2` |
| `(custom)` | `LicenseRef-custom` |

**Pipeline ordering**: the new fallback composes after the milestone-478
operator normalizer. The full pipeline: raw RPM `License:` header →
`normalize_bitbake_license_operators` (M478) → `try_canonical`
first-pass → (on failure) → operand-by-operand `LicenseRef-<sanitized>`
wrapping (M152) → `try_canonical` second-pass → (on failure) →
`NOASSERTION` (existing fail-closed behavior). Happy-path expressions
(every operand a recognized SPDX id) take the first-pass path and are
byte-identical to pre-152 output.

**WITH-clause behavior** (per Clarifications Q2): when the LEFT side of
a `WITH` clause is unrecognized, mikebom wraps it as
`LicenseRef-<sanitized>` and preserves the exception. When the
EXCEPTION (right side) is unrecognized, mikebom conservatively collapses
the entire surrounding compound expression to `NOASSERTION` — SPDX 2.3
does not define an `ExceptionRef-` carrier, and silently dropping an
unknown exception would misrepresent the license's legal meaning (e.g.,
GCC runtime library exception relaxes GPL terms).

**Scope**: RPM reader only this milestone. Other readers (deb, apk,
npm, etc.) that exhibit the same NOASSERTION-on-unknown-operand
collapse are out of scope; if they surface in operator feedback,
they'll be addressed in a follow-up milestone.

**Constitution Principle V**: the SPDX 2.3 `LicenseRef-<idstring>`
escape hatch is the standards-native carrier for unknown license
identifiers — no new `mikebom:*` annotation key introduced.

### Homebrew (brew + Linuxbrew) package detection (closes #432)

mikebom gains a sixth OS package-DB reader alongside dpkg, apk, rpm,
opkg, and alpm. Scanning macOS developer machines (Apple Silicon or
Intel), Steam Deck dev environments with Linuxbrew, CI runners with
Linuxbrew installs, or any rootfs containing a Homebrew install now
produces one component per `brew install`-ed formula and per
(JSON-backed) cask — instead of leaving them invisible to the scan.

**PURL identity** — uses the de-facto industry convention shared with
syft + cyclonedx-bom-gen:

```text
pkg:brew/<name>@<version>[?tap=<owner>/<tap>][&type=cask]
```

Examples:

- Apple Silicon `brew install curl`: `pkg:brew/curl@8.5.0`
- noarch n/a (Homebrew is single-arch-per-install)
- Third-party tap formula: `pkg:brew/terraform@1.10.0?tap=hashicorp/tap`
- macOS Cask: `pkg:brew/visual-studio-code@1.95.3?type=cask`
- Cask from non-default tap: `pkg:brew/intellij-idea@2024.3?tap=homebrew/cask-versions&type=cask`

**Three install-prefix detection** — `/opt/homebrew` (Apple Silicon
macOS), `/usr/local` (Intel macOS), `/home/linuxbrew/.linuxbrew`
(Linux). Discrimination signal is the presence of `Cellar/`
subdirectory; `/usr/local/` alone is NOT a Homebrew signal (it's a
generic Linux sysadmin path). The install location does NOT leak into
the PURL identity — a `curl@8.5.0` formula has the same PURL whether
installed under any of the three prefixes.

**Dep-graph edges** — extracted from each `INSTALL_RECEIPT.json`'s
`runtime_dependencies` array. Tap-qualified dep names like
`hashicorp/tap/terraform` are normalized to the bare formula name
(`terraform`) so cross-component lookups in the deduplicator succeed
for third-party-tap dependencies.

**Cross-reader coexistence** — Linuxbrew runs on top of a real Linux
distro; the brew reader runs alongside dpkg/apk/rpm cooperatively.
Both sets of components emit — Homebrew supplements the underlying
distro packages, never replaces them.

**Cask support** — modern (Homebrew 4.0+) `Casks/<token>.json`
metadata parses cleanly. Pre-4.0 `.rb`-only casks warn-and-skip per
Constitution Principle I (no Ruby parser); operator-visible
diagnostic names the cask and explains the skip.

**Per-formula error posture** — malformed `INSTALL_RECEIPT.json` files
warn-and-skip without aborting the scan; partial output is preserved
(FR-007). Missing Homebrew install is a clean no-op with zero
warnings (FR-006).

**Why mikebom-specific** (Constitution Principle V audit): the
`pkg:brew/` PURL type is not yet registered in the purl-spec
PURL-TYPES.rst — mikebom emits per the de-facto convention shared
with syft and cyclonedx-bom-gen. The PURL itself IS the
standards-native identity carrier; only the type-name is unblessed.
A follow-up issue should propose upstream addition. Zero new
`mikebom:*` annotations introduced; zero new parity-catalog C-rows;
zero new Cargo dependencies. Full audit in
`specs/136-homebrew-reader/research.md` R1.

**Out of scope** (deferred follow-ups documented in spec):

- **File-claim tracker integration**: Homebrew's symlink-heavy
  bottling (`<prefix>/bin/curl → <prefix>/Cellar/<formula>/<ver>/bin/curl`)
  is fundamentally different from alpm/dpkg's flat ownership model.
  Tracking it properly requires symlink resolution — separate spec.
  Known soft regression: the binary walker may emit
  `pkg:generic/<binary>` duplicates alongside `pkg:brew/<formula>`
  components on Linuxbrew rootfs scans.
- **License emission**: license is NOT in `INSTALL_RECEIPT.json` —
  extracting it requires Ruby DSL parsing (Principle I conflict) or
  network calls to `formulae.brew.sh` (FR-010 conflict). Tracked as
  a cross-reader follow-up alongside milestone-135's FR-012 URL
  deferral.
- **`HOMEBREW_PREFIX` env-var custom install locations**: only the
  three documented standard prefixes are scanned.
- **Pre-receipt-format Homebrew installs**: receipt has been
  universal since at least 2011; older installs (rare in 2026) are
  out of scope.
- **Pacman-style `co_owned_by` cross-reader dual-ownership
  detection**: deferred.

### Arch Linux pacman/alpm package database reader (closes #429)

mikebom gains a fifth OS package-DB reader alongside the existing
dpkg, apk, rpm, and opkg readers. Scanning Arch Linux containers,
Steam Deck images (SteamOS), Manjaro / EndeavourOS / CachyOS rootfs,
or any pacman-managed Linux installation now produces one component
per installed package instead of leaving them invisible to the scan.

**PURL identity** — uses the purl-spec's native `alpm` type:

```text
pkg:alpm/<distro>/<name>@<version>?arch=<arch>[&distro=<distro>-<verid>]
```

Examples:

- Stock Arch container: `pkg:alpm/arch/bash@5.2.026-1?arch=x86_64`
- SteamOS Deck: `pkg:alpm/steamos/bash@5.2.026-1?arch=x86_64&distro=steamos-3.5.7`
- noarch package: `pkg:alpm/arch/terminfo@6.4-3?arch=any`
- Unknown derivative `ID=mydistro`: `pkg:alpm/mydistro/...` (verbatim pass-through; no allowlist gate, so future Arch derivatives work without code changes)

**Rolling-release Arch** correctly omits the `distro=` qualifier
(matches the existing dpkg/apk/rpm convention when `VERSION_ID` is
absent in `/etc/os-release`).

**Binary walker dedup** — pacman's `files` manifests register into
the cross-reader file-claim tracker, so the binary walker skips
emission of `pkg:generic/<binary>` components for paths owned by an
Arch package. No more duplicate `pkg:alpm/arch/bash` +
`pkg:generic/bash` entries.

**Per-package error posture** — malformed `desc` files warn-and-skip
without aborting the scan; partial output is preserved (FR-009).
Missing pacman DB is a clean no-op with zero warnings (FR-008).

**Why mikebom-specific** (Constitution Principle V audit): the
purl-spec `alpm` type IS the standards-native identity carrier.
CDX 1.6, SPDX 2.3, and SPDX 3.0.1 all consume PURLs as a first-class
component-identity field. Zero new `mikebom:*` annotations
introduced; zero new parity-catalog C-rows; zero new Cargo
dependencies. Full Principle V audit in
`specs/135-arch-alpm-reader/research.md` R1.

**Out of scope** (deferred follow-ups): URL/homepage emission as a
wire-level external reference (cross-cutting OS-reader work — no
existing OS reader does this today); pacman `mtree` per-file hash
extraction; AUR vs official-repo provenance discrimination;
`pacman.conf` repo configuration parsing.

### Divergent-PURL collision detection in main-module dedup (closes #125)

Milestone 064's cargo main-module emission already dedupes same-PURL
collisions (vendored copies, `examples/` mirrors, `target/package`
extractions) and logs a `tracing::warn!` when it drops duplicates. The
realistic milestone-064 cases have identical declared dep sets so
first-wins is harmless. This milestone upgrades the silent
augment-in-place case where two `Cargo.toml` files claim the same
`pkg:cargo/<name>@<version>` PURL but the content actually diverges —
either via different declared `[dependencies]` / `[dev-dependencies]`
/ `[build-dependencies]` table keys, or (under `--deep-hash`) via
different per-crate file-tree SHA-256s.

**Per-component property** `mikebom:duplicate-purl-divergent` (C99)
lands on the deduped root component for every detected divergent
collision. Structured value:

```json
{
  "v": 1,
  "purl": "pkg:cargo/foo@1.2.3",
  "reason": "deps-differ",
  "paths": ["crates/foo/Cargo.toml", "vendor/foo/Cargo.toml"],
  "dep_sets_by_path": {
    "crates/foo/Cargo.toml": ["serde", "tokio"],
    "vendor/foo/Cargo.toml": ["anyhow", "serde", "tokio"]
  }
}
```

`reason` is one of `deps-differ` / `hashes-differ` / `both`.
`hashes_by_path` populates instead of (or alongside) `dep_sets_by_path`
when the deep-hash compare fires under `--deep-hash`.

**Document-scope summary** `mikebom:purl-collisions-detected` (C100)
aggregates every detected collision into one `metadata.properties[]` /
top-level `annotations[]` / `SpdxDocument.annotations[]` entry so
consumers can enumerate all collisions via a single `jq` query.
Sorted lex by PURL for stable output.

**Scope**: cargo only this milestone; the `DivergenceRecord` /
`CollisionsSummary` types in `mikebom-common::divergence` are
ecosystem-agnostic so future npm / maven / pip / gem / go-binary
follow-ups can populate the same shape at their own dedup sites.

**Defaults**: soft-only — the annotation surfaces in the SBOM but the
scan never fails on divergence. A hard-fail flag is deferred to a
future milestone. The existing milestone-064 `tracing::warn!`
continues to fire alongside the new annotation (FR-008 — preserves
the log-watching contract).

**Byte-identity**: no annotation appears in clean SBOMs. The existing
11-ecosystem regression goldens
(`mikebom-cli/tests/fixtures/golden/{cyclonedx,spdx-2.3,spdx-3}/*.json`)
stay byte-identical pre/post this milestone (SC-002 invariant).

**Why mikebom-specific (Constitution Principle V audit)**: no CDX 1.6
/ SPDX 2.3 / SPDX 3.0.1 native field expresses "same identity,
divergent content" semantics. CDX `evidence.occurrences[]` is
presence-only ("found here too"); SPDX `VARIANT_OF` / `COPY_OF` is
between DIFFERENT PURLs; `sourceInfo` is unstructured prose. Full
audit narrative in `docs/reference/sbom-format-mapping.md` C99/C100 +
`specs/134-divergent-purl-detection/research.md` R1.

### `--root-purl <PURL>` single-flag form for the root component PURL (closes #359)

New CLI flag that the operator passes to express the BOM subject's
PURL as a single, verbatim purl-spec string. Useful when downstream
consumers expect a specific canonical PURL (matching their other
data sources) and the operator wants to express it directly, including
PURL features the milestone-077 discrete flags can't reach:

- **Qualifiers** like `?arch=amd64`, `?distro=alpine-3.19`.
- **Subpaths** like `#cmd/worker`.
- **Custom namespace splits** like
  `pkg:golang/github.com/example/svc` (slashed namespace).

**Operator surface**:

```
mikebom sbom scan --path ... \
  --root-purl 'pkg:golang/github.com/example/svc@v1.2.3?arch=amd64'
```

→ CDX `metadata.component.purl` / SPDX 2.3 root `externalRefs[purl]` /
SPDX 3 root `software_packageUrl` + `externalIdentifier[packageUrl]`
ALL emit the operator-supplied string verbatim. Name + version are
parsed from the PURL (`packageurl`-crate spec parser) and surface on
`metadata.component.name` / `versionInfo` / `software_packageVersion`
accordingly.

**Precedence + mutex**: `--root-purl` is clap-`conflicts_with`
mutually exclusive with every discrete `--root-*` flag
(`--root-name`, `--root-version`, `--root-purl-type`, `--no-root-purl`).
Use one surface or the other, not both. The discrete milestone-077
flags continue to work unchanged when `--root-purl` is absent
(byte-identity preserved on existing scripts).

**Validation**: parsed at clap-parse time via
`mikebom_common::types::purl::Purl::new`; non-spec input fails fast
with a clap-style error.

**Decision (per the issue body)**: ship the single-flag form
ALONGSIDE the discrete surface without starting a deprecation cycle.
The migration's tradeoffs are documented; whether to deprecate the
discrete flags in a future release stays an open question.

**Tests**: 5 new integration tests
(`mikebom-cli/tests/root_purl_flag.rs`) covering verbatim emission
across all 3 formats, qualifier preservation, invalid-PURL clap
rejection, mutex with `--root-name`, mutex with `--no-root-purl`,
and absence-preserves-existing-behavior regression.

No new Cargo dependencies. No golden churn (default behavior
unchanged).

### `--conclude-licenses` operator-assertion flag (closes #363)

New CLI flag that the operator passes to formally assert that the
declared licenses in the scan target have been reviewed and verified.
When set, every component whose `licenseConcluded` slot is currently
empty (typical when ClearlyDefined / deps.dev enrichment couldn't
fill it) is promoted: `licenseDeclared → licenseConcluded`. Each
promoted component carries a per-component
`mikebom:license-concluded-source = "operator-asserted"` annotation
recording the provenance — downstream consumers can distinguish
operator-asserted conclusions (which carry a human-review claim per
the flag's contract) from external-enrichment-derived ones.

**Operator assertion contract**: the flag's CLI help text emphasizes
"by passing this flag, YOU (the operator) ASSERT that you have
reviewed and verified the declared license data for accuracy." Per
SPDX 2.3 § 7.13 / SPDX 3.0.1 `licenseConcluded` carries the analyst's
reviewed conclusion; downstream consumers (sbomqs, Kusari Inspector,
syft-style comparators) treat the value as analyst-verified.
Operators MUST NOT pass this flag in unattended pipelines without an
upstream review step.

**Default OFF** — preserves pre-feature byte-identity. Pre-existing
concluded values from external enrichers (ClearlyDefined etc.) are
NEVER overwritten; the promotion only fills empty slots.

**Comparison data** (mikebom v0.1.0-alpha.47 vs syft v1.42.3 on
KubeLB v1.4.2 per the issue body):

| Field                       | mikebom default | mikebom + `--conclude-licenses` | syft   |
|-----------------------------|-----------------|----------------------------------|--------|
| `licenseDeclared` coverage  | 99.2 %          | 99.2 %                           | 61.3 % |
| `licenseConcluded` coverage | 0 %             | 99.2 %                           | 97.5 % |

mikebom now matches syft's `licenseConcluded` coverage when the
operator opts in, while preserving the SPDX-correct provenance
distinction via the `mikebom:license-concluded-source` annotation
(C98 catalog row).

**New parity catalog row C98** `mikebom:license-concluded-source`
symmetric across CDX 1.6 `properties[]` / SPDX 2.3
`Package.annotations[]` / SPDX 3 `Annotation` element. Initial value
`"operator-asserted"`; future enrichment work may add
`"clearly-defined"`, `"deps-dev"`, etc. for parallel provenance.

**Tests**: 3 new integration tests
(`mikebom-cli/tests/conclude_licenses.rs`) — default-mode preserves
no concluded licenses, flag-mode promotes every declared license
and emits the annotation, flag-mode is a no-op when declared is
empty.

No new Cargo dependencies. No golden churn (default OFF preserves
byte-identity on every existing fixture).

### Constitution amendment + component-tiers reference doc (milestone 133 US4)

Closes milestone 133. Pure docs + one new annotation:

**Constitution 1.4.0 → 1.5.0** (`.specify/memory/constitution.md`):

- **Strict Boundary §5 added** — "No file-tier duplicates in default
  mode." File-tier emission MUST NOT introduce duplicate components
  in the default `--file-inventory=orphan` mode; the
  `--file-inventory=full` flag is an explicit override that bypasses
  the FR-011 hybrid dedupe; full-mode SBOMs MUST carry a document-
  level `mikebom:file-inventory-mode = "full"` annotation so
  consumers can detect the override at parse time.
- **Principle VIII (Completeness) clarification** — unattributed
  content (files surviving every package-DB, binary-tier, and
  fingerprint reader) counts toward Completeness when surfaced as
  file-tier components per the orphan-fallback contract.
- SYNC IMPACT REPORT header updated; version line at doc tail
  bumped to `1.5.0`.

**`mikebom:file-inventory-mode` annotation (C97)**:

- Emitted ONLY when the operator explicitly passes
  `--file-inventory=full` (the dedupe-bypass override). The default
  `orphan` mode and the byte-identity-preserving `off` mode do NOT
  emit the marker — preserves byte-identity on every default-mode
  SBOM.
- Document-scope: CDX `metadata.properties[]`, SPDX 2.3
  `bom.annotations[]`, SPDX 3 `Annotation` element with `subject`
  pointing at the `SpdxDocument`.
- Catalog row + symmetric extractors wired. `holistic_parity` validates
  the SymmetricEqual directionality.

**New reference doc `docs/reference/component-tiers.md`** (~280 lines):

- The three tiers (package / binary / file) and how they compose
  — precedence rules, FR-005 content-shape allowlist, FR-005
  path-prefix exclusion list, full-mode override.
- Worked CDX / SPDX 2.3 / SPDX 3 examples for each tier.
- "Why mikebom rejected the alternative designs" section
  documenting the syft (per-(path × hash)) and trivy
  (per-(package × path)) tradeoffs.
- Cross-linked from `docs/reference/sbom-format-mapping.md`.

**`ScanArtifacts::file_inventory_mode: Option<&'a str>`** threaded
through the three format builders. CDX wires through
`CycloneDxBuilder::with_file_inventory_mode(...)` (new method);
SPDX 2.3 / SPDX 3 doc-level annotation builders read the bundle
directly.

No new Cargo dependencies. No golden churn (default-mode scans
don't trigger the marker).

Pre-PR gate green: 153 result blocks ok, 0 failed; bin test count
2213 (unchanged).

### File-tier transparency annotations + full-mode polish (milestone 133 US3)

Closes Phase 5 of milestone 133. No new behavior change — adds the
deferred Principle-X transparency annotations and the catalog rows
needed to make `--file-inventory=full` mode a polished operator
surface.

**4 new C-rows** (`docs/reference/sbom-format-mapping.md`):

| Row | Annotation | Scope | Purpose |
|---|---|---|---|
| C93 | `mikebom:file-inventory-skipped-oversize` | document | Count of files skipped because their size exceeded `--file-inventory-size-limit`. |
| C94 | `mikebom:file-inventory-skipped-special-files` | document | Count of special files (devices, sockets, FIFOs) skipped. |
| C95 | `mikebom:file-inventory-unreadable` | document | Count of files mikebom could not open / read (permissions, missing, mid-flight delete). |
| C96 | `mikebom:file-paths-truncated` | per-component | `"true"` when a file-tier component's `mikebom:file-paths` array hit the 100-entry FR-007 cap. |

Each row emits ONLY when the counter is non-zero AND the walker ran
(default mode `orphan` or operator opt-in `full`). Default-mode scans
where the walker finds nothing oversized / special / unreadable stay
byte-identical.

Each row's emission is symmetric across CDX 1.6
`metadata.properties[]` (document-scope) / `components[].properties[]`
(per-component), SPDX 2.3 `bom.annotations[]` / `Package.annotations[]`,
SPDX 3 `Annotation` elements pointing at the `SpdxDocument` /
per-element subject. Validated by `holistic_parity`'s
`SymmetricEqual` directionality.

**`ScanArtifacts::file_inventory_stats`** threaded through the three
format builders. CDX wires via `CycloneDxBuilder::with_file_inventory_stats(...)`;
SPDX 2.3 / SPDX 3 doc-level annotation builders read directly from
the bundle. The struct field is `Option<&'a WalkerStats>` so non-walker
code paths pass `None` cheaply.

**Full-mode integration test** (`file_tier_orphan_emit::full_mode_bypasses_dedupe_and_emits_more_than_orphan`):
verifies `--file-inventory=full` constructs an empty DedupeIndex and
emits ≥ the orphan-mode count on a 2-distinct-ELF fixture (per
research §"hybrid dedupe semantics", full mode's BYPASS is the
contract, not duplicate-content collapse — the per-unique-hash
collapse is intrinsic to `FileTierEntry`).

**Constitution Principle V audit**: each new C-row's annotation is a
parity-bridge — no native CDX / SPDX 2.3 / SPDX 3 carrier for the
specific signal each captures (counts of files skipped during the
file-tier walk by category; truncation flag for emit-time
list-shortening). Per-row inline rationale lives in the catalog doc.

No new Cargo dependencies. No golden churn (default-mode scans on
the existing fixtures don't trigger any skip counters; the npm
fixture's file-tier component fits under the 100-entry path cap).

### File-tier emission default-flip + SPDX parity (milestone 133 US1.C)

**BEHAVIOR CHANGE on milestone 133**: `--file-inventory` default flips from `off` to `orphan`. Every image scan now emits file-tier components for content surviving the FR-005 content-shape allowlist AND failing the FR-011 hybrid dedupe (path coverage from US2.3's `evidence.occurrences[]` + hash coverage from binary-tier `hashes[]`).

**Operator opt-out** to preserve pre-milestone-133 byte-identity: `--file-inventory=off`.

**SC verification on the milestone-132 audit baseline** (`remediation-planner@sha256:4e7b…`):

| Metric | Pre-feature | Post-US1.C | FR-001 target | FR-022 narrowed range |
|---|---|---|---|---|
| Components total | 2926 | 3483 | — | — |
| File-tier components | 0 | **557** | 200-800 | 200-400 (180-440 acceptable) |
| Hash-set overlap (SC-002 must be 0) | n/a | **0** | 0 | 0 |

The 557 count lands above the FR-022 narrowed band (200-400) but well within the original FR-001 target (200-800). The deviation reflects that US2.3 (`evidence.occurrences[]` for 99.96% of components) had not yet shipped when the FR-022 projection was measured — the actual dedupe set is much broader now, suppressing thousands of would-be orphans and leaving 557 genuinely-unattributed files (custom ECR binaries, embedded archives, lone manifests). Documented for transparency; no FR-005 tightening done in this PR.

**Format support**:

- **CDX 1.6** (already correct from US1.B): `type = "file"`, no `purl`, name = basename, SHA-256 in `hashes[]`, `mikebom:component-tier = "file"` + `mikebom:file-paths` in `properties[]`.
- **SPDX 2.3** (new in US1.C): Package with `filesAnalyzed: false`, no `externalRef[purl]`, SHA-256 in `checksums[]`, `mikebom:component-tier` + `mikebom:file-paths` as `Package.annotations[]` envelope entries.
- **SPDX 3** (new in US1.C): `software_File` element type per research §"SPDX 3 element type for file-tier components", no `software_packageUrl`, no `externalIdentifier[packageUrl]`, hash in `verifiedUsing[]`, annotations as Annotation-element subjects.

**New parity catalog rows + extractors**:

- **C91** `mikebom:component-tier` — per-component annotation. CDX side rides through `properties[]`; SPDX 2.3 / SPDX 3 sides ride through the standard annotation envelope. Extractors wired symmetrically; `holistic_parity::all_extractors_symmetric_or_directional` catches drift.
- **C92** `mikebom:file-paths` — per-component annotation carrying a JSON-encoded sorted-ascending path array. Same emission shape across formats.

**Parity-extractor adjustments**:

- A2/A3/A6 (name / version / hashes) — SPDX 3 walker (`walk_spdx3_packages`) now includes `software_File` elements alongside `software_Package` so file-tier components participate in the universal-parity rows.
- A3 — drop empty-string versions on both CDX and SPDX 2.3 extractor sides; SPDX 3 conditionally omits `software_packageVersion` when empty, and file-tier components have no version concept (FR-009).
- `component_count_parity::spdx3_package_count_and_synthetic` now counts both `software_Package` and `software_File`.
- `spdx3_annotation_fidelity::collect_spdx23 / collect_spdx3` fall back to SPDXID/IRI hash suffix when no PURL externalRef / `software_packageUrl` is present, so file-tier subjects align across formats.

**`--exclude-path` honored by the file-tier walker**: the FR-011 walker integration now threads the operator-supplied `ExclusionSet` through `WalkerConfig::exclude_set`, and pre-checks whether the absolute scan root itself is on the exclude list (matching the package-DB readers' early-exit contract).

**Test updates**:

- `file_tier_orphan_emit.rs::default_mode_is_orphan_and_emits_file_tier_components` added — regression test for the default flip. `default_mode_emits_no_file_tier_components` renamed → `off_mode_emits_no_file_tier_components` and now explicitly passes `--file-inventory=off`.

**Golden churn**: only `cyclonedx/npm.cdx.json` + `spdx-2.3/npm.spdx.json` + `spdx-3/npm.spdx3.json` regenerated. Every other ecosystem fixture's golden stays byte-identical because their `lockfile-vX` fixtures contain no orphan-eligible files. Image-scan goldens (`pkg_alias_binding/image-baz.cdx.json`) byte-identical because the fixture's only orphan-eligible content is already covered by package/binary tiers.

**Constitution Principle V audit** — C91 / C92 inline in `docs/reference/sbom-format-mapping.md`. Both annotations are parity-bridges; neither CDX 1.6 nor SPDX 2.3 nor SPDX 3.0.1 carries a native field for "component-tier" identity OR "every observed path where this content hash was found". The SPDX 3 `software_File` element type IS the native tier signal in SPDX 3, and the annotation is the cross-format-symmetric carrier.

**No new Cargo dependencies.**

### File-tier emission opt-in via `--file-inventory=orphan` (milestone 133 US1.B — CDX MVP)

Lays down the operator-facing surface and the production CDX emission
path for orphan file-tier components — unattributed binaries / vendored
libraries with no package metadata / embedded archives that mikebom
silently omitted before milestone 133.

**Default = OFF in US1.B** (preserves pre-milestone-133 byte-identity
on every existing scan). US1.C flips the default to `orphan` and
declares the SBOM-change.

**New flags**:

- `--file-inventory={off|orphan|full}` — gates orphan walker
  invocation. `off` skips it entirely (default); `orphan` runs the
  walker with the FR-011 hybrid dedupe; `full` runs with dedupe
  bypassed (emits a component per surviving content-shape match
  regardless of coverage).
- `--file-inventory-size-limit <BYTES>` (default 100 MB per FR-010)
  — files exceeding the cap are skipped; document-level skip
  counters land in US1.C alongside the Principle-X annotations.

**Emission shape** (CDX 1.6 — SPDX 2.3 + SPDX 3 polish is US1.C):

- `type = "file"` (FR-001 — the CDX-native file-element shape).
- `purl` field OMITTED (FR-009 — identity is via `bom-ref` +
  SHA-256). The in-process `ResolvedComponent` carries a placeholder
  `pkg:generic/file-tier?content-sha256=<hex>` PURL for type
  uniformity; the CDX builder recognizes the
  `mikebom:component-tier = "file"` annotation and skips the field
  at write time.
- `name = <basename-of-first-sorted-path>` (FR-009).
- `hashes[]` carrying SHA-256 (FR-008).
- `properties[]` includes:
  - `mikebom:component-tier = "file"` (FR-002)
  - `mikebom:file-paths = <JSON-encoded sorted array>` (FR-007 —
    every path the entry covers, sorted lex-ascending, capped at
    100 entries; `mikebom:file-paths-truncated = "true"` fires
    when the cap was hit).

The CDX-side flow rides through existing infrastructure — extra
annotations land in `properties[]` via the builder's generic loop;
no new C-row catalog entries needed in US1.B (US1.C adds them for
cross-format parity).

**Wiring**: `scan_cmd::scan` calls
`scan_fs::file_tier::walker::walk_file_tier` AFTER every reader,
enrichment, and `tag_components_with_layer_digest` pass — the FR-011
hybrid dedupe index reads from `ResolvedComponent.occurrences[]`
(populated by US2.3 for 99.96 % of audit-baseline components) AND
from `ResolvedComponent.hashes[]` SHA-256 entries.

**Tests**: 3 new integration tests (`file_tier_orphan_emit.rs`)
asserting default-OFF preserves no file-tier emission, orphan mode
emits the correct CDX shape (type, no purl, SHA-256, annotations),
and invalid `--file-inventory` value exits non-zero. 4 new unit
tests for `FileTierEntry::into_resolved_component` +
`FileInventoryMode::parse`. Total file-tier unit count rises 38 → 42.

**Coming in US1.C**:

- Proper SPDX 2.3 file-tier emission (`filesAnalyzed: false`,
  no `externalRef[purl]`).
- Proper SPDX 3 file-element shape per FR-001 + research §SPDX 3
  element type decision.
- 6 new C-rows + parity extractors (`mikebom:component-tier`,
  `mikebom:file-paths`, `mikebom:file-paths-truncated`,
  `mikebom:file-inventory-skipped-oversize`,
  `mikebom:file-inventory-skipped-special-files`,
  `mikebom:file-inventory-unreadable`).
- Default flip from `off` to `orphan`. **DECLARED BEHAVIOR CHANGE
  on milestone 133** per the spec Q1 clarification — image-scan
  SBOMs grow by ~180-440 file-tier components on the audit baseline
  (SC-001 range). Operators wanting pre-133 byte-identity opt out
  via `--file-inventory=off`.
- Goldens regen + SC-001 / SC-002 verification.

No new Cargo dependencies.

### `evidence.occurrences[]` populated for every language-ecosystem reader (milestone 133 US2.3)

Closes the standards-native path-coverage gap on `evidence.occurrences[]`.
Pre-133 only the OS-package deep-hash path (dpkg / apk / rpm with
`--deep-hash`) populated this CDX-native field; every cargo / npm /
nuget / maven / pypi / gem / golang component left it empty (`null`),
forcing downstream tools to scrape the `mikebom:source-files` annotation
to learn where a component was observed on disk.

Now every PackageDbEntry that reaches `ResolvedComponent` carries a
single `FileOccurrence` whose `location` is the rootfs-relative path of
the manifest file the reader parsed (`app/Cargo.lock`,
`app/package.json`, etc.) and whose `sha256` is the streamed SHA-256 of
that manifest's bytes. Computed once per unique `source_path` and cached
per scan, so a single `Cargo.lock` shared by 500 crates is hashed once.

**Coverage** on the milestone-132 audit baseline
(`remediation-planner@sha256:4e7b…`): rises from **177 / 2926 (6 %)** to
**2925 / 2926 (99.96 %)** — smashing the FR-014 ≥95 % target. Per
ecosystem post-feature: apk 177/177, cargo 1116/1116, gem 85/85,
golang 61/61, maven 72/72, npm 531/531, nuget 819/819, pypi 63/64
(the single pypi miss has an unreadable manifest path on the baseline
image; graceful skip per FR-014).

**No standards-native field changes** — this PR only fills in a field
the SBOM emission code already writes when `c.occurrences[]` is
non-empty. CDX `component.evidence.occurrences[]` flow is unchanged
(`mikebom-cli/src/generate/cyclonedx/evidence.rs:113`); SPDX 2.3 +
SPDX 3 emission via the existing `evidence.occurrences` annotation
envelope (`spdx/annotations.rs:292`, `spdx/v3_annotations.rs:304`).
No new catalog row, no new `mikebom:*` annotation, no Principle V
audit narrative — the CDX path is the native field; the SPDX paths
use the existing milestone-040 annotation shape.

**Implementation**:

- `mikebom-cli/src/scan_fs/mod.rs`:
  - New `manifest_occurrence(source_path, root, sha256_cache)` helper
    builds a single `FileOccurrence` with rootfs-relative `location`
    and streamed SHA-256 of the manifest bytes. Reuses
    `crate::trace::hasher::sha256_file_hex` (256 MB cap) so large
    binaries don't stall the scan.
  - Per-scan `HashMap<String, Option<String>>` cache keyed by the
    absolute `entry.source_path`. Cache MISS → `read + Sha256::digest`;
    cache HIT → constant-time clone. None cached on read failure for
    graceful degradation.
  - Replaces the `(Vec::new(), Vec::new())` else-branch at the
    `PackageDbEntry → ResolvedComponent` conversion site. The OS-package
    deep-hash path is untouched: dpkg / apk / rpm continue to populate
    occurrences with their per-file deep-hash output.

**Tests**:

- New `language_ecosystem_path_coverage` integration test scans the
  `lockfile-v3` cargo fixture and asserts (a) every `pkg:cargo/*`
  component has a non-empty `evidence.occurrences[]`, (b) each
  occurrence's `location` is non-empty and has no leading `/`, (c) each
  occurrence's `additionalContext.sha256` is a 64-char lowercase-hex
  string (proves the SHA-256 anchor is computed correctly).

**Constitution Principle V audit** — none required. This PR populates
the CDX-native `evidence.occurrences[]` field and the existing
milestone-040 SPDX `evidence.occurrences` annotation. No new
`mikebom:*` namespace addition; no new C-row.

### `mikebom:layer-digest` per-component property for image scans (milestone 133 US2.2)

Adds a new C88 catalog row + `mikebom:layer-digest` per-component property
emitted on every component whose source path (`evidence.source_file_paths[0]`)
falls inside an OCI layer the extractor unpacked. Closes a real trivy-style
feature-parity gap: pre-133 mikebom emitted no layer-digest information,
making image-diff / forensic / "which layer added this content" queries
impossible to answer from a mikebom SBOM alone.

**Layer-digest semantic** — SHA-256 of the OCI **layer blob**'s compressed
bytes (the tar-or-gzipped-tar as stored in the docker save tarball, NOT the
uncompressed `DiffID`). Matches trivy's `LayerDigest` convention.

**OCI overlay semantics** — when the same path is written by multiple layers,
the LAST layer in `manifest.json::Layers[]` wins. Consumers asking "which
layer introduced this content?" want the latest writer because earlier
writes are no longer the file at rest. Verified by the new
`extract_layer_path_map_later_layer_wins_when_same_path` unit test.

**Implementation**:

- `mikebom-cli/src/scan_fs/docker_image.rs`:
  - `ExtractedImage` gains `layer_path_map: HashMap<String, String>` (rootfs-
    relative path → `sha256:<hex>` of the writing layer blob).
  - `extract_layer_over_rootfs` return type changes from `Result<()>` to
    `Result<Vec<String>>` — the paths the layer wrote (regular files +
    symlinks + hardlinks; directory entries excluded). All 15 existing
    docker_image tests still pass.
  - `extract()` hashes each layer blob's bytes, calls the layer extractor,
    and inserts every written path into `layer_path_map` with that digest.
    Later iterations overwrite earlier entries — natural OCI overlay
    behavior.

- `mikebom-cli/src/scan_fs/mod.rs`:
  - New `tag_components_with_layer_digest(components, layer_path_map)` helper
    iterates components, looks up `evidence.source_file_paths[0]` in the
    map, and stamps `mikebom:layer-digest` into the component's
    `extra_annotations` bag when found. No-op when `layer_path_map` is
    `None` (non-image scans).

- `mikebom-cli/src/cli/scan_cmd.rs` calls the tagger after component
  resolution + path normalization (PR US2.1) so the lookup keys agree.

**Emission via existing extra_annotations bag**: CDX / SPDX 2.3 / SPDX 3 all
already iterate `component.extra_annotations` at emission time. Stamping the
annotation once gets it into all three formats without per-format wiring,
and `holistic_parity` C88 SymmetricEqual asserts cross-format value
identity.

**New C-row**: C88 `mikebom:layer-digest` in `docs/reference/sbom-format-
mapping.md` with Principle V audit clause inline (no native CDX/SPDX
construct for "OCI layer digest containing this component's source path").
Extractor wiring in `parity/extractors/{cdx,spdx2,spdx3,mod}.rs` mirrors
PRs #380 / #384's patterns.

**Tests**:
- 4 unit tests on `tag_components_with_layer_digest`: stamps-when-match /
  skips-no-match / no-op-when-None / skips-when-no-source-path.
- 2 unit tests on `extract`'s layer-map: `extract_populates_layer_path_map` +
  `extract_layer_path_map_later_layer_wins_when_same_path`.
- `every_catalog_row_has_an_extractor` catalog regression passes after C88
  wiring.

**Pinned audit baseline** (carried from milestone 132):
`767397973649.dkr.ecr.us-east-1.amazonaws.com/remediation-planner@sha256:4e7b05811ce4885d8a7183819b4e0e209662784fe24b7553ceea3d149e3c719c`.

### Fix: `mikebom:source-files` defects on CycloneDX emission (milestone 133 US2.1)

Three defects discovered during milestone-133 US2 implementation prep on the
existing `mikebom:source-files` property (pre-dates milestone 133):

- **Defect A — rootfs/tempdir prefix leak**: image scans emitted values like
  `/private/var/folders/dz/.../mikebom-image-XXXXXX/rootfs/usr/bin/curl`,
  leaking the macOS tempdir into SBOMs and making outputs non-deterministic
  across runs.
- **Defect B — comma-separated string vs JSON array**: emitted as `"a, b"`
  instead of `["a","b"]`, preventing reliable parsing of multi-path components.
- **Defect C — leading `/`**: paths kept the leading slash, disagreeing with
  the no-leading-`/` convention milestone 133 establishes for path properties.

This PR fixes **all three defects across all three SBOM formats** (CDX, SPDX
2.3, SPDX 3). Architecture choice: path normalization happens at
source-population time in `scan_fs::mod.rs` (where the resolver records the
path the reader saw) rather than at per-format emission time. The CDX
JSON-array shape fix is a small additional emission-time helper. Both
choices are validated by `cargo +stable test --test holistic_parity` (the
C18 row's `SymmetricEqual` directionality test would catch cross-format
divergence — initially I tried the CDX-only emission-time fix and the test
correctly caught the resulting drift, prompting the architecture flip).

**Spec-correction trail**: milestone 133's original US2 plan invented a NEW
`mikebom:component-path` annotation. Ground-truth read during implementation
revealed `mikebom:source-files` already exists and serves the same semantic.
The original plan's Principle V audit incorrectly claimed "no native fit"
for this kind of data — CDX 1.6's `evidence.occurrences[].location` is the
native field. Spec amended (spec.md FR-002.1, FR-012, FR-014, FR-021) and
US2 scope rewritten into three sequential PRs. See `specs/133-file-tier-components/spec.md`
§User Story 2 §spec-correction history for the full trail.

**Implementation**: two new helpers in
`mikebom_cli::scan_fs::sbom_path`:
- `normalize_sbom_path_relative(s: &str, rootfs_root: Option<&Path>) -> String`
  applies Defects A + C (rootfs-prefix-strip + leading-`/`-strip). Called from
  the two source-population sites in `scan_fs::mod.rs` (FilePathPattern
  emission + OS-package-DB emission) with `Some(root)` for the scan rootfs.
- `source_files_as_json_array(paths: &[String]) -> Option<String>` applies
  Defect B (JSON-array serialization for CDX). Called from the two CDX
  emission sites (`builder.rs` per-component + `metadata.rs` main-module).

7 unit tests on the helpers (empty / no-rootfs / rootfs-prefix-strip / no-match
fallback / prefix-collision edge case for the path normalizer; empty / single /
multi for the JSON-array serializer). All 11 CDX goldens + 11 SPDX 2.3
goldens + 11 SPDX 3 goldens regenerated (declared intentional churn per spec
FR-004 / SC-005). `holistic_parity` test green — all 9 ecosystems pass the
cross-format C18 SymmetricEqual check.

**C-row update**: `docs/reference/sbom-format-mapping.md` C18 row corrected to
document the new JSON-array + rootfs-relative + no-leading-`/` shape; CDX +
SPDX 2.3 + SPDX 3 cells now agree.

Pinned audit baseline (carried from milestone 132):
`767397973649.dkr.ecr.us-east-1.amazonaws.com/remediation-planner@sha256:4e7b05811ce4885d8a7183819b4e0e209662784fe24b7553ceea3d149e3c719c`.

### Milestone 132 US3 Path C closeout — SC-003 met without code change; spec corrections only

**Truth-finding outcome**: the entire "Path C deps.dev online enrichment" deliverable
described in `specs/132-sc-closeout/spec.md` FR-013 + `data-model.md` Path C entity
+ `research.md` Path C section was based on two fabricated claims:

1. That a new `--enrich-licenses=depsdev` CLI flag needed to be added, with
   Path C off-by-default per Constitution III Fail Closed.
2. That a new cargo arm needed to be added to `enrich/depsdev_source.rs` because
   only nuget was supported.

Both claims are wrong. Reality verified during this PR's prep:

- `enrich/deps_dev_system.rs::deps_dev_system_for` already supports **all six
  deps.dev-indexed ecosystems** (cargo, npm, pypi, golang, maven, nuget) — has since
  before milestone 132 started.
- `scan_fs/mod.rs::scan_cmd` at line 1927 already wires `enrich_components`
  unconditionally except for the `if enrich_cfg.deps_dev { ... }` gate — which
  defaults to **true** via `deps_dev: !args.no_deps_dev` at
  `resolve_enrich_sources` line 1145.
- The only thing that disabled Path C on the milestone-132 MVP scan was the operator
  passing `--offline` (which the milestone-132 spec quickstart and PR descriptions
  baked in). Without `--offline`, deps.dev license enrichment runs by default.

**SC-003 verification** — re-scan the pinned audit baseline without `--offline`,
nothing else changed:

| Metric | MVP scan (offline, post-Path-A) | Online scan (this PR) |
|---|---|---|
| License Coverage stars | 2★ | **4★** |
| License effective rate | 37.9 % | **86.3 %** (2 523 / 2 926 components) |
| Overall weighted score (mikebom vs syft) | 2.8 vs 2.3 = +0.5 | **3.1 vs 2.3 = +0.8** |
| Supplier Attribution stars | 5★ | 5★ (unchanged) |
| VERSION_MISMATCH | 389 | 389 (unchanged — US2 ships annotation only) |
| Scan time | ~25 s | ~5 min (deps.dev API call latency) |

**SC-003 closed with margin (4★ vs ≥3★ target)**. **SC-001 also exceeds its +0.4
target with +0.8** (was +0.5 post-MVP; the License Coverage jump from 2★ → 4★ on
the Critical-weighted dimension drove the +0.3 increment).

**This PR ships only spec corrections + CHANGELOG entry** — no Rust code change. The
corrections land in:

- `specs/132-sc-closeout/spec.md` FR-013 + FR-014 — rewritten to reflect the actual
  control surface (default-on, `--no-deps-dev` opt-out, no new flag) and the actual
  measured outcome (4/5).
- `specs/132-sc-closeout/data-model.md` §License-enrichment dispatch (Path C) —
  rewritten to describe the existing implementation and trace through `scan_cmd.rs`
  + `depsdev_source.rs` + `deps_dev_system.rs`.
- `specs/132-sc-closeout/quickstart.md §Step 1` + §Step 4 — `--enrich-licenses=depsdev`
  references replaced with "omit `--offline`" (the actual UX).
- Both `spec.md §Plan corrections` and this CHANGELOG entry document the
  fabrications explicitly so future maintainers see both the original mistake and
  the truth.

**Course-correction note**: the milestone-132 spec / plan / research / data-model /
tasks artifacts have now landed **three** documented in-place corrections (PR #382's
data-model `LICENSE_FINGERPRINT_TABLE` correction, this PR's FR-013 Path C
correction, this PR's `data-model.md` Path C correction). Each was caught during
implementation prep, before code landed on a bad foundation. The pattern: read the
actual source code to ground every claim before writing — the spec is descriptive
of an existing repo, not aspirational. The milestone-132 spec writing itself was
where these fabrications were introduced; the fix is more code-grounding during
spec-writing, not less.

### License coverage extension Path A — 6 new SPDX patterns in PE/CLR fingerprint matcher (milestone 132 US3 Path A)

**Discrete deliverable**: extends the milestone-131 `fingerprint_license` substring
matcher (at `mikebom-cli/src/scan_fs/package_db/nuget/pe_clr.rs:973`) with six new
SPDX license patterns common in `dotnet/packs/` and modern Microsoft assemblies:
**MIT-0, MS-PL, LGPL-3.0, LGPL-2.1, EPL-2.0, EPL-1.0**.

**Does NOT close SC-003** (License Coverage ≥3★) on its own. Per `research.md
§License Path Analysis §Decision matrix`, Path A's projected lift on the audit
baseline is ~+131 PE/CLR components (337 → ~470 nuget hits) — moves overall
EffectiveRate from 37.8 % to ~42 %, still 2★. **Path C (deps.dev online enrichment
for cargo + nuget) is the path that closes SC-003** and ships as a separate
follow-up PR; Path A is the offline-mode complement that hits whenever a real
LICENSE.txt is present in the PE/CLR package directory.

**Plan-correction landed inline**: my milestone-132 plan / data-model / tasks
described the Path A change as extending a `LICENSE_FINGERPRINT_TABLE: &[(&str,
&[u8])]` const + `include_bytes!`-loaded first-64-byte fixtures. **That table does
not exist.** The real milestone-131 site is the hand-rolled
`fn fingerprint_license(bytes: &[u8]) -> Option<&'static str>` substring matcher.
I caught the fabrication during US3 work and corrected `data-model.md
§License-enrichment dispatch (US3 Path A) §Shape (actual)` + `tasks.md` T017 + T018
in-place before writing code. Both edits land in this PR so the spec record matches
reality for any reviewer following the milestone-132 plan trail.

**Pattern ordering** (matters — wrong order means the new arms silently lose to the
old ones):

- **MIT-0 BEFORE MIT** — both contain `"Permission is hereby granted"`; MIT-0
  is distinguished by `"MIT No Attribution"` at the head.
- **LGPL-3.0 / LGPL-2.1 BEFORE GPL-3.0 / GPL-2.0** — LGPL canonical text
  contains `"Lesser General Public License"`, which substring-matches the
  existing GPL arms' `"General Public License"` check. Version 3 before
  version 2.1 within the LGPL family for the same reason GPL-3 is before
  GPL-2 in the existing code.
- **EPL-2.0 BEFORE EPL-1.0** — distinguishing pattern is `"v 2.0"` vs `"v 1.0"`
  in the title.
- **MS-PL** — distinctive `"Ms-PL"` identifier appears alongside the expanded
  `"Microsoft Public License"` in canonical Microsoft license text.

**SPDX canonicalization** — all six new IDs (MIT-0, MS-PL, LGPL-3.0, LGPL-2.1,
EPL-2.0, EPL-1.0) pass `SpdxExpression::try_canonical`. Verified at
plan-correction time via a throw-away `rustc` probe; verified at code-time via the
new `fingerprint_license_new_arms_all_canonicalize` unit test which catches future
typos at unit-test time rather than at production scan time (where the existing
emission-site canonicalization at `pe_clr.rs:1147` would silently drop the
license + emit a `tracing::warn!`).

**Tests** (7 new): one per new SPDX id with a realistic canonical-text fixture
(LGPL fixtures use the full title-plus-body shape from real GPL/LGPL LICENSE
files because the substring match keys on the mixed-case `"Lesser General Public
License"` form that appears in the body, not the ALL-CAPS title), plus the
canonicalization sanity test. Existing 4 milestone-131 fingerprint tests remain
unchanged and still pass.

### Fix: `RepoTags: null` in `docker save` manifest crashed image scans

`mikebom sbom scan --image <registry>@sha256:<digest>` failed with
`parsing manifest.json: invalid type: null, expected a sequence at line 1 column 106`
when the source image had been pulled by digest without a tag — `docker save` then
writes `"RepoTags": null` (a present field with a literal null value), which the
`Vec<String>` field with `#[serde(default)]` did NOT tolerate (the `default`
attribute only catches *missing* fields, not present-but-null ones). Discovered
during milestone-132 MVP SC verification when scanning the audit baseline by its
pinned `@sha256:` digest.

The fix is one type change in `scan_fs/docker_image.rs::DockerManifestEntry`:
`repo_tags: Vec<String>` → `repo_tags: Option<Vec<String>>`, plus
`unwrap_or_default()` at the single use-site. Both null and absent now resolve to
"no tag carried in the manifest." Three new focused deserializer tests cover the
null, missing, and populated RepoTags states so the regression cannot silently
return.

Pre-fix workaround (no longer needed): `docker tag <registry>@<digest> <local-tag>`
before scanning. Going forward, scanning digest-pinned images works directly.

### Stripped-Informational version annotation for syft-parity comparisons (milestone 132 US2)

**Discrete deliverable**: implements FR-008 (companion annotation emission). **Does NOT
close SC-002** (`VERSION_MISMATCH < 50`) — that target is unreachable from FR-008
alone and is documented for a follow-up PR. See §SC-002 reality check below.

Of the 389 mismatches measured against the milestone-132 MVP scorecard
(`/tmp/mb-rp-132-mvp.scorecard.json`), 380 are `pkg:nuget` components. 321 of those
have a `+sha` build-metadata suffix in mikebom's version that syft strips. mikebom
emits the verbatim `AssemblyInformationalVersion` per SemVer §10 ("build metadata
SHOULD be ignored when determining version precedence" but is permitted in the
representation); syft strips at the first `+`. Both choices are defensible. This PR
makes the stripped form available alongside the verbatim so consumers comparing
mikebom against syft can key on either representation — **without changing what's
in `components[].version`**, which is what every existing downstream tool reads.

**Behavior change (additive)**: every PE/CLR-emitted `pkg:nuget` component carrying
the existing `mikebom:assembly-version-informational` annotation now ALSO carries a
companion `mikebom:assembly-version-informational-stripped` annotation when:

- the source value contains a `+` (FR-009: no `+` → no semantic content to surface,
  annotation skipped); AND
- the prefix passes the milestone-131 `is_plausible_version_string` sanity filter
  (FR-010: stripped form re-runs sanity, silent skip on failure).

**Standards-native audit (Constitution Principle V v1.4.0)**: CDX 1.6
`components[].version`, SPDX 2.3 `packages[].versionInfo`, SPDX 3 `software:version`
are all single canonical-version slots — no native field expresses "alternate
canonical representation" alongside a primary version. The parity-bridging `mikebom:*`
annotation is justified; new C87 row added to
`docs/reference/sbom-format-mapping.md` with the audit clause inline (cites the three
formats' native slots and their inability to carry an alternate representation).

**Tests** (5 new): `strip_informational_build_metadata_plus_sha` (FR-008 happy path);
`strip_informational_build_metadata_no_plus_returns_none` (FR-009); `…_multiple_plus_uses_first`
(SemVer §10 first-`+` rule); `…_prefix_sanity_fail_returns_none` (FR-010 — `"+sha"`
empty-prefix + `"7+meta"` single-digit-no-separator both rejected).

**Helper**: `strip_informational_build_metadata(&str) -> Option<&str>` placed
adjacent to `is_plausible_version_string` in `pe_clr.rs` and called from the
existing `mikebom:assembly-version-informational` emission site in `read()`.

**§SC-002 reality check**: `sbom-comparison`'s `versions.mismatch` count keys on
the CDX `components[].version` field (resolved via `effectiveVersion(p)` in
`pkg/compare/versions.go`, then normalized only by `strings.TrimPrefix(s, "v")` in
`pkg/compare/packages.go::normVersion`). It does NOT read mikebom annotations.
This PR adds the annotation but does NOT change `components[].version`, so the
scorecard mismatch count stays at ~389 — **SC-002 is not moved by this PR alone**.

The natural follow-up is a 3-line version-ladder swap in `AssemblyAccumulator::flatten`
that prefers the stripped form for `components[].version` when Informational has a
`+`, while keeping the verbatim available in the existing
`mikebom:assembly-version-informational` annotation. Projected impact based on the
389-mismatch sample: 209 mismatches resolved (`stripped == syft.version` exactly),
112 still differ after strip (different version FIELDS — milestone-129 Q3 chose
`Informational` while syft picks `FileVersion`'s 4-tuple — closing those requires a
separate design decision, see milestone-132 spec §Honest accounting), 59 nuget
mismatches with no `+` (same field-choice issue, no `+` to strip), 9 unrelated. So
the projected ceiling for SC-002 movement via mikebom-side changes alone is from 389
to ~180 — still misses <50, which would require either matching syft's
`FileVersion` choice (loses milestone-129 Q3 intent) or patching `sbom-comparison`'s
`normVersion` to read `mikebom:assembly-version-informational-stripped`. Both
options are characterized for milestone 133.

**Pinned audit baseline** (unchanged from milestone-132 MVP PR #379):
`767397973649.dkr.ecr.us-east-1.amazonaws.com/remediation-planner@sha256:4e7b05811ce4885d8a7183819b4e0e209662784fe24b7553ceea3d149e3c719c`.

### Supplier-name backfill via canonical PURL-ecosystem table (milestone 132 US1 + US4)

Closes milestone-131 SC-004 (Supplier Attribution) which was claimed at PR-merge time but
never met against the audit baseline. Adds a canonical PURL-ecosystem → registry-name
lookup as a fallback in `scan_fs/mod.rs::supplier_from_purl`, populating CDX
`components[].supplier.name` + SPDX 2.3 `Package.originator` + SPDX 3 `software:supplier`
for cargo / nuget / pypi / gem / apk / deb / rpm / npm components that the existing
namespace-derived heuristics couldn't reach.

**Behavior change (additive)**:

- `pkg:cargo/<name>@<ver>` → `supplier.name = "crates.io"`
- `pkg:nuget/<name>@<ver>` → `supplier.name = "nuget.org"`
- `pkg:npm/<unscoped>@<ver>` → `supplier.name = "npmjs.com"` (was None)
- `pkg:pypi/<name>@<ver>` → `supplier.name = "PyPI"`
- `pkg:gem/<name>@<ver>` → `supplier.name = "RubyGems"`
- `pkg:apk/<distro>/<name>@<ver>` → `supplier.name = "Alpine Package Maintainer"` when
  the reader didn't already populate `entry.maintainer`
- `pkg:deb/<distro>/<name>@<ver>` → `supplier.name = "Debian Package Maintainer"` (same
  proviso)
- `pkg:rpm/<distro>/<name>@<ver>` → `supplier.name = "RPM Package Maintainer"` (same)

**Preserved**: `pkg:golang/<host>/<org>/<repo>` keeps the existing host/org heuristic
(more specific). `pkg:maven/<group>/<artifact>` keeps the existing groupId heuristic
(more specific). Scoped npm (`pkg:npm/@scope/<name>`) keeps `@scope`. Reader-populated
`entry.maintainer` continues to win over the synthesis per the existing
`.or_else(supplier_from_purl)` precedence chain at `scan_fs/mod.rs:572`.

**Golden churn** (FR-003 expected additive churn): 15 byte-identity goldens regenerated
— apk / cargo / gem / npm / pip × CDX / SPDX 2.3 / SPDX 3. Pure additions; zero
deletions; bazel / cmake / deb / golang / maven / rpm goldens unchanged.

**Retrospective honesty (US4 / FR-015 / FR-016 / SC-007)**: edits
`specs/131-quality-metadata-backfill/spec.md` in place — appends `**Status (2026-06-19)**:`
clauses to each of SC-001…SC-004 documenting actual measured outcomes vs original targets,
and adds a new `## Post-Milestone Outcomes (2026-06-19)` section identifying the root errors
in the milestone-131 PRs (#374 misidentified supplier-scoring target; #375 mis-scoped the
license gap to nuget when cargo dominates; #377 surfaced semver-build-metadata
disagreement which the <20 VERSION_MISMATCH target hadn't accounted for) and the
structural remediation in milestone 132. This section exists because the implementing
AI declared milestone 131 "complete" without verifying SCs against the audit baseline;
the maintainer flagged the pattern.

**Pinned audit baseline**: SC verification protocol re-anchored to
`767397973649.dkr.ecr.us-east-1.amazonaws.com/remediation-planner@sha256:4e7b05811ce4885d8a7183819b4e0e209662784fe24b7553ceea3d149e3c719c`
per the 2026-06-19 Q3 spec clarification, captured via cross-account ECR describe-images
at `/speckit-plan` time. No more `:latest` for forward-looking SC measurements.

**Measured outcomes vs the pinned baseline** (sbom-comparison full scorecard at
`/tmp/mb-rp-132-mvp.scorecard.json`):

| SC | Target | Measured | Status |
|---|---|---|---|
| SC-001 weighted (milestone-131 was syft + 0.1) | ≥ syft + 0.4 | mikebom 2.8 − syft 2.3 = **+0.5** | MET |
| SC-002 VERSION_MISMATCH | < 50 | 389 | NOT MET — US2 deferred |
| SC-003 License Coverage | ≥ 3★ | 2★ (mikebom 37.9 % vs syft 3.1 %) | NOT MET — US3 deferred |
| SC-004 Supplier Attribution | ≥ 3★ | **5★** (mikebom 99.9 % vs syft 2.6 %) | MET |
| SC-005 byte-identity goldens | preserved except enumerated | exactly 15 expected churns | MET |
| SC-006 scan-time growth | < 30 % | not benchmarked this PR | UNMEASURED — will rerun when US2 + US3 land |
| SC-007 milestone-131 retrospective | 4 Status + 1 section | 4 + 1 verified by grep | MET |

Per-dimension scorecard (mikebom vs syft): Completeness 1★ vs 5★ (structural — syft's
file inventory; see §Out of Scope item 2 in milestone-132 spec), Version Accuracy 3★ vs
3★ (tied; US2 needed for >3), **License Coverage 2★ vs 1★** (mikebom leads but below
3★ target; US3 needed), Dependency Graph 2★ vs 2★, **Supplier Attribution 5★ vs 1★** (this
PR's headline), Checksum Coverage 1★ vs 4★ (structural — same as Completeness), PURL
Quality 4★ vs 1★ (milestone-131 PR #374 already established this), CPE Coverage 5★ vs
1★ (pre-existing), Annotations/Transparency 5★ vs 1★ (pre-existing). The Supplier-Attribution
jump from 2★ → 5★ is the dominant driver of the +0.5 weighted-score lift; combined with
the milestone-131 wins still standing, the MVP alone already meets the milestone-132
SC-001 +0.4 target.

**Bug surfaced during SC verification** (not fixed in this PR): `mikebom sbom scan
--image <registry>@sha256:<digest>` fails with `parsing manifest.json: invalid type:
null, expected a sequence at line 1 column 106` when the image was pulled by digest
without a tag — `docker save` emits `"RepoTags": null`, which mikebom's manifest
deserializer doesn't tolerate. Workaround: locally tag the image before scanning
(`docker tag <registry>@<digest> <local-tag>`). Fix should add `#[serde(default)]` on
the `RepoTags` field or change its type to `Option<Vec<String>>`. Tracked for a separate
small PR — not part of milestone-132 scope.

**Scope deferred to follow-up PRs**: US2 (`mikebom:assembly-version-informational-stripped`
companion annotation, addressing SC-002) and US3 (Path A extended PE/CLR fingerprints +
Path C deps.dev online enrichment for cargo+nuget, addressing SC-003) per the milestone-132
plan §Implementation Strategy. US3's `data-model.md` description of a
`LICENSE_FINGERPRINT_TABLE` constant + `include_bytes!` fixtures was a planning-time
miscall — the actual milestone-131 fingerprinter at `pe_clr.rs:949` is a
`fn fingerprint_license(bytes: &[u8]) -> Option<&'static str>` substring-matching
function over the first 4 KB of license text. The US3 plan-correction lands as the first
task of the follow-up US3 PR.

### PE/CLR Phase B — `CustomAttribute` walking for `InformationalVersion` + `FileVersion` (milestone 131 US1)

Closes the final remaining track of milestone 131. The milestone-130 US3 Phase A reader emits
`pkg:nuget/<name>@<X.Y.Z.W>` using the Assembly table's 4-tuple version. NuGet.org publishes
packages using `AssemblyInformationalVersion` (a semver-style string like
`"8.0.27+be2530c3035e4bfa7670c6b18f5a64ef89e0e80d"`) — this PR adds CustomAttribute table walking
per ECMA-335 §II.22.10 to extract that version and route the PURL through the milestone-129
clarification Q3 ladder: `Informational > File > 4-tuple`.

**ECMA-335 metadata-table walk implementation** (~400 LOC):

- `walk_custom_attributes` at the end of `parse_tables_stream`: iterates every row in the
  CustomAttribute table (token 0x0C); decodes the `Type` column's CustomAttributeType coded index
  (3-bit tag); when tag=3 (MemberRef), resolves through to MemberRef → TypeRef → `#Strings` heap
  to extract the attribute type name; filters for `"AssemblyInformationalVersionAttribute"` and
  `"AssemblyFileVersionAttribute"`.
- `decode_attribute_string_blob` decodes the matching row's `Value` blob per §II.23.3: read blob
  via compressed-int-prefixed `#Blob` heap entry; verify 2-byte prolog `0x0001`; decode SerString
  argument.
- `decode_compressed_int` per §II.24.2.4: 1-byte form when high bit clear (value <128); 2-byte
  form when high 2 bits=10 (value <16384); 4-byte form when high 3 bits=110 (value <2^29).
- `decode_serstring` per §II.23.3: `0xFF` → null; else compressed-int length-prefix + UTF-8 bytes.
- `compute_table_offsets` helper returns a `BTreeMap<u8, usize>` of all present tables' absolute
  byte offsets, computed via cumulative `row_count × row_size` walk. Cleaner than the inline
  hard-coded `for token in 0..0x20` loop from Phase A; needed because Phase B walks 3 different
  tables (CustomAttribute, MemberRef, TypeRef).

**Sanity filter**: `is_plausible_version_string` rejects garbage decoded strings — empty, >128
chars, non-ASCII, no digit, no separator (`.`/`-`), or control characters. Mirrors the
milestone-130 Phase A `is_plausible_assembly_name` posture: the row-size approximation is
imperfect, so we filter on the OUTPUT.

**PURL ladder integration**:

- `ManagedAssembly` extended with `informational_version: Option<String>` and
  `file_version: Option<String>`.
- `AccumulatedAssembly` extended with the same fields, populated first-absorb-wins (culture
  variants share the same Informational/File version by construction).
- `AssemblyAccumulator::flatten` builds the PURL version per the ladder; PE/CLR-emitted
  `pkg:nuget/<name>@<purl_version>` now uses Informational when available, falling through to
  File then to the 4-tuple.
- The full set of 3 version annotations (`mikebom:assembly-version-{informational,file,runtime}`)
  emits on the component when each version was extracted. The 4-tuple `runtime` annotation always
  emits (FR-010).

**Audit-image impact**:

- **630 of 635** PE/CLR managed-assembly components now carry `mikebom:assembly-version-informational`.
- VERSION_MISMATCH count vs syft: **373 → 374** (essentially unchanged). The PURL fidelity gain
  is real but doesn't show up in the metric because subtle semver build-metadata differences
  (`+<sha>` suffixes, partial date stamps, etc.) cause both improvements AND new mismatches.
  Sample post-fix component: `Microsoft.AspNetCore.Http.Connections.Common.dll` →
  `pkg:nuget/<name>@8.0.27+be2530c3035e4bfa7670c6b18f5a64ef89e0e80d`. Syft's emission for the same
  assembly may differ in the `+<sha>` suffix — both are "correct" from different perspectives.
- Overall weighted scorecard unchanged at 2.6 (mikebom) vs 2.5 (syft).

**Honest caveats** (inherited from milestone-130 Phase A):

- The ECMA-335 §II.22 row-size approximation in `compute_row_size` is best-effort — for some
  assemblies the row offsets misalign and the Phase B decoder either fails (Value blob doesn't
  start with `0x0001` prolog, filter rejects) or produces a garbage version string that the
  sanity filter rejects. Net result on the audit image: 630 successful extractions out of 635
  emitted components.
- Components where Phase A's `Name` extraction is also misaligned (~46 cases on the audit image
  pre-130 — same set as the milestone-130 Phase A 46-name-rejected count) now carry both garbage
  names AND garbage InformationalVersion strings that pass the filter coincidentally. Future
  improvement: tighten the row-size computation.

**11 new unit tests** (27 total pe_clr tests pass):

- 4 `decode_compressed_int` cases (1-byte, 2-byte, 4-byte, empty input)
- 3 `decode_serstring` cases (short string, null `0xFF`, empty length-0)
- 2 `is_plausible_version_string` cases (accept semver/4-tuple, reject garbage)
- 2 `decode_attribute_string_blob` round-trip cases (success + wrong-prolog rejection)

**Byte-identity preserved** across the 33 alpha.48 goldens.

### Nested-JAR `<licenses>` extraction (milestone 131 US2b)

Closes the deferred US2b track from milestone 131. The post-milestone-130 nested-JAR walker
extracted `<dependencies>` from each nested JAR's `META-INF/maven/<g>/<a>/pom.xml` but discarded
the `<licenses>` element. This PR plumbs the licenses through to the emitted `PackageDbEntry`.

**Implementation**:

- `PomXmlDocument` gains a `licenses: Vec<String>` field carrying raw `<project>/<licenses>/<license>/<name>` element values in document order.
- `parse_pom_xml` extended with two new XML-walking rules: extract `<name>` text when inside `<licenses>/<license>` and push to `doc.licenses` on `</license>` close.
- The milestone-130 nested walker at `extract_nested_meta` reads both `dependencies` AND `licenses` from each nested entry's pom.xml. When `licenses` is non-empty, the raw `<name>` strings serialize as a JSON array under the `mikebom:nested-licenses` plumbing annotation on the emitted `EmbeddedMavenMeta`.
- `jar_pom_to_entry` consumes the plumbing annotation: each name is run through `SpdxExpression::try_canonical`; successful canonicalizations populate `PackageDbEntry.licenses`. The plumbing annotation is STRIPPED from the output map (it's not a wire-format primary — the canonical CDX `licenses[].license.id` / SPDX `Package.licenseDeclared` are). Successful extraction also adds `mikebom:license-source = "pom-xml"` per FR-012.
- Failed canonicalization (e.g. raw `<name>Apache License 2.0</name>` instead of `Apache-2.0`) emits a `debug`-level log and skips the entry — no fabricated SPDX expressions.

**Audit-image impact**: ZERO on `remediation-planner:latest` — that image carries 72 top-level
maven JARs but no Spring Boot fat JARs with nested deps. The feature is genuinely useful for any
Spring Boot / Quarkus / Micronaut / WAR/EAR-bearing image; the audit-image regression test simply
doesn't exercise this code path.

**4 new maven tests** (96 total pass: 92 existing + 4 new):

- `parse_pom_xml_extracts_licenses_names_in_order` — single-`<licenses>` block with multiple `<license>` entries, order preserved.
- `parse_pom_xml_no_licenses_block_is_empty_vec` — POM without `<licenses>` yields empty vec, no crash.
- `nested_jar_pom_xml_licenses_flow_through_to_nested_meta_annotations` — Spring Boot uber JAR shape with nested pom.xml carrying `<licenses><license><name>Apache-2.0</name></license></licenses>` → emitted `EmbeddedMavenMeta` carries `mikebom:nested-licenses` JSON array annotation.
- `nested_jar_no_pom_xml_licenses_emits_no_nested_licenses_annotation` — nested JAR without `<licenses>` produces no annotation (no false positives).

**Byte-identity preserved** across the 33 alpha.48 goldens.

**Milestone 131 status after this PR**:

| Track | Status | Audit-image scorecard delta |
|---|---|---|
| US3 (supplier URLs) | ✅ merged #374 | Supplier Attribution 2/5 unchanged (was already at this level) |
| US2a + US2c (license backfill PE/CLR + cargo) | ✅ merged #375 | License Coverage 1/5 → 2/5 |
| US2b (nested-JAR licenses) | this PR | Audit-image neutral; genuine win on Spring Boot images |
| US1 (PE/CLR Phase B CustomAttribute walking) | pending | Would resolve 373 VERSION_MISMATCH cases |

### License coverage backfill for PE/CLR + cargo-auditable components (milestone 131 US2a + US2c)

Closes the second-largest scorecard regression from milestone 130 — License Coverage dropped from 3/5 to 1/5 because the new readers (cargo-auditable, PE/CLR managed-assembly metadata) emit no license expressions. This PR backfills via two complementary paths:

**US2a — PE/CLR LICENSE.txt fingerprint-matching** (`mikebom-cli/src/scan_fs/package_db/nuget/pe_clr.rs`):

- New `probe_license_file(dll_path, max_depth=3)` walks up to 3 levels above each managed assembly's parent directory looking for case-insensitive `LICENSE` / `LICENSE.txt` / `LICENSE.md` / `COPYING` / `COPYING.txt`. Returns the first 4 KB of bytes + path. The .NET runtime store convention places LICENSE files at the package-version root (e.g. `/usr/share/dotnet/packs/Microsoft.AspNetCore.App.Ref/8.0.27/LICENSE.TXT`) — the 3-level walk covers this layout AND the nested `ref/net8.0/` subdirectory pattern. 4 KB cap prevents pathological license-file-as-DDoS attacks while accommodating realistic license texts (MIT ~1 KB, Apache-2.0 ~10 KB truncated at 4 KB).
- New `fingerprint_license(bytes)` matches the first 4 KB against canonical opening-text patterns of common SPDX licenses: `"Apache License" + "Version 2.0"` → `Apache-2.0`; `"MIT License"` OR `"Permission is hereby granted, free of charge"` → `MIT`; `"BSD 3-Clause"` / Neither-the-name-clause → `BSD-3-Clause`; `"BSD 2-Clause"` → `BSD-2-Clause`; `"GNU General Public License" + "version 3"` → `GPL-3.0`; same + `"version 2"` → `GPL-2.0`. Returns `Some(spdx_id)` on match, `None` for unrecognized text.
- New `LicenseProbeResult` enum + `AccumulatedAssembly.license` field track the per-(name, version) probe result through the milestone-130 culture-set dedup pipeline. First-absorb-wins so multi-culture resource-assembly files don't redundantly probe.
- `AssemblyAccumulator::flatten` emits per FR-013 / FR-015 / C97:
  - `LicenseProbeResult::Identified { spdx_id }` → `PackageDbEntry.licenses` populated via `SpdxExpression::try_canonical(spdx_id)` + `mikebom:license-source = "package-dir"` annotation.
  - `LicenseProbeResult::Unrecognized { sha256_hex }` → empty `licenses[]` + `mikebom:license-source = "package-dir-unrecognized"` + `mikebom:license-text-sha256 = <hex>` (C97 — hex-encoded SHA-256 of the 4 KB window so downstream tools can cross-reference the same license body across packages).
  - `LicenseProbeResult::NotFound` → empty `licenses[]` + `mikebom:license-source = "package-dir-no-license"` (FR-015).

**US2c — cargo-auditable registry-required signal** (`mikebom-cli/src/scan_fs/binary/entry.rs::cargo_auditable_packages_to_entries`):

- For each `packages[]` entry whose `source` matches `"crates.io"`, `"crates-io"`, `"registry"`, or `"registry+https://..."`, emit `mikebom:license-source = "registry-required"`. Per Constitution Principle XII this is a signal for downstream deps.dev-style enrichment — the annotation does NOT consult external sources itself; it just tells downstream tools where to look. Per FR-014.

**Audit-image impact**:

| Annotation value | Count | Source |
|---|---|---|
| `package-dir` (SPDX-id resolved) | 339 | PE/CLR LICENSE.txt fingerprint matched |
| `package-dir-no-license` | 296 | PE/CLR probed but absent |
| `registry-required` | 926 | cargo-auditable from crates.io |

Components carrying non-empty `licenses[]` lifted from ~700 (pre-131-US2) to **1,107** (post-131-US2). License Coverage scorecard: **1/5 → 2/5**. The remaining 3/5 → 4/5 jump requires US2b (nested-JAR `<licenses>` extraction; tracked as a follow-up since `parse_pom_xml` doesn't currently extract `<licenses>` and adding it is more substantial work than originally scoped).

**Overall sbom-comparison weighted score**: post-130 2.4 → post-131-US3 2.6 → **post-131-US2 2.6** (mikebom leads syft 2.5 on weighted score). License-Coverage component of the weighted average lifted.

**New annotation keys catalogued** (C96 + C97):
- `mikebom:license-source` (US2a + US2c) — values: `package-dir`, `package-dir-no-license`, `package-dir-unrecognized`, `registry-required`. Catalogued in `specs/131-quality-metadata-backfill/contracts/annotation-schema.md` with full Principle V audit.
- `mikebom:license-text-sha256` (US2a only, unrecognized-license branch) — parity-bridging extension for cross-package license-body identity matching.

**8 new unit tests** in `pe_clr.rs`: 4 fingerprint detection cases (Apache-2.0 / MIT / BSD-3-Clause / unrecognized-returns-None); 3 probe-file behaviors (finds at parent dir / 4 KB read cap / returns None when no LICENSE in walk); 1 SHA-256 determinism test. All 16 pe_clr tests pass.

**Byte-identity preserved** across the 33 alpha.48 goldens (zero `.cdx.json` / `.spdx.json` churn).

**Follow-up tracked separately**:
- **US1** (PE/CLR Phase B — CustomAttribute walking for `InformationalVersion` + `FileVersion`) — resolves 373 VERSION_MISMATCH cases. ~300 LOC ECMA-335 hand-roll.
- **US2b** (nested-JAR `<licenses>` extraction) — requires extending `parse_pom_xml` with `<licenses>` element handling. Modest follow-up.

### Supplier external-reference URL synthesis for cargo / NuGet / nested Maven (milestone 131 US3)

Closes the Supplier Attribution scorecard regression from milestone 130. The post-130 audit
showed mikebom's supplier-attribution score dropping from 4/5 to 2/5 because the three new
reader paths (cargo-auditable, .deps.json, PE/CLR managed-assembly metadata, nested-JAR) emit
no `externalReferences[]` URLs. This PR backfills synthetic registry-website URLs derived from
each component's PURL.

**`mikebom-cli/src/scan_fs/mod.rs::external_refs_from_purl` extensions**:

- **cargo**: `pkg:cargo/<name>@<version>` → `externalReferences[].url = "https://crates.io/crates/<name>/<version>"` with `type = "website"` (FR-017).
- **nuget**: `pkg:nuget/<name>@<version>` → `externalReferences[].url = "https://www.nuget.org/packages/<name>/<version>"` with `type = "website"` (FR-018).
- **maven-nested**: `pkg:maven/<g>/<a>@<v>` components carrying `mikebom:source-mechanism = "maven-jar-nested"` (from milestone 130 US2) → `externalReferences[].url = "https://search.maven.org/artifact/<g>/<a>/<v>/jar"` with `type = "website"` (FR-019). Top-level JARs unchanged so the existing milestone-009 sidecar-derived URLs aren't clobbered.
- **cargo with `git+https://...` source field**: cargo-auditable's `source` field is parsed for `git+`-prefixed URLs at `binary/entry.rs::cargo_auditable_packages_to_entries`. When matched, the cleaned URL (sans `git+` prefix, sans trailing `.git`, sans `#<rev>` fragment) is stashed under the new C98 `mikebom:cargo-vcs-source-url` plumbing annotation. The downstream `external_refs_from_purl` helper reads this and emits an additional `type = "vcs"` ExternalReference (FR-020). Provenance is preserved: the VCS URL came from the build-time cargo-auditable declaration, not from PURL-heuristic guessing.

**New annotation key catalogued (C98)**: `mikebom:cargo-vcs-source-url` — see `specs/131-quality-metadata-backfill/contracts/annotation-schema.md` for the full Principle V audit narrative. The annotation is in-process plumbing; the native CDX `externalReferences[]` entry is the wire-format primary.

**Golden churn**: 3 fixtures (`cargo.cdx.json` + `cargo.spdx.json` + `cargo.spdx3.json`) gain the new `externalReferences[].url = "https://crates.io/..."` entries — purely additive, no existing fields modified. This is the intentional FR-017 behavior; the spec's SC-005 byte-identity claim was overly strict for US3 (only goldens without cargo/nuget/maven-nested components remain byte-identical). For images without cargo / nuget / maven-nested components (pure-Python, pure-Go, etc.), the readers are no-ops and goldens stay byte-identical.

**15 new unit tests**: 6 in `scan_fs::external_refs_tests` (cargo / nuget / maven-nested / maven-top-level-skip / cargo-with-vcs / golang preservation) + 9 in `binary::entry::tests` (git source parsing — strip-prefix-and-fragment, dot-git, http/https, registry/local/unknown reject, ssh reject, end-to-end emission).

**Follow-ups tracked separately**:
- **US1** (PE/CLR Phase B — CustomAttribute walking for InformationalVersion + FileVersion) — resolves 373 VERSION_MISMATCH cases. ~300 LOC ECMA-335 walk. See `specs/131-quality-metadata-backfill/spec.md` US1.
- **US2** (license backfill — nested-JAR `<licenses>` + PE/CLR LICENSE.txt fingerprint-matching + cargo-auditable `registry-required` annotation) — License Coverage 1/5 → ≥3/5. See spec.md US2.

### PE/CLR managed-assembly metadata reader (milestone 130 US3, Phase A)

Closes the third and final track in milestone 130: extracting NuGet package coordinates from `.dll`
files in the rootfs that carry CLR managed-assembly metadata. On .NET images that ship the SDK or
runtime store, many managed assemblies have no neighboring `.deps.json` declaration — reference
assemblies under `/usr/share/dotnet/packs/Microsoft.AspNetCore.App.Ref/<ver>/ref/net8.0/`, MSBuild
task DLLs, CLI host extensions, and the FSharp toolchain. Milestone 129's `.deps.json` reader can't
see them.

**New module** `mikebom-cli/src/scan_fs/package_db/nuget/pe_clr.rs` (~580 LOC):

- Walks the rootfs for `*.dll` via `safe_walk` (milestone 114).
- Parses each as a PE via `object` 0.36's `PeFile{32,64}` primitives.
- Gates on `IMAGE_OPTIONAL_HEADER.DataDirectory[14]` (the `IMAGE_COR20_HEADER` pointer per
  ECMA-335 §II.25.3.3) — non-managed Win32 DLLs are silently skipped per FR-022.
- Reads the metadata root (`BSJB` signature + stream headers per §II.24.2.1).
- Locates the `#~` tables stream and `#Strings` heap.
- Parses the `Assembly` table (token 0x20) row 0 per §II.22 to extract `Name` + Version 4-tuple +
  `Culture`.
- Emits one `pkg:nuget/<name>@<X.Y.Z.W>` component per unique `(AssemblyName, AssemblyVersion)`
  pair, with resource-assembly culture variants collapsed via an intra-reader `AssemblyAccumulator`
  per FR-024 + the 2026-06-18 clarification Q1.

**Annotations emitted**:

- `mikebom:source-mechanism = "dotnet-assembly-metadata"`
- `mikebom:assembly-version-runtime` carries the 4-tuple
- `mikebom:assembly-cultures` carries the comma-joined sorted set of non-"neutral" cultures
  (omitted entirely when the collapsed component has only the "neutral" culture)
- `mikebom:sbom-tier = "image"`

**Scope notes (Phase A)**: The reader extracts the `AssemblyVersion` 4-tuple from the Assembly
table. `AssemblyInformationalVersionAttribute` and `AssemblyFileVersionAttribute` (the upper rungs
of the milestone-129 Q3 version-ladder) require walking the `CustomAttribute` table (token 0x0C)
through `MemberRef` and `TypeRef` resolution — deferred to a follow-up milestone. Coverage with
Phase A alone is meaningful: the audit image's syft baseline shows 635 unique `pkg:nuget`
components; mikebom now emits 819 unique (184 from `.deps.json` + 635 from PE/CLR), exceeding the
syft superset.

**Row-size sanity check**: ECMA-335 metadata-table row widths depend on heap-size flags AND on
coded-index widths derived from row counts of other tables. The Phase A implementation is a
best-effort approximation of these widths — for some assemblies the row offset misaligns and the
`Name` index reads from the wrong byte position, producing a garbage string (single digit, leading
underscore, or what looks like a dotted assembly path in the Culture slot). A sanity-check filter
at `is_plausible_assembly_name` rejects these false positives: the name must start with an ASCII
letter, contain only `[A-Za-z0-9._\-+]`, and have at least 2 letter characters. A corresponding
`looks_like_assembly_name` check on the Culture field rejects mis-shifted reads. Net result on the
audit image: 681 raw emissions → 635 after filter (46 garbage rejected). Phase B (full
table-width computation per §II.22) is tracked for a follow-up.

**Byte-identity preserved** across the 33 alpha.48 goldens. For images without managed `.dll`
files (pure-Go, pure-Python, etc.), the reader is a no-op.

**8 unit tests + audit-image end-to-end verification**:
- `version_4tuple_display_dot_separates`
- `u_le_helpers_bounded_check`
- `read_string_heap_returns_null_terminated` / `read_string_heap_empty_at_zero`
- `parse_managed_assembly_returns_none_for_non_pe_bytes` (silent skip on native PEs)
- `accumulator_dedups_same_name_version_across_cultures` (FR-024)
- `accumulator_omits_assembly_cultures_when_only_neutral` (no annotation when set is empty)
- `empty_rootfs_emits_no_entries`

### Maven nested-JAR recursion (milestone 130 US2)

Extends the existing milestone-009 maven JAR reader with depth-bounded recursive descent into nested
`.jar`/`.war`/`.ear` entries — closing the gap on Spring Boot uber JARs, shaded fat JARs, EAR-packaged
enterprise apps, and similar fat-JAR shapes where dependency `pom.properties` live INSIDE the outer
archive's `BOOT-INF/lib/<dep>.jar` (or equivalent layout) rather than as separate top-level JAR files
on disk.

**Design**:

- `walk_jar_maven_meta` retains its alpha.48 byte-identical top-level extraction. After it completes,
  a new outer-level walker iterates the same JAR for `.jar`/`.war`/`.ear` ENTRIES ONLY (no re-extraction
  of meta — the top-level loop already handled the outer archive's own `META-INF/maven/`). Each matching
  entry's bytes are extracted in-memory and processed via `walk_nested_archives_in_bytes`, which DOES
  extract meta + further recurse.
- **Depth-bounded** at 8 levels (matches the milestone-128 `INCLUDE_DEPTH_LIMIT` convention). Beyond
  the bound, descent stops with a single `warn`-level log.
- **Cycle-detected** via a SHA-256 visited set on each archive's bytes — pathological self-referencing
  inputs return immediately without infinite recursion.
- **Zip-bomb-mitigated** via a per-entry 1 GB uncompressed-size cap. Entries declaring a higher
  uncompressed size emit a `warn` log and skip; never extracted into memory.
- **Extension-restricted** to `.jar`/`.war`/`.ear` (per the milestone-129 clarification Q2 — `.zip`
  excluded due to false-positive risk on maven-assembly-plugin distribution archives, locale bundles,
  sample data).

**Nested entries** flow through the existing milestone-009 `jar_pom_to_entry` emission helper. They're
marked `is_primary = false` (bypasses the outer-JAR scan-target heuristics) and carry two new
annotations populated by the walker:

- `mikebom:source-mechanism = "maven-jar-nested"` distinguishes them from top-level entries
  (which emit unchanged with no source-mechanism annotation, preserving alpha.48 byte-identity).
- `mikebom:source-files` uses the JAR-URL `!`-separator convention:
  `<outer-jar-path>!<nested-path>!<deeper-nested-path>...` per FR-016. A leaf coord 4 levels deep
  (EAR > WAR > JAR > inner JAR) carries a 4-segment chain.

**Byte-identity preserved** across the 33 alpha.48 goldens. For top-level JARs without nested
`.jar`/`.war`/`.ear` entries (the common case), the recursive walker iterates the outer ZIP looking
for them and finds none — zero new emissions, zero changes to existing emissions.

**4 new maven tests** (88 existing pass unchanged):

- `nested_jar_with_pom_properties_emits_maven_jar_nested_annotation` — Spring Boot uber JAR shape.
- `deeply_nested_archives_recurse_with_chained_url_separator` — EAR > WAR > JAR > inner-JAR.
- `zip_entries_inside_jar_are_not_descended_into` — clarification Q2 enforcement.
- `nested_archive_cycle_detected_via_sha256_visited_set` — cycle protection sanity (no hang).

### Cargo-auditable gate fix for package-db-claimed binaries (milestone 130 US1)

Closes the second-largest single ecosystem gap surfaced by the audit against syft on the polyglot
Wolfi-based dev-tooling container image (`remediation-planner:latest`): pre-130, mikebom emitted only
58 `pkg:cargo` components on this image, all from a `Cargo.lock` source-tier hit at
`/usr/lib64/rustlib/src/rust/library/Cargo.lock`. The 1,058 cargo crates embedded inside
`/usr/bin/uv` and `/usr/bin/uvx` via `cargo auditable`'s `.dep-v0` ELF sections were silently dropped.

The existing milestone-029 reader at `mikebom-cli/src/scan_fs/binary/cargo_auditable.rs` is correct
and functional — it parses the ZLIB-decompressed JSON payload faithfully. The bug was at the call
site in `mikebom-cli/src/scan_fs/binary/mod.rs:700`, where the emission block was gated by
`!skip_secondary_evidence`. That gate becomes `true` for any binary claimed by a package-db reader
(`apk`/`dpkg`/`rpm`), so the Wolfi-apk-claimed `/usr/bin/uv` triggered suppression. The gate's intent
("don't double-emit shadows of authoritative package-db claims") is correct for the version-string
scanner, the linkage aggregator, and the ELF-note reader — those genuinely produce shadows of
claimed binaries. But cargo-auditable's per-crate emissions are NOT shadows of the file-level binary
identity — they're the transitive build closure of crates statically linked into the binary, which
is a separate tier of truth from the package-db claim. The gate was conceptually wrong for this one
block from the start.

**Fix**: remove the `skip_secondary_evidence` gate around lines 700-708 only. All other
`skip_secondary_evidence`-gated blocks at lines 502, 530, 561 stay gated (the comments documenting
the per-block intent are preserved verbatim).

**Audit-image impact**: total `pkg:cargo` components 58 → 1,116 (+1,058). Unique
`(name, version)` cargo PURLs: 58 → 582. mikebom now exceeds syft's 493 unique cargo count by 89
components (mikebom's superset comes from the Cargo.lock source-tier reader catching the Rust
stdlib's internal `alloc@0.0.0`, `alloctests@0.0.0`, etc. that syft's binary cataloger doesn't
enumerate).

**Byte-identity preserved** across the 33 alpha.48 goldens. For images without `cargo-auditable`-
built binaries (the common case), the fix is a no-op.

**Follow-ups still tracked** for separate PRs per the milestone-130 plan's recommended cadence:

- US2 — Maven nested-JAR recursion (~300 packages, ~400 LOC). See
  `specs/130-binary-tier-completion/spec.md` US2.
- US3 — PE/CLR managed-assembly metadata (~450 packages, ~1000 LOC ECMA-335 hand-roll). See US3.

### `.deps.json` reader for .NET container images (milestone 129 US1A)

Fills the largest single ecosystem gap surfaced by a side-by-side audit against syft on a polyglot Wolfi-based dev-tooling container image (`767397973649.dkr.ecr.us-east-1.amazonaws.com/remediation-planner:latest`): pre-milestone-129, mikebom emitted **zero** `pkg:nuget` PURLs on .NET-bearing images because the existing milestone-106 NuGet reader is source-tier (`.csproj`/`Directory.Packages.props`/`packages.lock.json`) and production container images ship only the compiled output. This milestone adds a `.deps.json` sidecar reader to `mikebom-cli/src/scan_fs/package_db/nuget/deps_json.rs` that walks the rootfs for `*.deps.json` files (emitted by `dotnet publish` and shipped throughout the .NET SDK + runtime store layouts) and emits one `pkg:nuget/<name>@<version>` per `libraries[]` entry with `type: "package"`. On the audit image, this lifts the unique NuGet count from 0 to **184** (vs syft's 635 unique; the residual ~451 packages live in `.dll` PE/CLR metadata which a follow-up milestone will address).

**Output shape**:

- Per-component `mikebom:sbom-tier = "image"` annotation (distinguishes from source-tier NuGet hits from milestone 106).
- Per-component `mikebom:source-mechanism = "dotnet-deps-json"` annotation.
- Per-document `mikebom:dotnet-runtime-target` annotation carrying the `.NETCoreApp,Version=v8.0`-style runtime target name when `runtimeTarget.name` is set in the `.deps.json`.
- Per-component `mikebom:image-presence = "declared-not-installed"` annotation when the `.deps.json` entry declares a `path` field pointing at an assembly file that isn't present in the rootfs (best-effort probe checks the `.deps.json`'s parent dir, grandparent dir, and the `/usr/share/dotnet/` runtime-store root).

**Behavior**:

- `type: "project"` entries (the application's own first-party assembly) are silently skipped — these are not third-party NuGet dependencies (FR-009).
- `type: "referenceassembly"` entries are silently skipped — these are compile-time-only reference assemblies, not runtime dependencies.
- Malformed JSON, malformed `name/version` library keys, and unknown `type` values emit a single `warn`-level log and skip the affected entry; the surrounding scan does not abort (FR-006 fail-closed-but-keep-going).
- Reader honors `--offline` (no network, no subprocess) and `--exclude-path` (routes via `safe_walk`).
- Existing milestone-105 dedup pipeline collapses cross-mechanism duplicates; collisions between source-tier (.csproj) and image-tier (.deps.json) emit ONE component with both source-mechanism strings in `mikebom:also-detected-via`.

**Byte-identity preserved** across the 33 committed alpha.48 goldens. For images without `.deps.json` files (pure-Go, pure-Python, pure-Rust trees), the reader is a no-op and the emitted SBOM is byte-identical to alpha.48.

**Follow-ups tracked separately**:

- US1B (PE/CLR managed-assembly metadata reader) — covers the ~451 packages in `Microsoft.AspNetCore.*.dll` / `DotNetWatchTasks.dll` / etc. that don't have neighboring `.deps.json` entries. Bounded ECMA-335 §II.22 hand-roll on `object` 0.36's PE primitives. Tracked for a follow-up milestone.
- US2 cargo-auditable debugging — mikebom's existing milestone-029 `cargo_auditable.rs` reader emits 0 components on the audit image's `/usr/bin/uv` and `/usr/bin/uvx`; the 58 `pkg:cargo` components currently emitted come from a `Cargo.lock` source-tier hit, not the `.dep-v0` ELF section. Bug investigation tracked for a follow-up.
- US3 maven nested-JAR recursion — tracked for a follow-up milestone.

### Deeper Yocto/OpenEmbedded SBOM coverage (milestone 128)

Extends the milestone-107 `.bb` recipe reader from "name + version only" to a full source-tier identity layer for OpenEmbedded recipes. Inspired by an audit of three balena meta-layers (`balena-os/meta-balena`, `balena-os/balena-raspberrypi`, `balena-os/balena-generic`), where the alpha.48 reader emitted just `pkg:bitbake/<name>@<version>` — enough to land the recipe in the SBOM but not enough for SCA, OSV vulnerability matching, or downstream license review.

**PURL migration (FR-001, breaking for SBOM consumers indexing on the type segment):** recipe PURLs move from `pkg:bitbake/<name>?layer=<collection>@<version>` (alpha.48) to `pkg:generic/<name>@<version>?openembedded=true&layer=<collection>` (milestone 128). Aligns with upstream Yocto-emitted CDX/SPDX and the broader `pkg:generic` convention; the `openembedded=true` qualifier carries the ecosystem signal that `bitbake` previously held in the type slot. SBOM-byte-identity preserved across the 33 committed goldens (no balena/yocto fixture in the golden set).

**Five new user stories, all standards-native where possible per Principle V:**

1. **US1 — License extraction (FR-001 + FR-014 + FR-019)** — `LICENSE` field parsed verbatim from the recipe body, translated from BitBake `&`/`|` syntax to SPDX `AND`/`OR`, canonicalized via `SpdxExpression::try_canonical`, and emitted into CDX `licenses[].license.id|expression` + SPDX 2.3 `Package.licenseDeclared` + SPDX 3 `software_declaredLicense`. Compound expressions split across multiple `licenses[]` entries in CDX per spec. `LICENSE = "CLOSED"` does NOT poison the license field; the recipe carries `mikebom:yocto-license-closed = "true"` (C80) instead.
2. **US2 — Source identity + OSV-direct-match (FR-002 + FR-002a + FR-015)** — `SRC_URI` + `SRCREV` parsed. When `SRC_URI` is a recognizable git host (`github.com`, `gitlab.com`, `bitbucket.org`, `codeberg.org`), the recipe emits an OSV-queryable host-typed PURL `pkg:github/<owner>/<repo>@<srcrev>` (or `pkg:gitlab/...`, etc.) — directly queryable against OSV without intermediate joins. Recipe identity preserved on the same component via new `mikebom:yocto-recipe-name` (C85) + `mikebom:yocto-recipe-version` (C86) annotations. `SRC_URI = "git://..."` with `SRCREV = "${AUTOREV}"` or version `"git"` rejected as anti-patterns (FR-018) — the component falls back to manifest-derived version with a `mikebom:yocto-version-derived` log surface.
3. **US3 — Layer attribution (FR-003 + FR-004 + FR-016)** — each recipe gets attributed to its containing layer via a `conf/layer.conf` walk + nearest-ancestor heuristic. The layer collection name (e.g. `meta-balena`), version, and series compatibility flow into `mikebom:yocto-layer` (C73) + `mikebom:yocto-layer-version` (C74) + `mikebom:yocto-layer-series` (C75) annotations. Layer-roots are synthesized as their own main-module-tagged components — milestone-127's `is_workspace_root` heuristic elects the right one as the BOM subject for scans rooted at a layer dir.
4. **US4 — `.bbappend` provenance (FR-008)** — `.bbappend` files walked across all detected layers; matched against base recipes by name + version (with `%` wildcard). Matched appends surface as `mikebom:bbappend-applied` (C76) annotations on the base recipe component, naming every applied append path. Orphaned `.bbappend` files (no matching base recipe in any walked layer) DO NOT synthesize phantom components per Constitution VIII completeness; they instead emit a `tracing::warn!` log naming each orphan path.
5. **US5 — DEPENDS edges (FR-005 + FR-006 + FR-007)** — `DEPENDS` (build-time) and `RDEPENDS` (runtime) parsed and resolved against the in-scan recipe set. Resolved entries flow into the CDX `dependencies[]` graph + SPDX `relationships[]` (BUILD_DEPENDENCY_OF / DEPENDENCY_OF). Unresolved entries surface in new `mikebom:depends-unresolved` (C77) + `mikebom:rdepends-unresolved` (C78) annotations so auditors can see what's in the recipe vs what's in the scan.

**CPE candidates emission (FR-017 + FR-019):** the existing milestone-097 `mikebom:cpe-candidates` annotation channel gets a Yocto-specific normalization table mapping recipe names to canonical CPE product names (e.g. `linux-yocto` → `linux_kernel`, `gcc-source-13.2.0` → `gcc`). The table sources from openembedded-core's `cpe-update-helper.inc` upstream mapping (~115 entries embedded in `mikebom-cli/src/scan_fs/package_db/yocto/cpe_name_map.rs`). Recipes with no normalized mapping fall back to recipe-name verbatim — never poisoning the array with low-confidence guesses.

**Recipe body fidelity (FR-009 + FR-010 + FR-011 + FR-012 + FR-013):** the new recipe-body parser handles 8 assignment operators (`=`, `?=`, `??=`, `:=`, `+=`, `=+`, `.=`, `=.`), BitBake `:append`/`:prepend`/`:remove` override syntax, machine-specific overrides (`SRCREV_qemuarm`), and `inherit`/`require`/`include` chains up to 8 levels deep with cycle detection. `${PN}` and `${PV}` expand to the recipe's own name + version; other unexpanded `${VAR}` references surface in a `mikebom:yocto-unexpanded-vars` (C79) annotation so reviewers can see what evaluation BitBake itself would have completed but mikebom (deliberately) does not.

**17 new annotation keys catalogued (C70..C86) with full Principle V audit narratives** in `docs/reference/sbom-format-mapping.md`. Every key emits SymmetricEqual across CDX + SPDX 2.3 + SPDX 3 — same parity guarantee the rest of the `mikebom:*` family enjoys. Catalog row test (`extractors_table_is_sorted_by_row_id` + `every_catalog_row_has_an_extractor`) green.

**13 new integration tests** at `mikebom-cli/tests/yocto_recipe_enrich_us{1,2,3+5+cpe,4}.rs` against 6 vendored Yocto fixture trees (`mikebom-cli/tests/fixtures/yocto_recipe_enrich/`). The fixtures stay in the main repo (not the sibling fixture-cache repo) — they're synthetic-shape minimal trees, not real-world projects, matching the milestone-090 "stay-set" rule. balena_smoke integration test against MIKEBOM_FIXTURES_DIR + SC-007 sbomqs verification deferred to a follow-up milestone.

### Smarter root component selection on polyglot + multi-module Go workspace scans (milestone 127, closes #366 + #367)

Replaces the previously inline metadata.component / SPDX 2.3 documentDescribes / SPDX 3 rootElement priority ladder in each of the three format emitters with a single shared `generate::root_selector::select_root` ladder. The new ladder applies these tiebreakers, in order, when multiple main-module-tagged components exist:

1. **Operator override wins** — `--root-name` / `--root-version` / `--root-purl-type` / `--no-root-purl` per milestone 077 + #358 unchanged.
2. **Single-main-module fast path** — exactly one main-module exists → use it. Byte-identical to alpha.48 output across all 33 committed goldens (cdx/spdx2.3/spdx3 byte-identity regression: 33/33 pass).
3. **Repo-root tiebreaker (FR-002)** — exactly one main-module's manifest file sits at the scan's `--path` root → use it. Confidence 0.95.
4. **Ecosystem-priority tiebreaker (FR-003)** — multiple main-modules at the repo root → fixed priority order `[golang, cargo, maven, npm, pip, gem, generic]` picks one. Confidence 0.70.
5. **Longest-common-prefix tiebreaker (FR-004)** — no main-module at the repo root → if exactly one main-module's manifest path equals the LCP of all main-module manifest paths, pick it. Confidence 0.80.
6. **Maven `scan_target_coord` fallback** — as today, confidence 0.60.
7. **`pkg:generic/<target>@0.0.0` placeholder** — as today, confidence 0.30.

Whenever any tiebreaker fires AND the auto-pick falls through past at least one detected main-module, the emitted SBOM gains a document-scope `mikebom:root-selection-heuristic` annotation carrying `{"heuristic": <name>, "confidence": <float>}` (C-row C69 in `docs/reference/sbom-format-mapping.md` — full Principle V native-field audit attached). The same condition surfaces a `tracing::warn!` log at scan-end naming the picked subject AND every loser main-module's PURL, recommending the operator pass `--root-name`/`--root-purl-type` for deterministic control.

**Behavior changes worth flagging:**

- **`argo-workflows`-shape repos** (polyglot Go + Maven + npm where the Go module sits at the repo root) — root identity moves from `pkg:maven/io.argoproj.workflow/argo-client-java-tests@0.0.0-VERSION` (alpha.48) to `pkg:golang/github.com/argoproj/argo-workflows/v3@v3.5.5` (post-127). Closes #366.
- **`opentelemetry-collector`-shape repos** (multi-module Go workspace with 50+ nested `go.mod` files, one at the repo root) — root identity moves from an alphabetic-leaf sub-module to the repo-root module (`pkg:golang/go.opentelemetry.io/collector@v0.105.0`). Closes #367.
- **`--bind-to-source` operator scripts targeting the old (wrong) subject** on the two affected project shapes above need updating; the binding follows the corrected root.

**Internal-only annotation kept off the wire:** the new `mikebom:is-workspace-root` boolean drives the tiebreakers but is filtered out at every per-format `extra_annotations` iteration site (`generate/cyclonedx/builder.rs`, `generate/spdx/annotations.rs`, `generate/spdx/v3_annotations.rs`) so it never reaches serialized SBOM output. This is the byte-identity preservation lever — without it the 33 alpha.48 goldens would churn at every emission.

**FR-012 Maven `scan_target_coord` dedup:** when the Maven `pom.xml` reader (milestone 070) emits a main-module whose PURL matches the JAR walker's `scan_target_coord`, the duplicate signal gets suppressed at the source (in `scan_fs/mod.rs::scan_path` before the metadata.component ladder runs). Pure-Java repos with one `pom.xml` at the root see one coord, not two, AND the FR-007 warning surface stays clean for them.

## [0.1.0-alpha.48] — 2026-06-16

Milestones 114 (`safe_walk` refactor), 115 + 117 (walker-audit CI gate), 116 (cross-tier `produces-binaries` binder), 118 (`--exclude-path` polish), 119 (operator supplement file via `--supplement-cdx`), and 122 (Swift Package Manager + Kotlin DSL Gradle ecosystem readers + KMP polyglot regression) all ship in this release. Plus producer-side root-PURL control (`--root-purl-type` / `--no-root-purl`), the deprecated `--include-dev` shim removal, and the CI release-tag-push gap closure.

**Default behavior changes (non-byte-identical):**

- Every Go source-tree scan that previously needed the milestone-113 `--exclude-path` workaround to bypass exotic `gradle.lockfile` discovery now naturally walks via `safe_walk` (milestone 114) with identical output bytes — but readers across cargo / maven / gem / pip / npm / gradle / nuget / yocto / Go source / Go binary now go through one centralized helper.
- Cargo / npm / pip / gem / maven / Go main-module components now carry the new `mikebom:produces-binaries` (C64) annotation listing the canonical binary names the ecosystem manifest declares (milestone 116). The cross-tier `--bind-to-source` binder uses this to auto-alias image-tier `pkg:generic/<name>` components to their source-tier ecosystem PURL — operators using `mikebom verify-binding` get more binding coverage out of the box.
- `mikebom sbom scan --supplement-cdx <PATH>` now accepts a hand-authored CDX 1.6 supplement file declaring ground truth the scanner cannot observe (SaaS deps, vendored libraries without manifests, license / supplier / copyright metadata). When the flag is in effect, the emitted SBOM carries `mikebom:source-tier = "declared"` on solo entries + a document-scope `mikebom:supplement-cdx = "<path>@sha256:<hex>"` provenance annotation + per-component `mikebom:assertion-conflict` annotations on collisions. Three new annotation keys (C65 / C66 / C67) with full Principle V audit narratives in `docs/reference/sbom-format-mapping.md`.
- Two new ecosystem readers: Swift Package Manager (`Package.resolved` lockfiles, `pkg:swift/<host>/<ns>/<name>@<version>` PURLs) and Kotlin DSL Gradle (`build.gradle.kts` + `libs.versions.toml` + `settings.gradle.kts`, `pkg:maven/...` PURLs). KMP multi-target source-set provenance rides the new `mikebom:kmp-source-set` (C68) annotation. Off by default in the sense that scans of non-Swift / non-Kotlin trees produce identical output, but Swift / Android / KMP scans previously produced empty SBOMs and now produce real ones.

All other ecosystems see byte-identical SBOMs by default — milestone-119's `--supplement-cdx` is opt-in, milestone-122's readers contribute zero components when their ecosystems aren't present, and PR #358's root-PURL flags are opt-in.

### Internal cleanup — `safe_walk` migration (milestone 114, #341)

Every ecosystem-reader filesystem walker migrated to a shared `scan_fs::walk::safe_walk` helper. Pre-114, 15 hand-rolled `fn walk_*` recursions across `mikebom-cli/src/scan_fs/` each carried their own canonicalize-keyed visited-set + depth-bound + milestone-113 directory-exclusion + skip-cause logging code. Post-114 a single helper centralizes all four invariants; each reader configures a `WalkConfig` + visit callback. Four documented known exceptions (`walker.rs` whole-FS deep-hash, `npm/walk.rs` `@scope`-aware, `cmake_observer.rs` stop-at-match descent, `maven_sidecar.rs` lstat-style M2 cache walker) stay hand-rolled with explicit one-sentence reasons in the helper module's comment block. No user-visible behavior change — byte-identical SBOMs across all 33 committed goldens. The audit pattern `grep -rEn 'fn walk[_(]' mikebom-cli/src/scan_fs/` is the durability mechanism documented in `docs/design-notes.md`.

### CI walker-audit gate (milestone 115 #344, milestone 117 #349)

Permanent CI guard against new ad-hoc walkers reappearing. A new `walker-audit` job grep-walks `mikebom-cli/src/scan_fs/` for `fn walk*` and `fn .*walk` declarations, diffs against `walk.audit-allowlist.txt`, and fails CI on any net-new unrecognized walker. Milestone 117 (#349) tightens the allowlist comparison so it ignores line-number drift — entries are compared by `file:content` rather than `file:line:content` so unrelated edits to a file don't churn the allowlist.

### Cross-tier `produces-binaries` binder (milestone 116, #345 + #346 + #348)

New `mikebom:produces-binaries` (C64) annotation on main-module components carries the canonical binary names the ecosystem manifest declares. The cross-tier `--bind-to-source` binder consumes the annotation to auto-alias image-tier `pkg:generic/<name>` components to their source-tier ecosystem PURL even when the operator doesn't pass an explicit `--pkg-alias` flag. Per-ecosystem extractors: Cargo (`[[bin]]` + `src/main.rs` + `src/bin/*.rs`) via #345, npm (`bin` field both shapes) + pip (`[project.scripts]` + setup.cfg `console_scripts`/`gui_scripts`) + gem (`executables`) + maven (shade-/jar-plugin `<finalName>`) via #346, Go (filesystem walk for `package main` directories) via #348. Alias provenance is recorded via the new `alias_source` field on the milestone-111 `SourceDocumentBinding` envelope so auditors can distinguish operator-supplied aliases from automatic-from-produces-binaries.

### `--exclude-path` polish (milestone 118, closes #343, #350)

Six per-ecosystem regression tests + an opt-in perf benchmark + a scan-end `tracing::info!` summary. The scan summary now surfaces `excluded_entries=N excluded_literals=N excluded_patterns=N suppressed_dirs=N` when at least one exclusion entry is in effect. Operators can grep stderr for the summary instead of paging through `RUST_LOG=debug` output. The perf benchmark gates exclusion overhead at ≤1.10× the no-flag baseline on the kusari-cli fixture.

### Operator-supplied supplement file via `--supplement-cdx` (milestone 119, closes #326, #351 + #352 + #353)

`mikebom sbom scan --supplement-cdx <PATH>` accepts a hand-authored CDX 1.6 (1.4 / 1.5 also accepted) JSON document declaring ground truth the scanner cannot observe — SaaS dependencies, vendored libraries without manifests, license / supplier / copyright metadata on otherwise-known components. The merge runs once per scan, before emission, so every output format sees the same combined view.

- **Solo entries** (PURL not in scanner output) become new components tagged `mikebom:source-tier = "declared"`.
- **Collisions** resolve via the hard/soft split: scanner wins on bytes-derived facts (hashes, cpe, canonical purl, version, binary_role); developer wins on metadata (licenses, supplier, copyright, name, description, externalReferences — all types). Catch-all default: scanner wins (FR-015 safety property — developer cannot suppress scanner detection of bytes-evident content).
- Every disagreement records a `mikebom:assertion-conflict` annotation as a JSON-encoded array of conflict records per the C1-committed `BTreeMap<String, serde_json::Value::Array>` storage convention.
- Document-scope `mikebom:supplement-cdx = "<path>@sha256:<hex>"` provenance.
- Parse / I/O / schema failures fail closed before any walker begins per FR-002.

Phase-2 (#352) extended this with SPDX 2.3 + SPDX 3 service projection (CDX `services[]` entries surface as `packages[]` with `mikebom:component-role = "saas-service"` annotation in SPDX), C68/C67 parity-catalog rows, and the document-scope `mikebom:supplement-cdx` annotation on SPDX outputs. Follow-up (#353) propagates supplement-declared `licenses[]` overrides onto Cargo's `metadata.component` main-module path via a typed `Vec<SpdxExpression>` projection so the operator's declared license appears in every emission format uniformly.

Three new annotation keys with full Principle V audit narratives: C65 (`mikebom:source-tier = "declared"` value extension), C66 (`mikebom:supplement-cdx` envelope-level provenance), C67 (`mikebom:assertion-conflict` per-component JSON-array). Hand-rolled structural validator (no `jsonschema` runtime dep).

### Swift Package Manager + Kotlin DSL Gradle + KMP polyglot readers (milestone 122, #354 + #356 + #357)

Two new ecosystem readers shipped under one coordinated milestone:

- **Swift Package Manager** (#354) — parses `Package.resolved` lockfiles (v1 / v2 / v3 schema dispatch), emits `pkg:swift/<host>/<namespace>/<name>@<version>` PURLs per the [purl-spec swift type](https://github.com/package-url/purl-spec/blob/main/PURL-TYPES.rst#swift). Commit-pinned mode (no `state.version`) uses the FULL 40-char revision SHA as the version segment. `Package.swift` is detected (signals SwiftPM project root) but never parsed for content. Deep-namespace URLs (GitLab subgroups) are warn-and-dropped — purl-spec swift type allows single-segment namespaces only.
- **Kotlin DSL Gradle** (#356) — regex-extracts dep declarations from `build.gradle.kts` (the Android Studio / IntelliJ default since 2023), resolves `libs.<alias>` against `gradle/libs.versions.toml`, emits `pkg:maven/<group>/<name>@<version>` per the existing milestone-106 lane. Multi-module workspaces synthesize a `pkg:generic/<rootProject.name>@0.0.0` workspace-root per FR-007. KMP source-set provenance via the new `mikebom:kmp-source-set` (C68) annotation, JSON-array storage. Components are design-tier (`mikebom:sbom-tier = "design"`) gated by `--include-declared-deps`. Complements (not replaces) the existing milestone-106 `gradle.lockfile` reader.
- **KMP polyglot regression suite** (#357) — three-module monorepo fixture (`androidApp/` + `shared/` with KMP source-sets + `iosApp/` SwiftPM) verifies both ecosystems coexist in one SBOM without cross-ecosystem collapse.

### Producer-side root-PURL control (#358)

Two new opt-in flags on `mikebom sbom scan` that give operators producer-side control of the root component's PURL across all three output formats:

- **`--root-purl-type <TYPE>`** — overrides the type segment of the auto-derived root PURL. Today, when `--root-name` is supplied, mikebom hardcodes `pkg:generic/<name>@<version>`. The new flag replaces that hardcoded `generic` with an operator-supplied type token (e.g., `--root-purl-type=golang --root-name=github.com/example/svc` produces `pkg:golang/github.com%2Fexample%2Fsvc@<version>`). Validated at parse time against the purl-spec type charset `^[a-z][a-z0-9.+-]*$`. REQUIRES `--root-name`. Mutually exclusive with `--no-root-purl`.
- **`--no-root-purl`** — omits the root component's PURL entirely. CDX: `metadata.component.purl` absent. SPDX 2.3: no `purl` `externalRef`. SPDX 3: no `software_packageUrl` AND no `externalIdentifier[packageUrl]`. REQUIRES `--root-name`. Mutually exclusive with `--root-purl-type`.

Default behavior unchanged — both flags are opt-in. Extends the existing milestone-077 `RootComponentOverride` surface. Follow-up GitHub issue #359 tracks a possible future simplification to a single `--root-purl <VALUE>` flag.

### Removed (BREAKING)

- **`--include-dev` CLI flag removed** (#340, closes #101). Deprecated since milestone 052/part-3 (alpha.20, PR #100) when the scan default flipped to emit ALL lifecycle scopes natively tagged. The post-052 shim only logged a deprecation warning and was otherwise a no-op; the soak window has elapsed (>20 weeks since the warning landed). Operators wanting the pre-052 strict deployed-runtime view should use `--exclude-scope dev,build,test`. Shell scripts and CI configs still passing `--include-dev` will now fail with clap's standard "unexpected argument" message — the operator-visible fix is a one-token swap.

### CI plumbing (#338, #339)

- Closed the release-bump-merged-but-tag-not-pushed gap (#171). When a release-bump PR merges, the new `auto-tag-release.yml` workflow extracts the version from `Cargo.toml`, creates the matching annotated tag on the merge commit, pushes it via an explicit `x-access-token` URL, and explicitly dispatches the `release.yml` workflow against the new tag — closing the gap where `GITHUB_TOKEN`-pushed tags don't trigger downstream workflows by design.
- Bumped `actions/checkout` from 6.0.2 → 6.0.3 (closes #319).

### Release deltas

- `Cargo.toml`: workspace version `0.1.0-alpha.47` → `0.1.0-alpha.48`.
- `Cargo.lock`: regenerated via `cargo +stable build`.
- `mikebom-cli/tests/fixtures/golden/`: 33 byte-identity goldens regenerated (11 CDX + 11 SPDX 2.3 + 11 SPDX 3). Deltas are version-bump-only — the mikebom-self-component `version` field bumps from alpha.47 → alpha.48 across CDX + SPDX 2.3 + SPDX 3, and the SHA-derived SPDX 3 document IDs shift accordingly per milestone 011's deterministic-ID scheme.

## [0.1.0-alpha.47] — 2026-06-12

Milestones 110 Phase 5-Slim (multi-source corpus configuration + fetch), 111 (cross-tier PURL aliasing), 112 (Go build-inclusion clarity via `go mod why -m -vendor`), and 113 (user-supplied directory exclusion) ship in this release. Two Go correctness fixes also land: test-scope closure propagation, and skipping `testdata/` + `_`-prefixed directories per the Go tool convention.

**Default behavior changes (non-byte-identical for Go scans):**

- Go components discovered only via `go.sum` fallback now carry `mikebom:build-inclusion: unknown` always-on; when a `go` toolchain is on PATH, `go mod why -m -vendor` runs by default to classify those modules into `not-needed` (CDX `scope: "excluded"`) / `test-only` (lifecycle scope test) / `needed`. Opt-out via `--no-go-mod-why` or `MIKEBOM_NO_GO_MOD_WHY=1` (milestone 112).
- Go test scope now propagates correctly through the test-only module closure — modules transitively reachable only from `_test.go` imports now carry `mikebom:lifecycle-scope: test` plus `mikebom:lifecycle-scope-derivation: test-only-closure`.
- Go walkers now skip `testdata/` directories and `_`-prefixed directories per `go help packages` ("ignored by the go tool"). Fixes the inverted-dependency-edge bug class where a fixture's `go.mod` was emitted as a real workspace.

All other ecosystems see byte-identical SBOMs by default — milestone 113's `--exclude-path` is off by default and milestone 111's `--pkg-alias` is opt-in.

### Pluggable fingerprint corpus v2 — multi-source configuration (#322)

First slice of milestone 110 Phase 5-Slim. Adds the `MIKEBOM_FINGERPRINTS_CORPUS_SOURCES` env-var parser + `CorpusSource` deduplication + per-source cache directory derivation. The default public corpus source is still the sole effective source when the env var is unset; operators wanting to layer in private corpus sources can now do so by listing multiple URLs.

### Pluggable fingerprint corpus v2 — multi-source fetch + wire-up (#325)

Second + third slice. Wires the multi-source configuration into the runtime fetch path: each source is fetched in parallel into its own per-source cache dir (`~/.cache/mikebom/fingerprints/<source-id>/<sha>/`), with a 24-hour TTL via the `last_used.touch` sidecar pattern. Matcher loads records from every successful source and de-duplicates by record `primary_purl + indicator content`.

### Cross-tier PURL aliasing — foundational types + envelope (#327)

First PR of milestone 111. Introduces the `mikebom:source-document-binding-alias` envelope extension on the existing milestone-072 binding envelope. The `PurlAlias { from: Purl, to: Purl, reason: Option<String> }` newtype carries an operator-declared "this PURL alias-equals that PURL" assertion so downstream consumers can collapse cross-tier same-component shadows even when the source-tier and build-tier readers emit slightly different canonical PURLs (e.g. `pkg:github/madler/zlib@v1.3.1` ↔ `pkg:generic/zlib@1.3.1`).

### Cross-tier PURL aliasing — `--pkg-alias` flag + env var (#330)

Second PR of milestone 111. Wires `--pkg-alias <FROM=TO>` (repeatable) + `MIKEBOM_PKG_ALIAS` (comma-separated) into the scan binding path. The alias map is propagated into the milestone-072 binding-emit pipeline so the envelope is extended at SBOM emission time. Off by default; byte-identical output when not supplied.

### Cross-tier PURL aliasing — US1 end-to-end integration tests + qualifier-aware parser (#331)

Third PR of milestone 111. Adds the qualifier-aware alias parser (so `pkg:github/foo/bar@v1?subpath=…` is properly canonicalized before equality compare) + 6 end-to-end integration tests covering CLI parsing, env var precedence, envelope emission across CDX/SPDX 2.3/SPDX 3, and round-trip via `verify-binding`.

### Scan performance — drop build intermediates before reading them (#329)

Bug fix: the milestone-098 ELF compiler-stamps extractor was reading `.o`/`.a`/`.rlib` files looking for `.comment` sections, dominating scan wall-time on Rust `target/` trees with hundreds of thousands of intermediate object files. Added a fast-path skip on those four extensions in `go_binary.rs`'s recursive walker before the full-file probe. On a `target/`-heavy fixture, this cuts scan time by ~95%. No behavioral change to emission.

### Go test-scope closure propagation (#332)

Fix for a class of false-negative test-scope tagging. Pre-fix, a module reachable only from test-only roots through transitive `requires` was tagged `lifecycle-scope: prod` because the import walk hit it from a non-`_test.go` file (transitively, via a module that was itself only test-needed). Post-fix, mikebom propagates test scope through the test-only closure: a module is `lifecycle-scope: test` iff every path from any `_test.go` import root in any main module reaches it, and no path from a non-test import does. The new derivation gets `mikebom:lifecycle-scope-derivation: test-only-closure`.

### Go walker — skip `testdata/` + `_`-prefixed dirs per Go tool convention (#335, closes part of #334)

`go help packages` is explicit: directories named `testdata`, plus any directory whose name begins with `.` or `_`, are ignored by the Go tool. mikebom's Go source walker and Go binary walker now match exactly. Fixes the inverted-dependency-edge bug class where a Go test fixture at `pkg/sbomgen/testdata/gofixture/go.mod` (whose go.mod `required` the parent module as a fixture scenario) was emitted as a real main-module with a synthetic edge back to the parent — producing the chain `app → test-fixture-sbomgen → app` in consumer tooling.

### User-supplied directory exclusion — `--exclude-path` flag (#336, milestone 113, closes #334)

Generic directory-exclusion flag for ecosystems without a documented language convention. Repeatable on the CLI (`--exclude-path tests/fixtures --exclude-path '**/examples'`), env-var counterpart `MIKEBOM_EXCLUDE_PATH` accepting platform-path-list-separated entries, and combines by union. Entries containing `*`/`?`/`[` are `globset` patterns matching directory paths at arbitrary depth; other entries are literal paths anchored at the scan root.

Honored across every ecosystem walker (cargo, maven, gem, pip, npm, gradle, nuget, yocto, golang source + binary) by threading `&ExclusionSet` through each walker's recursive descent decision. Additive on top of the built-in skip set — operators can't use it to re-enable scanning of `vendor/`/`node_modules/`/etc., only to add their own entries.

Off by default: zero entries produce byte-identical SBOMs to a pre-feature build. Non-empty entries trigger the Principle-X transparency annotation `mikebom:exclude-path` at envelope level across CDX 1.6 / SPDX 2.3 / SPDX 3.0.1 (new parity catalog row C63), so consumers can detect the narrowing without access to the original scan invocation. Malformed patterns abort before any walker begins with a single error line naming the offending entry verbatim.

One new direct Cargo dep: `globset = "0.4"` (pure Rust; all transitives — `regex`, `regex-syntax`, `regex-automata`, `aho-corasick` — already in the workspace closure).

### Validation across the release

- Workspace-wide unit + integration tests: 1900+ tests pass on `./scripts/pre-pr.sh` (milestone 110 Phase 5-Slim + milestone 111 + milestone 112 + milestone 113 land cleanly).
- 33 byte-identity goldens regenerated for the version bump (alpha.46 → alpha.47). All deltas are version-bump-only — the milestone-112 build-inclusion annotations + test-closure-propagation derivation were already captured in their respective feature PRs' golden regenerations, so this release carries no emission-shape changes to the committed goldens beyond the mikebom-self-component `version` field bump (and the SHA-derived SPDX document IDs that shift accordingly per milestone 011's deterministic-ID scheme).
- One new direct Cargo dependency between alpha.46 and alpha.47: `globset = "0.4"` (pure Rust, all transitives already in the lockfile).

## [0.1.0-alpha.46] — 2026-06-08

Milestone 110 Phase 4 surface complete: the pluggable fingerprint corpus v2 capability lands end-to-end. mikebom now ships a multi-indicator corpus record schema (symbols + version strings + Build-IDs + ABI markers + ecosystem-alias PURLs + CPE candidates) AND the matcher + loader + production wiring that consumes it. Third-party corpus authors can target `docs/reference/corpus-record-v2.schema.json` today and have mikebom load, fuse, and emit versioned PURLs against scanned binaries.

**Default behavior unchanged.** Operators who don't opt into `--fingerprints-corpus` see byte-identical SBOMs to alpha.45 across all 33 byte-identity goldens. Operators who DO opt in continue to see the milestone-108 behavior; v2 records only emit when authored AND present in the fingerprint cache — neither condition is met by today's public milestone-108 corpus, so the v2 path is dormant for typical operators until corpus authors begin publishing v2 records.

### Pluggable fingerprint corpus v2 — foundational types (#313)

Lays the type system + public JSON Schema for v2 corpus records. New types in `mikebom-cli/src/scan_fs/binary/fingerprints/`:

- `confidence.rs` — `Confidence` newtype + `FusedConfidence` enum (`High` / `Medium` only — no `Low` variant; encodes the spec-clarified below-medium-suppression rule at the type level). `from_pct_in_range_const::<PCT>()` const constructor preserves constitution Principle IV at fixed-baseline call sites without `.unwrap()`.
- `record.rs` extended — `CorpusRecordV2`, `IndicatorKind` (closed enum), `IndicatorSpec` (tagged enum: `SymbolSet` / `RodataLiteral` / `ExactHash`), `Provenance`, `CollisionSpec`, `CorpusError` (thiserror). v1 `FingerprintRecord` unchanged for backward compat. `#[serde(deny_unknown_fields)]` throughout.
- `source_config.rs` — `CorpusSource` + `CorpusSourceId` (16-char BASE32(sha256(url)) or `"public-milestone-108"` sentinel for the default).
- `self_identity.rs` + `matcher.rs` — stubs declaring the Phase 4/6 surface area (the matcher stub becomes real in #315; the resolver ladder ships in a follow-on milestone).

User-visible behaviour change for opt-in scans: every fingerprint-derived component now carries a new `mikebom:fingerprint-confidence` annotation whose value is the numeric fused-confidence score (formatted `"X.XX"` — e.g., `"0.70"` / `"0.85"` / `"0.99"`). Distinct from the existing C16 `mikebom:confidence` enum-string carrier (value=`"heuristic"`) so no collision. Spec FR-017 revised during implementation to emit numeric (lossless, matches CDX-native `evidence.identity.confidence` semantics) rather than the originally-planned bucket-name.

Public artifacts:
- `docs/reference/corpus-record-v2.schema.json` — JSON Schema Draft 2020-12 contract for third-party corpus authors.
- `mikebom-cli/contracts/corpus-record-v2.schema.json` — test-local copy.
- `specs/110-pluggable-corpus-v2/` — full spec + plan + research + data-model + contracts + quickstart + tasks artifacts (~125 KB of design).

Validation: 28 new unit tests + 6 new integration tests cover the type system + JSON-Schema conformance + the new annotation's presence-on-opt-in / absence-on-default-scan contract.

### Parity row C59 for `mikebom:fingerprint-confidence` (#314)

Adds the catalog row for the new annotation in `docs/reference/sbom-format-mapping.md` + the three parity-extractor entries (CDX 1.6 / SPDX 2.3 / SPDX 3.0.1). Principle-V audit clause documents the distinction from the existing C16 carrier + the Phase-4 forward-pointer for additionally populating CDX-native `evidence.identity[].confidence`. The existing `sbom_format_mapping_coverage` + `mapping_doc_bidirectional` CI gates continue to enforce 100% row coverage.

### v2 matcher + fusion algorithm (#315)

Replaces the Phase-2 `matcher.rs` stub with the actual multi-indicator matching + confidence-fusion logic. Pure additive — production scan path continues through milestone-108's matcher until #317 wires the new pipeline in.

- `BinaryArtifact` — matcher-internal synthesis struct carrying the extracted-indicator inputs (`exported_symbols`, `rodata_strings`, `build_id`, `macho_uuid`, `pe_pdb`).
- `MatchResult` — emission shape with `confidence_score` (numeric) alongside the coarser `FusedConfidence` bucket so downstream annotation emission can populate the numeric value losslessly.
- "max + bump" fusion algorithm per the design-doc §7 / research R2: `max(per-indicator baseline)`, then `+0.05` per agreeing additional indicator, capped at `0.99`.
- Per-indicator matchers: `match_symbol_set` (HashSet-based O(N+M) overlap), `match_rodata_literal` (substring search), `match_exact_hash` (case-insensitive hex equality against Build-ID / LC_UUID / PE PDB GUID).
- `match_binary` — multi-record driver with deterministic emission order (`bucket DESC, numeric score DESC, primary_purl ASC`).

21 new unit tests covering per-indicator matchers, fusion arithmetic edge cases, above/below-floor emission, and deterministic ordering.

### v2 loader (#316)

Extends `loader.rs` with `load_v2_records_from_cache` that peeks at each JSON file's `schema_version` field to dispatch (presence → v2; absence → v1). v1 and v2 records may now coexist in a single corpus archive — detection is record-level rather than archive-level because existing milestone-108 archives have no `VERSION` sentinel and adding one would be a breaking change to the public corpus contract.

The two loader entry points (`load_corpus_from_cache` for v1, `load_v2_records_from_cache` for v2) are independent: a caller may invoke one or both. Forward-compat for archives that adopt v2 ahead of mikebom's matcher wiring.

6 new unit tests covering empty-when-v1-only, mixed-archive-loads-only-v2, unsupported-schema-version skip, malformed-record graceful degradation, missing-index error path, v1-and-v2-loaders-independent on the same cache.

### v2 production wiring (#317)

Bridges the v2 matcher + v2 loader into the existing `binary/mod.rs` scan loop. v2 records living in the configured fingerprint cache now flow through the matcher and emit as `PackageDbEntry` components alongside the v1 path. The v2 matcher pipeline is now **end-to-end live in production scans**.

Critical ordering for byte-identity preservation: v2 results merge **after** v1 and **only for libraries the v1 path didn't already cover** (the `entry().or_insert_with(...)` gate). A v2 record sharing a library name with a v1 record does NOT override the v1 emission — the 33 existing byte-identity goldens stay byte-identical for default scans.

New helpers:
- `v2_bridge::extract_printable_strings` — `strings(1)`-style extractor over `BinaryScan.string_region`.
- `v2_bridge::binary_artifact_from_scan` — `BinaryScan` → matcher's `BinaryArtifact`.
- `entry::v2_match_to_entry` — `MatchResult` → `PackageDbEntry`. Uses the matcher's numeric confidence score for `mikebom:fingerprint-confidence` rather than v1's hardcoded `"0.70"` baseline.

9 new unit tests for the helpers + zero v1 regression (all 1800+ existing tests pass unchanged).

### v2 pipeline end-to-end integration test (#318)

Three integration tests in `mikebom-cli/tests/fingerprints_v2_e2e.rs` prove the v2 pipeline emits versioned PURLs in production SBOMs:

1. **`v2_record_emits_canonical_purl_when_indicators_match`** — proves the v2 component emits with its canonical (versioned) PURL.
2. **`v2_record_emits_numeric_confidence_annotation`** — proves the FR-017 numeric annotation surfaces with value ≥ `"0.70"`.
3. **`v1_zlib_emission_survives_alongside_v2_emission`** — proves the v2 path's by_library merge gate doesn't override the v1 / source-binding zlib emission.

Fixture cache built in a `tempfile::tempdir`; the v2 record's PURL name is intentionally distinct from `zlib` so the by_library merge doesn't collide with the existing v1 / source-binding emission. Tests skip gracefully when the cmake-demo isn't pre-built on the test host (same pattern as the milestone-109 binary_source_binding tests).

### Validation across the release

- Workspace-wide unit + integration tests: 1840+ tests pass on `./scripts/pre-pr.sh` (matcher + loader + e2e + parity additions land cleanly).
- 33 byte-identity goldens pass byte-identically pre/post each PR — SC-003 contract honored across the entire release window. The version-bump-only golden regeneration in this release is the only delta to those files.
- No new Cargo dependencies between alpha.45 and alpha.46.

## [0.1.0-alpha.45] — 2026-06-03

Cross-platform completion of the symbol-fingerprint matcher (PE now joins ELF + Mach-O) + the milestone-109 cross-tier PURL attribution that closes the cmake-demo's documented "source SBOM and binary SBOM don't equality-join" gap.

**Default behavior unchanged.** Operators who don't opt into `--fingerprints-corpus` see byte-identical SBOMs to alpha.44 across all 33 byte-identity goldens. Operators who DO opt in get richer attribution: PE binaries now participate in fingerprint matching, AND cmake `FetchContent_Declare` source declarations now drive the binary-tier match's PURL into a single source-tier identity (`pkg:github/madler/zlib@v1.3.1` instead of two non-joining shadows).

### PE export-table fingerprint extraction (#309)

Extends the symbol-fingerprint matcher (originally ELF-only in milestone 099; Mach-O added in alpha.44 #305) to Windows PE binaries via `IMAGE_EXPORT_DIRECTORY` reads. The matcher's cross-platform story is now complete: same corpus content + same `min_symbols` thresholds apply across ELF + Mach-O + PE. Catches DLLs that re-export wrapped library APIs (the canonical fingerprint-matcher target on Windows). Stripped EXEs with empty export tables are documented false-negatives — same shape limitation as ELF/Mach-O.

New `pe::extract_pe_export_names(bytes)` dispatches PE32 vs PE32+ via the same magic-byte path as the milestone-098 `parse_pe_identity`; the `scan.rs::scan_binary` branch adds `class == "pe"` alongside ELF and Mach-O. 3 new unit tests (defensive garbage-bytes, PE without export table on both PE32 and PE32+).

### Milestone 109 — binary-source PURL binding via cmake build-directory observation (#310, #311)

When mikebom scans a cmake project root with `--fingerprints-corpus`, fingerprint matches in built binaries are attributed to the source-tier PURL the cmake reader emitted from `FetchContent_Declare` (`pkg:github/madler/zlib@v1.3.1`) instead of the milestone-108 generic shadow (`pkg:generic/zlib`). The two SBOM emission paths produce ONE component per real library — closing the cmake-demo's documented "phantom mismatch" gap consumers tripped over.

#### MVP (#310, US1+US2)

New sub-module `mikebom-cli/src/scan_fs/binary/source_binding/` containing the cmake build-directory observer + attribution registry:

- `cmake_observer.rs` walks the scan root with bounded recursion (depth ≤6) for cmake project build dirs (`CMakeCache.txt` + `_deps/` co-presence); joins cmake declarations against `_deps/<name>-build/` existence.
- `registry.rs` provides case-insensitive library-name lookup + path-ancestor scope matching + deterministic multi-project tie-break per the milestone-105 dedup-pipeline conventions.
- `BuildDirObserver` trait keeps the cmake-specific path-observation logic isolated; future Bazel / Meson observers plug into the same registry without rework.

`symbol_fingerprint::scan_with_corpus` gained two optional params (`Option<&BuildAttributionRegistry>` + `Option<&Path>`) — when both are `Some(_)`, the matcher rewrites each match's `target_purl` from `pkg:generic/<library>` to the registry-resolved source-tier PURL. `None`/`None` preserves milestone-108 behavior exactly.

The dedup pipeline (`resolve::deduplicator::deduplicate`) now folds LOSER-side `extra_annotations` into the WINNER on PURL collision so the merged component carries BOTH source-tier (`mikebom:source-mechanism = cmake-fetchcontent-git`) AND binary-tier (`mikebom:fingerprint-corpus-sha`, `mikebom:fingerprint-symbols-matched`) annotations.

Scope: `FetchContent_Declare` (git + url forms) only this milestone. `ExternalProject_Add` deferred per the Phase-2 clarification (its `<name>-prefix/` default layout needs separate research). Bazel + Meson tracked as follow-on observers.

12 new unit tests + 3 new integration tests cover the join key, scope-ancestry, ExternalProject deferral, multi-project workspaces, noise-directory skip (`.git`, `node_modules`, etc.), and the cmake-demo's end-to-end "scan project root, get ONE zlib component" contract.

#### Polish (#311, US3+US4+US5)

- **US3 regression tests** prove non-opt-in scans + single-binary scans preserve milestone-108 behavior (SC-003 + SC-004): no `mikebom:fingerprint-corpus-sha` annotations appear without the opt-in; the milestone-108 generic-PURL fallback fires correctly for single-binary scans.
- **US4 cross-format symmetry test** emits CDX 1.6 + SPDX 2.3 + SPDX 3 of the same fixture; verifies each format carries the source-mechanism + corpus-sha annotations AND the source-tier PURL, with zero `pkg:generic/zlib` shadows.
- **US5 forward-compat smoke test** implements `BuildDirObserver` with a Bazel-shaped stub, proving the trait surface is observer-agnostic per FR-012. Architectural extension comment at the bottom of `source_binding/mod.rs` documents the 3-step recipe for adding future observers.
- `docs/ecosystems.md` "Binary analysis" section gains a milestone-109 subsection explaining the 4-step attribution mechanism + scope limits.
- `docs/reference/identifiers.md` §11.7 explains the three observable component shapes (source+binary multi-evidence; source-only declared-but-unused; binary-only single-binary-scan).
- New `tests/offline_mode_audit_ecosystem_109.rs` enforces all three `source_binding/` files are free of network primitives (no allowlist — milestone 109 has no network surface).

### README pre-1.0 stabilization framing (#308)

Replaced the "no way production ready / needs a lot more hardening" line with concrete pre-1.0 framing that names the three stabilizing surfaces (CLI, output formats, per-ecosystem coverage) and sets the expectation that more ecosystem readers + binary-analysis surface keep landing release-over-release.

### Companion demo: `kusari-sandbox/mikebom-cmake-demo`

The runnable cmake + ninja C project (introduced in alpha.44) gained:

- Cross-platform reframing for Step 4: Mach-O (alpha.44) + PE (alpha.45) are now first-class; the original "Linux-only via Docker" framing is gone.
- New **Step 5 — Cross-tier alignment** demonstrating milestone 109's attribution mechanism end-to-end + the consumer-side `comm -23` equality-join recipe for diffing source-only vs project-root SBOMs.

### Validation across the release

- Workspace-wide unit + integration tests: 1790+ tests pass on `./scripts/pre-pr.sh`.
- 33 byte-identity goldens pass byte-identically pre/post #309 + #310 + #311 — SC-003 contract honored across the entire release window.
- No new Cargo dependencies between alpha.44 and alpha.45.

## [0.1.0-alpha.44] — 2026-06-02

External symbol-fingerprint corpus (milestone 108) — the first milestone since 091 to add a NEW network call to mikebom, deliberately and bounded. mikebom can now identify statically-linked C libraries beyond the bundled 7 (openssl, zlib, libcurl, sqlite, pcre, pcre2, gnutls) by consulting an external corpus pinned at a SHA in the sibling repo [`kusari-sandbox/mikebom-fingerprints`](https://github.com/kusari-sandbox/mikebom-fingerprints). Every component identified via the external path carries a `mikebom:fingerprint-corpus-sha` provenance annotation that consumers can resolve back to the exact fingerprint record. Plus a Mach-O extension to the symbol matcher that closes the macOS gap on the fingerprint path.

**Default behavior unchanged.** Operators who don't opt into the external corpus (no `--fingerprints-corpus` flag, no `MIKEBOM_FINGERPRINTS_CORPUS=1` env) see zero behavioral change — bundled `FINGERPRINTS` const is still the matcher's source, no new annotations stamped, all 33 byte-identity goldens pass byte-identically. SC-003 byte-identity contract is the milestone's primary no-regression guarantor.

### Sibling repo seeded: `kusari-sandbox/mikebom-fingerprints`

New public Apache-2.0 repo that holds the source-of-truth corpus. Day-1 content mirrors the bundled 7 libraries (same symbol lists, same `min_symbols=8` thresholds). `schema/fingerprint-record.v1.json` + `schema/index.v1.json` formalize the record shape; `scripts/validate.sh` runs the same checks the CI workflow runs so contributors can pre-flight before pushing. Branch protection requires 1 approving review + a green `schema + invariants` check (deliberate-failure PR `#2` proved the gate during bootstrap). `CONTRIBUTING.md` walks through the libxml2 worked example, the `min_symbols` rule-of-thumb table, symbol-selection do/don'ts, and the PR template.

### Foundation: loader + cache + bundled fallback (#299)

`mikebom-cli/src/scan_fs/binary/fingerprints/` lands as a new sub-module:

- `source_sha.rs` — `CorpusSha([u8;20])` newtype with full / short hex display widths.
- `record.rs` — `FingerprintRecord` serde shape + `validate()` covering FR-010 defensive rules.
- `cache.rs` — `cache_root()` resolution (`MIKEBOM_FINGERPRINTS_CACHE_DIR` env > XDG > HOME), `cache_dir_for_sha`, `cache_hit`, `cache_clear(KeepRev)` per FR-009.
- `loader.rs` — `load_corpus_from_cache(sha)` reads `<cache>/<sha>/corpus/index.json` + per-library JSONs; malformed records warn-and-skip per FR-010.
- `mod.rs` — `FingerprintCorpus` container, `CorpusSource` enum, `LoadOptions`, `load_bundled()` memoized via `OnceLock`.

Build-time SHA pin lives at `tests/fingerprints.rev` (one line); `build.rs::emit_fingerprints_corpus_sha()` reads it, validates 40-char lowercase hex, emits `cargo:rustc-env=MIKEBOM_FINGERPRINTS_CORPUS_SHA=<sha>`. No network at build time.

### US1 — Maintainer contribution flow (#300)

Tracking-only PR. Marks T030 verified end-to-end via the deliberate-failure PR on the sibling repo (CI rejected the malformed record in 10s on `missingProperty: 'min_symbols'`); T031/T032 already shipped in the sibling-repo bootstrap.

### US2 — Operator opt-in + cache-first fetch + annotation stamping (#301)

`--fingerprints-corpus` flag (also `MIKEBOM_FINGERPRINTS_CORPUS=1`) opts the binary scanner into the external corpus. `fingerprints/fetch.rs` ships the GitHub-archive fetch path: 30s timeout, 5-redirect cap, retry 3x on 5xx with 1/2/4s exponential backoff, `Retry-After`-on-429 (60s cap), 404 → typed `NotFound`. Atomic-write protocol stages to `.tmp-<uuid>/corpus/` then renames to `<full-sha>/`; concurrent-writer race handled. Blocking HTTP wrapped in `std::thread::scope` to escape mikebom's tokio runtime — same posture as `golang::graph_resolver`'s blocking workers.

`load_corpus(opts)` implements the FR-004 decision tree (`!external_enabled` → bundled; cache hit → `Cached`; cache miss + `!offline` → fetch → `Fetched`; cache miss + offline → bundled with warn). `symbol_fingerprint::scan_with_corpus` stamps `mikebom:fingerprint-corpus-sha` (12-hex of the corpus revision OR literal `bundled` sentinel) on every emitted match. FR-013 multi-record collision: when matched-symbol sets overlap (e.g., LibreSSL + OpenSSL share `SSL_*` symbols), each match emits + carries `mikebom:also-detected-via` listing the other; independent co-resident libraries (openssl + zlib, disjoint matched sets) don't trigger.

### US3 — Consumer verification recipe + CI gate (#302)

`docs/reference/identifiers.md` §11 documents the consumer-side 4-step lookup recipe (`jq` annotation → GitHub git-API resolve 12-hex → tarball download → record read → `readelf` symbol-table confirmation). The milestone-108 quickstart Scenario 1.5 mirrors the recipe with operator-friendly framing. New CI gate `embedded_sha_resolves_to_real_commit_on_sibling_repo` (network-gated) catches the maintainer-typo failure mode at CI time (bumping `tests/fingerprints.rev` to a SHA that doesn't exist on the sibling repo).

### US4 — Air-gapped operator subcommands (#303)

`mikebom fingerprints` top-level subcommand with `fetch [--corpus-rev <SHA>]` (the only mikebom subcommand REQUIRED to perform a network call per FR-008), `cache-clear [--keep-rev <SHA>]` (purely local; idempotent), `list` (purely local introspection, alphabetically sorted). Categorized exit codes (0/1/2/3/4/10) per `contracts/cli-surface.md`. 11 new tests including a full 5-stage air-gapped roundtrip (fetch on tempdir A → tar → untar to tempdir B → offline scan against B → assert valid CDX SBOM).

### US5 — `--fingerprints-rev` runtime override (#304)

Operators can pin a specific corpus version regardless of the mikebom binary's build-time-embedded SHA. Clap value parser validates 40-char lowercase hex; implicit-dep warn when supplied without `--fingerprints-corpus` (override ignored, bundled fallback used). `LoadOptions.sha_override` plumbs through to the cache key AND the fetch URL; the SBOM annotation reflects the override. 4 new tests in `hermetic_build_pin.rs` cover byte-identity (modulo `serialNumber`), distinct-SHA cache routing, and the implicit-dep warn.

### Mach-O symbol extraction (#305)

Extends the milestone-099 symbol-fingerprint scanner from ELF-only to ELF + Mach-O so `--fingerprints-corpus` works directly on macOS without a Linux container. New `strip_macho_underscore_prefix` helper handles Mach-O's C ABI symbol prefix; `scan_binary` filters `Object::symbols()` to globals (`N_EXT`) for Mach-O. PE export-table extraction deferred (different shape — `IMAGE_EXPORT_DIRECTORY`). Verified end-to-end against the cmake-demo macOS binary: 10/10 zlib symbols matched + `mikebom:fingerprint-corpus-sha: fff39c6ad22c` stamped.

### Polish (#306)

- New catalog row C58 in `docs/reference/sbom-format-mapping.md` for `mikebom:fingerprint-corpus-sha` with the full native-field audit per Constitution Principle V. Matching parity extractors registered in all three formats so the `every_catalog_row_has_an_extractor` invariant holds.
- `tests/offline_mode_audit_ecosystem_108.rs` enforces that `fingerprints/fetch.rs` is the ONLY file in the sub-module allowed to contain network primitives. Different shape than milestone-106/107 audits because milestone 108 DOES make ONE legitimate network call (the corpus fetch); the audit's allowlist-of-one isolates the blast radius.
- `bundled_fingerprint_const_size_locked` asserts `FINGERPRINTS.len() == 7` so future contributors can't accidentally grow the bundled const — the source-of-truth lives at the sibling repo now.
- `docs/ecosystems.md` gains a cross-link section pointing at identifiers.md §11, the milestone-108 quickstart, and the cmake-demo.

### Companion demo: `kusari-sandbox/mikebom-cmake-demo`

New runnable cmake + ninja C project that exercises both the source-tree reader AND the fingerprint matcher end-to-end. `FetchContent_Declare(zlib v1.3.1)` + static-link + `ENABLE_EXPORTS TRUE`; main.c uses 10 zlib API entry points so the static linker pulls them all in. README walks the source scan (cmake reader → `pkg:github/madler/zlib@v1.3.1`) and the binary scan with `--fingerprints-corpus` (fingerprint match → corpus-sha annotation) on both macOS (post-#305) and Linux.

### Validation across the milestone

- Workspace-wide unit + integration tests: 1770+ tests pass on `./scripts/pre-pr.sh`.
- 33 byte-identity goldens unchanged across all 7 PRs — SC-003 contract honored.
- 1 new Cargo dep (none) — `wiremock` was already a dev-dep; `reqwest`, `tar`, `flate2`, `tempfile`, `uuid` all already in the tree.

## [0.1.0-alpha.43] — 2026-06-01

Yocto / OpenEmbedded coverage (milestone 107) — the explicit follow-on to milestone 105's US7 split-off, and the largest remaining C/C++ source coverage gap. Three new filesystem readers landed across five PRs, plus a foundational refactor that opkg shares with dpkg. Filesystem-only, zero new Cargo dependencies, zero new network calls (FR-011 build-time audited).

### Foundation refactor: control_file stanza parser (#293)

opkg's `/var/lib/opkg/status` uses byte-identical RFC-822 control-file syntax to dpkg's status file. Rather than duplicate the ~70-LOC stanza parser, #293 extracts it into a new shared `mikebom-cli/src/scan_fs/package_db/control_file.rs` helper that both `dpkg.rs` and the new `opkg.rs` consume. **Net behavior-neutral for dpkg**: the 33 byte-identity goldens (11 CDX + 11 SPDX 2.3 + 11 SPDX 3) pass byte-identically post-refactor. First-occurrence-wins on duplicate field names, multi-line `Description:` continuation, case-insensitive field-name lookup — all the prior dpkg parser's behaviors preserved verbatim.

### opkg installed-DB reader + sysroot detection (#294, US1+US3+US5)

New reader at `mikebom-cli/src/scan_fs/package_db/opkg.rs`. Yocto-built device rootfs scans and OpenSTLinux SDK sysroot scans now emit one `pkg:opkg/<name>@<version>?arch=<arch>` component per stanza, with `mikebom:source-mechanism: "opkg-installed"` annotation feeding the milestone-105 dedup pipeline. Per-package `/usr/lib/opkg/info/<pkg>.list` files are read for binary-walker claim collection (prevents duplicate `pkg:generic/<basename>` emissions for files already owned by an opkg package).

New `yocto/context.rs` implements the FR-005a two-signal sysroot heuristic: primary = `environment-setup-*` script anywhere from the scan target up to 2 ancestors above (Yocto SDK installer always writes one); secondary = `/usr/include/` present + `/etc/init.d/` absent. Sysroot context applies `LifecycleScope::Build` to every emitted entry → CDX `scope: "excluded"`, SPDX `BUILD_DEPENDENCY_OF`. Ambiguity (primary fires AND `/etc/init.d/` actively present) records a `mikebom:scan-ambiguity` SBOM-metadata diagnostic.

Per-stanza FR-006 override: `nativesdk-*` prefixed packages OR packages whose `Architecture:` field matches a host-arch literal (`x86_64`, `i686`, `aarch64`, `arm64`) always carry build-scope regardless of context.

### Yocto image-manifest reader (#295, US2)

New reader at `mikebom-cli/src/scan_fs/package_db/yocto/manifest.rs`. Walks `build/tmp/deploy/images/<machine>/*.manifest` and emits one `pkg:opkg/<name>@<version>?arch=<arch>` component per `<name> <arch> <version>` line — same PURL ecosystem as the installed-DB reader, so cross-source emissions of the same coord collapse via the milestone-105 dedup pipeline. FR-010 precedence: `OpkgInstalled` > `YoctoImageManifest`, so when both fire on the same scan the installed-DB wins and the manifest's source-mechanism appears in `mikebom:also-detected-via`.

### BitBake recipe walker (#296, US4)

New reader at `mikebom-cli/src/scan_fs/package_db/yocto/recipe.rs`. Walks the scan tree (max_depth=8) for `.bb` recipe files in `meta-<vendor>/recipes-*/<name>/<name>_<version>.bb` and emits one component per recipe. **Filename-only** — recipe body is NOT parsed (FR-007 explicit scope boundary). PURL: `pkg:bitbake/<name>@<version>?layer=<layer-name>` — distinct ecosystem from `pkg:opkg/` because recipes are layer declarations, not installed packages.

Per FR-008: filenames containing unexpanded `${...}` (typically shared-base recipes like `${PN}_${PV}.bb`) are silently skipped with `tracing::warn!` — no placeholder component, no `unresolved` sentinel. Filenames with no `_<version>` segment emit with `version: "unknown"` + `mikebom:version-status: "missing"` annotation.

### Polish (#297)

- `docs/ecosystems.md` gains a new `## yocto` H2 section covering all three readers + a `[yocto](#yocto)` matrix row.
- `tests/offline_mode_audit_ecosystem_107.rs` (FR-011) grep-audits the 6 new reader source files against tripwire substrings (`reqwest::`, `tokio::net::`, `hyper::`, `Command::new("curl"|"wget"|"http"`, `TcpStream`/`TcpListener`, `std::net::TcpStream/Listener`). Asserts FR-011 offline-only contract independently of the readers' own behavior.
- `tests/polyglot_robustness_ecosystem_107.rs` (SC-006) builds a single-rootfs fixture with well-formed AND malformed inputs from all three readers in close proximity (opkg DB with a garbage block between two well-formed stanzas; two `.manifest` files in adjacent machine dirs — one well-formed, one wrong-token-count; one well-formed `.bb` + one `${PN}_${PV}.bb` for the silent-skip path). Asserts the scan exits 0 and each well-formed input still surfaces despite its malformed sibling.
- `tests/cross_reader_dedup_ecosystem_107.rs` (SC-007) puts the same canonical PURL into BOTH `/var/lib/opkg/status` and a `<image>.manifest`. Asserts the emitted SBOM's surviving component carries `mikebom:source-mechanism: "opkg-installed"` — proving the FR-010 precedence ladder + the `SourceMechanism` enum declaration order are wired correctly.

### Validation across the milestone

- Workspace-wide unit + integration tests: 1730+ tests pass on `./scripts/pre-pr.sh`.
- All fixture package names use synthetic `mikebom-fixture-*` prefixes (lesson from milestone 106 — no CVE-advisory collisions).
- No new Cargo dependencies — uses existing `regex` (workspace) for the `.bb` filename parser; everything else is std.

## [0.1.0-alpha.42] — 2026-05-31

Ecosystem coverage expansion (milestone 106). Five new lockfile readers landed across six PRs, covering every modern JS / Python / Java / .NET package manager mikebom didn't previously see on source-tree scans. Filesystem-only, zero new Cargo dependencies, zero new network calls (FR-012 build-time audited).

### uv (#284, closes #276)

New reader at `mikebom-cli/src/scan_fs/package_db/pip/uv_lock.rs`. Modern Python projects using `uv.lock` (TOML) now emit `pkg:pypi/<name>@<version>` components with `[[package.dependencies]]`-derived dep-graph edges. Workspace support: `[tool.uv.workspace]` member detection emits a synthetic `pkg:generic/<workspace-name>` root + `mikebom:component-role: "main-module"` member components with intra-workspace edges preserved.

### Bun (#285, closes #278)

New reader at `mikebom-cli/src/scan_fs/package_db/npm/bun_lock.rs`. Parses Bun's JSONC `bun.lock` format via the shared `npm/jsonc.rs` comment-stripper (also new). Emits `pkg:npm/<name>@<version>` PURLs with URL-encoded `@` on scoped names; workspace support mirrors uv's shape; `overrides` map applied at registry-emission time. The legacy binary `bun.lockb` format is out of scope.

### Gradle (#286, closes #277)

New reader at `mikebom-cli/src/scan_fs/package_db/gradle/`. Handles both `gradle.lockfile` (runtime classpath) and `buildscript-gradle.lockfile` (build-script / plugin classpath) via one line-oriented parser. Emits `pkg:maven/<group>/<name>@<version>` PURLs so existing deps.dev enrichment applies without changes. Filename selects lifecycle scope: `buildscript-gradle.lockfile` → `LifecycleScope::Build` → CDX `scope: "excluded"` + SPDX `BUILD_DEPENDENCY_OF` via the existing milestone-052 emission path.

### NuGet (#287, closes #275)

New reader at `mikebom-cli/src/scan_fs/package_db/nuget/` (5 files: `mod.rs` orchestration, `csproj.rs` XML parser, `directory_packages_props.rs` CPM lookup + walk-up, `private_assets.rs` LifecycleScope classifier, `packages_lock.rs` JSON lockfile parser). Walks `.csproj`/`.vbproj`/`.fsproj` files and applies a four-step version-resolution ladder: `packages.lock.json` → inline `Version=` → CPM (`Directory.Packages.props` walked up bounded by `scan_root`) → `unresolved` sentinel. `PrivateAssets="All"` / `IncludeAssets`-without-`runtime` / `ExcludeAssets=runtime` map to `LifecycleScope::Build`. Lockfile transitives that don't appear in any `.csproj` emit as `mikebom:source-type: "transitive"`. Dep-graph edges from the lockfile populate `PackageDbEntry.depends`.

### Yarn (#289, closes #274)

New reader at `mikebom-cli/src/scan_fs/package_db/npm/yarn_lock.rs`. Handles **both** Yarn lockfile formats — v1 (Classic, line-oriented text) and Berry (Yarn 2+, YAML-shaped) — with content-sniffed auto-detection via the `__metadata:` block sentinel. Both formats emit the same `pkg:npm/<name>@<version>` PURL shape (scoped names URL-encoded) and populate dep-graph edges from each entry's `dependencies:` map.

### Polish (#288, #290)

- `docs/ecosystems.md` updated: new `nuget` matrix row; existing `pip` / `npm` / `maven` rows amended with the new lockfile sources; per-lockfile subsections (uv / Bun / Gradle / Yarn) under their parent ecosystems; full `## nuget` section covering the resolution ladder + lifecycle-scope + source-files merging.
- `tests/offline_mode_audit_ecosystem_106.rs` — FR-012 build-time test grep-fails the build if any of the 11 new reader source files contains `reqwest::`, `tokio::net::`, `hyper::`, `Command::new("curl"|"wget"|"http"`, `TcpStream`/`TcpListener`. Locks the readers as filesystem-only against regression.
- `tests/polyglot_robustness_ecosystem_106.rs` — SC-006 regression test builds a fixture with well-formed + deliberately-malformed manifests from all 5 ecosystems; asserts each well-formed manifest still emits its representative component despite sibling malformed files. Mirrors milestone-105's `polyglot_legacy_lockfile_robustness.rs`.

### Validation across the milestone

- Workspace-wide unit + integration tests: 1700+ tests pass on `./scripts/pre-pr.sh`.
- All five fixture sets use synthetic `mikebom-fixture-*` / `MikebomFixture.*` package names (lesson from PR #285's lodash flagging) — the fixtures never collide with real-world CVE advisories.
- No new Cargo dependencies (uses existing `quick-xml`, `serde_json`, `serde_yaml`, `toml`).

## [0.1.0-alpha.41] — 2026-05-27

Quick follow-up to alpha.40. Single change.

### npm: stop emitting edges for peerDependencies (#270)

alpha.40's #267 walked all four standard npm dep sections — `dependencies`, `devDependencies`, `peerDependencies`, `optionalDependencies` — when building `entry.depends`. User flagged that `peerDependencies` are semantically **declarative** ("the consumer should have X installed") not **install-relational** ("this package depends on X"). The SBOM `dependsOn` / `DEPENDS_ON` slot encodes the latter; trivy and syft also skip peer-edges. mikebom now matches.

Dropped from the walked sections in `parse_package_lock` (Tier A), `walk_node_modules` (Tier B), `apply_nameless_secondary_umbrella`, and `build_npm_main_module_entry`.

**Molcajete validation (from alpha.40 → alpha.41):**

| Metric | alpha.40 | alpha.41 |
|---|---|---|
| Orphan count | 0 | 0 (unchanged) |
| Total relationships | 1368 | 1217 (-151 spurious peer-edges removed) |

Every package previously reachable via a peer-edge is also reachable via a real dep / dev / optional edge from some other parent. Peer-edges were pure noise, not load-bearing.

## [0.1.0-alpha.40] — 2026-05-27

This release bundles a major npm reachability fix (#267, validated against the `molcajete` corpus 66 → 0 orphans) plus a test-only Go regression (#264).

### npm: full walk-up `node_modules` dep resolution (#267)

PR #263 (alpha.38) introduced version-pinning for nested-`node_modules` installs. Validation against a real-world Vite + Vue corpus (`molcajete`) surfaced two remaining orphan classes — both attributable to a single root cause that this PR addresses fully:

**Root cause** — mikebom's npm reader emitted `entry.depends` as bare names like `"commander"` when the immediate-child path lookup didn't match. The edge resolver's `name_to_purl` last-write-wins HashMap then produced the **wrong version** when multiple installs of the same package existed in the tree. Concrete case: `d3-dsv` declares `commander: "7"` with no nested install; a different parent (`editorconfig`) has nested `commander@10.0.1`. Pre-fix, `d3-dsv → commander@10` (wrong); the hoisted `commander@7.2.0` ended up orphan.

**Fix** — walk up the `node_modules` tree to resolve every declared dep, mirroring npm's actual resolution algorithm. For a parent at `<a>/.../<x>` declaring dep `Y`:

1. Try `<a>/.../<x>/node_modules/Y`
2. Try `<a>/.../node_modules/Y`
3. ... ascending
4. Top-level `node_modules/Y` (hoisted)

Whichever path finds Y FIRST wins. Bare-name fallback only fires for deps that aren't installed anywhere (rare for well-formed lockfiles).

**Applied to BOTH** `package_lock.rs::parse_package_lock` (Tier A lockfile parsing) AND `walk.rs::build_npm_main_module_entry` (synth root's main-module emission). The latter previously bypassed all version-pinning because it doesn't go through `parse_package_lock`.

**Section coverage** — the fix walks all four standard npm dep sections (`dependencies`, `devDependencies`, `peerDependencies`, `optionalDependencies`). PR #263 walked only `dependencies`; canonical example: `optionalDependencies fsevents` declared by chokidar/rollup/esbuild/vite/vitest now correctly pins.

**Molcajete corpus**:

| Stage | Orphans |
|---|---|
| alpha.39 (pre-fix) | **66** |
| Walk peer + optional + dev sections | 19 |
| Full walk-up `node_modules` resolution | **0** |

**Transitive-parity baseline shift** — `transitive_parity_npm`'s `EXPECTED_MIKEBOM_EDGE_COUNT` shifts 147 → 155. The +8 edges are recovered routings where bare-name last-write-wins was previously sending parents to dev-scoped nested variants (causing those edges to emit as `DEV_DEPENDENCY_OF` and not count in the extractor).

### test(go): end-to-end regression for issue #250 tool-directive linkage (#264)

Test-only. PR #252 (alpha.37) fixed the Go 1.24+ `tool` directive linkage with unit tests on `parse_go_mod` and `resolve_tool_to_module`. This PR adds the missing end-to-end test that exercises the full `read()` pipeline against the user's exact reproducer shape — pins the working behavior so future refactors can't silently regress.

## [0.1.0-alpha.39] — 2026-05-27

This release bundles four PRs: a new global `--timeout` flag for wall-clock-bounded scans, two npm orphan-attribution fixes (#260, #263), and a docs recipe for Kubernetes workload identity tagging (#261).

### New global flag: `--timeout <SECONDS>` (#265)

Adds a wall-clock time limit for the entire `mikebom` invocation. When exceeded, mikebom emits a `tracing::error` to stderr and exits with status `124` (POSIX [`timeout(1)`](https://www.gnu.org/software/coreutils/manual/html_node/timeout-invocation.html) convention). Disabled when omitted or set to `0`.

Use cases: bounding runaway scans in CI, protecting Kubernetes CronJob pod-disruption budgets, capping exploratory image scans against potentially-large container filesystems.

Coexists with the existing `mikebom trace run --timeout` (which caps the SUBPROCESS being traced, not mikebom itself). Whichever fires first wins. Per-fetch internal timeouts (OCI registry pulls, deps.dev HTTP, `go mod graph` subprocess) are unaffected.

Partial output isn't guaranteed when the watchdog fires — operators who need "best-SBOM-in-N-seconds" semantics should pair `--timeout` with `--output <PATH>` and check the file's presence after the run. New docs section in `cli-reference.md` includes a `case $?` recipe for distinguishing exit 124 from 0.

### npm: nested-`node_modules` deps now pin to their installed versions (#263, closes #262)

When npm's resolver hoists one version of a transitive dep and nests another under its consumer (`node_modules/<parent>/node_modules/<dep>`), the nested install was previously emitted as a component but had **zero incoming edges** — the lockfile parser populated `entry.depends` with bare names like `"pathe"`, and the edge resolver's last-write-wins matched the hoisted version, leaving the nested one orphan.

Real-world impact: a Vite + Vitest project's molcajete corpus had 47 multi-version orphans (`debug` 2.x/4.x, `minimatch` 3.x/9.x, three `@opentelemetry/semantic-conventions` versions, etc.) — ~7% of the dep tree unreachable from root.

#### Fix shape

1. **Lockfile parser**: build a `(path_key → version)` index up front. For each entry, when constructing `entry.depends`, check whether a nested child exists at `<this_path>/node_modules/<dep>`. If yes, emit the dep as `"<dep> <version>"` (version-pinned); else bare `"<dep>"` (existing behavior). Mirrors cargo's milestone-087 disambiguation pattern.
2. **Edge resolver**: register a `(npm, "name version")` key for every npm component alongside the existing `(npm, "name")` bare-name key.
3. **Same-PURL dev/runtime dedup**: when the same PURL appears as both Dev and Runtime variants (e.g., `@babel/core/node_modules/ms@2.1.3` dev alongside `send/node_modules/ms@2.1.3` runtime), the dedup at `mod.rs:118` previously kept whichever came first alphabetically — typically the dev variant. Inert pre-fix (the wrongly-tagged variants were orphans). After this PR's version-pinning lands, edges now correctly land on these dedup'd components, so the wrong scope tag triggers `DEV_DEPENDENCY_OF` rewriting. Fix: dedup IN `parse_package_lock` preferring Runtime over Development.

#### Test baseline shift

`transitive_parity_npm`'s `EXPECTED_MIKEBOM_EDGE_COUNT` shifts **150 → 147**. The 3-edge shift is a wire-format reclassification: 3 edges from dev-scope parents now correctly route to their nested dev-scope targets and emit as `DEV_DEPENDENCY_OF` (reversed direction, per milestone-228) instead of `DEPENDS_ON`. The underlying dep relationships are still present; they just don't ride the `DEPENDS_ON` slot. Pre-fix routing was wrong (bare-name last-write-wins accidentally pointed dev parents at hoisted runtime targets); post-fix routing matches the actually-installed nested versions.

### npm: nameless-secondary umbrella widened to private+no-version manifests (#260, closes #245)

PR #257's umbrella for nameless secondary `package.json` files only fired when the `name` field was missing. The trigger missed `private: true` + no-`version` manifests, which FR-001 also skips from main-module emission (per #104 guidance). Those manifests' declared deps ended up orphan.

Fix: switch the umbrella's "target pool" criterion from "manifest has a name" to "manifest's main-module entry actually exists in entries". The new criterion captures every manifest the main-module-build loop didn't handle — whether nameless, private+no-version, or any future skip condition.

The basic-shape #245 reproducer (named secondary with `node_modules/`) was already passing on alpha.38 thanks to milestone-066's main-module emission — a unit-test regression captures that working shape for future maintainers.

### docs: Kubernetes workload identity recipe (#261, closes #231 + task #35)

Issue #231 originally proposed dedicated `--cluster-id` / `--namespace` / `--workload-name` / `--workload-kind` / `--workload-uid` CLI flags. On triage we opted for a docs recipe using the existing `--id <scheme>=<value>` flag rather than adding net-new CLI surface for a single ecosystem's identity model.

Recipe 7 in `docs/user-guide/quickstart.md` now covers:
- The canonical flag invocation pattern with `k8s_*` scheme prefix.
- Where the values land per format (CDX `metadata.annotations[]`, SPDX 2.3 document annotations, SPDX 3 native `externalIdentifier[]`).
- A `kubectl get pod ... -o jsonpath=...` snippet for driving the flags from a Kubernetes operator.
- Cross-reference to `--registry-credentials-dir` for the in-cluster `imagePullSecret`-mount pattern.
- Naming tips (prefer `workload_uid` over pod-name for stability).

Renumbered existing recipes 7-10 → 8-11.

## [0.1.0-alpha.38] — 2026-05-26

This release bundles two orphan-attribution improvements that surfaced in user-supplied reproducers after alpha.37: the nameless secondary `package.json` umbrella (#257 / closes #256) and the Go `+incompatible` legacy-residue filter (#258 / closes #255). Both refine the orphan-handling work from PR #253 (alpha.37): #257 closes a real linkage gap that #253 didn't cover (orphan dep subtree from a non-npm-publishable secondary manifest); #258 reduces a false-positive (`+incompatible` residue getting flat-attached to root via #253's backfill).

### npm nameless secondary `package.json` umbrella (#257, closes #256)

A secondary `package.json` in a scan tree that omits the `name` field previously produced an entire orphan dependency subtree in BOTH CycloneDX and SPDX outputs. In the user's repro, 57 of 88 components (65% of the dep tree) were unreachable from the document root. Per the npm spec, `name`/`version` are only required for *publishable* packages; lock-down secondary manifests (integration-test utility configs, schema-lint configs, CI tooling configs) routinely omit them — a real and common shape.

- **Fix**: new `apply_nameless_secondary_umbrella` pass in the npm reader. For each nameless secondary, find the closest enclosing PRIMARY project root and merge the nameless manifest's declared `dependencies[]` (and `peer-`/`optional-`, plus `dev-` when `--include-dev`) into that primary main-module's `.depends`.
- **Annotation**: each merged dep's component gets a `mikebom:source-manifest: <relative-path>` annotation so the manifest provenance survives the topology flattening. Graph-walking SBOM consumers see the dep is reachable from root; provenance-walking consumers can still trace it to its declaring manifest.
- **Edge case**: scans with ONLY nameless secondaries (no primary main-module to anchor to) warn-log and leave the deps as orphans — there's no anchor to attach to.

### Go `+incompatible` legacy-residue filter (#258, closes #255)

A real-world `guacsec/guac@ebb808e` scan emitted both `pkg:golang/github.com/google/martian@v2.1.0+incompatible` AND `pkg:golang/github.com/google/martian/v3@v3.3.3` as components, with `martian@v2` flat-attached to root via PR #253's residual-orphan backfill. `go mod why -m github.com/google/martian` confirmed the main module doesn't actually need `martian` v2 — it's residue left in go.sum after the project upgraded to the `/v3`-suffixed module form.

- **Fix**: drop a Go component if BOTH its version contains `+incompatible` AND a same-base-path `<path>/vN` (N ≥ 2) sibling exists in the emitted entries. The narrow filter matches the user's "narrow bug" framing.
- **Intentionally NOT filtered**: general go.sum-but-not-go.mod-reachable modules (e.g. `gopkg.in/check.v1`, pulled via `yaml.v3`'s test deps). A broader filter caught these too and was walked back to preserve test-transitives that operators expect to see in the SBOM for vulnerability scanning.

### Added utility

- `ModuleGraphMap::reachable_from(seeds: &[ModuleId]) -> HashSet<ModuleId>` — BFS through the resolver map. Added as documented public API for future work on reachability-based filtering or external resolver-inspection tooling. Not consumed by the alpha.38 narrow `+incompatible` filter; reserved for future use.

## [0.1.0-alpha.37] — 2026-05-26

This release bundles two Go-orphan fixes (#252 / #253) and two release-pipeline fixes (#248 / #249) that surfaced during the alpha.36 publishing of the multi-arch container image.

### Go 1.24+ `tool` directive support (#252, closes #250)

`parse_go_mod` previously fell into its "unknown directive — skip" branch for `tool` lines, leaving Go 1.24+ tool deps as orphans tagged `mikebom:orphan-reason: unresolved-indirect-require`. This release adds:

- **Parser**: recognises the `tool` directive (single-line + block form), with a new `tools: Vec<String>` field on `GoModDocument`.
- **Edge emission**: each tool's enclosing module is resolved via longest-path-segment-prefix-match against the discovered Go module set and flat-attached to main-module's `.depends`.
- **Annotation**: matched components get `mikebom:component-role: build-tool` (a new closed-enum value on the existing milestone-061 annotation slot — `main-module` was the only prior value). SBOM consumers can now distinguish Go build-time tool deps from regular runtime/library deps.
- **Standards-first follow-ups deferred**: CDX `Component.scope: optional`, SPDX 2.3 `BUILD_TOOL_OF` relationship type, and SPDX 3.0.1 `usesTool` relationship type would be deeper emitter changes; the mikebom-namespaced annotation carries the diagnostic signal across all three formats today.

#### Added

- 9 new unit tests covering the parser (single-line / block / empty-path-skip / comment-stripping), the resolver (longest-prefix wins / segment-boundary required / exact match / no match), and the annotation-contract naming stability.

### Go indirect-require orphan backfill (#253, closes #251)

Real-world Go workspaces (e.g. `guacsec/guac@ebb808e`) saw 70 `// indirect` requires emitted as orphans on alpha.36 even though each was reachable per `go mod why -m` through a module mikebom DID include in the SBOM. With 161 of 660 components (24%) unreachable from the document root, graph-walking SBOM consumers were missing a quarter of the dep tree. This release adds:

- **Backfill**: after milestone-091's `gosum_fallback_paths()` flat-attach step, a second pass identifies any Go component in `out` with zero incoming edges from non-main entries and flat-attaches it to `main_entry.depends`. Establishes the reachability invariant "every emitted Go component is reachable from main-module" while preserving the milestone-059 hierarchical graph topology where the resolver's attribution succeeded — the flat edge is a FALLBACK, AFTER the resolver's 3-step ladder gets first chance.
- **Annotation**: backfilled components get `mikebom:orphan-reason: flat-attached-fallback` (a new closed-enum value on the milestone-061 annotation slot). The annotation's meaning widens slightly: from strictly "no incoming edge" to "incoming edge attribution unknown / synthesized." Existing `unresolved-indirect-require` continues to mean "no incoming edge AND backfill couldn't pick it up" (rare).
- **Trade-off**: the flat-backfilled edge says "main-module depends on `<orphan>`" rather than the strictly-accurate per-transitive-parent attribution. This matches trivy/syft's behavior and is operationally more valuable for graph-walking SBOM consumers than honest-but-unreachable orphans.

#### Added

- 8 new unit tests in `compute_orphan_backfill` covering empty input, well-connected graphs (no backfill fires), single-orphan attribution, dedup against existing direct requires, cross-ecosystem incoming-edge counting, deterministic sort order, the real-world guac shape, and annotation-contract naming stability.

### CI: container-image release publishing fixes

Two latent bugs in PR #243's container-image plumbing surfaced when alpha.36's tag fired `release.yml` for the first time. Both are now fixed:

- **#248** — `mv mikebom-* staging` globbed both the tarball file AND the extracted directory, requiring `staging/` to pre-exist. Trailing slash on the glob (`mikebom-*/`) restricts to directories only.
- **#249** — pre-FROM `ARG TARGETARCH` in `Dockerfile` shadowed buildx's auto-populated per-platform value, expanding `${TARGETARCH}` to empty in the COPY step. Removed the redundant pre-FROM declaration (post-FROM ARG correctly picks up buildx's auto-value).

#### Notes on alpha.36

The alpha.36 retag chain left a stale empty GitHub Release at the `v0.1.0-alpha.36` tag (binaries are in a draft release 329550674 that couldn't be promoted due to softprops/action-gh-release's duplicate-release behavior). This is independent of the alpha.37 release pipeline — alpha.37's fresh tag will create a clean release page. The alpha.36 container image at `ghcr.io/kusari-sandbox/mikebom:v0.1.0-alpha.36` is published, signed, and working.

## [0.1.0-alpha.36] — 2026-05-26

This release bundles three PRs merged since alpha.35: the orphan-fix SPDX synth-root fallback gate (#244) plus two CI/feature deliveries — the multi-arch production container image (#243 / issue #234) and the registry credential extension for in-cluster operation (#242 / issue #235).

### SPDX synth-root fallback over-attached graph-roots under `--root-name`

Fixes a cross-format divergence the alpha.35 regen surfaced. When `--root-name` is active, the milestone-#229 alias rewrite at `generate/spdx/document.rs:458-465` already populates outgoing edges from the synthetic root SPDXID for every relationship originally sourced at the dropped manifest main-module's PURL. The #236 graph-root fallback (lines 483+) was firing on top of that — gated on `synthetic_root_added` alone — and adding extra `DEPENDS_ON` edges from synth-root to components mikebom couldn't link into the rest of the dep graph (Go `// indirect` entries the milestone-091 go.sum fallback couldn't inter-link under `--offline`; orphan npm packages from secondary `node_modules/` trees that lost their parent linkage during npm resolution). CDX's primary-dep fallback at `cyclonedx/dependencies.rs:74-78` is gated on `target_has_no_edges` symmetrically — it correctly skipped under `--root-name`, so CDX never had this problem.

**Behavior change:** the SPDX fallback now mirrors CDX's gate. It checks whether `synth_id` has any outgoing edges in the post-alias-rewrite `relationships` vec and only fires when there are none. Image scans, OS-package-only scans, and any other shape where `artifacts.relationships` contains no main-module-sourced edges → synth-id stays with zero outgoing edges → fallback still fires (image-scan synth root remains connected to its top-level packages, per #236's original intent). Override-active scans where the alias rewrite already populated outgoing edges → fallback skips → SPDX root edges now match CDX root edges component-for-component.

**Concrete impact on the alpha.35 regen results:**

- **guac** (`--root-name guac --root-version ebb808e`): SPDX root went from 441 DEPENDS_ON edges to 372 — drops the 70 Go `// indirect` entries that were over-attached. Now matches CDX (372 dependsOn). The 1 remaining diff vs CDX is the testify `TEST_DEPENDENCY_OF`-typed edge, which is the milestone-#228 by-design behavior.
- **Multi-`package.json` orphan-npm reproducer**: SPDX root went from 11 DEPENDS_ON edges to 4. The 7 orphan npm packages from `sub/node_modules/` are no longer over-attached. Now matches CDX (4 dependsOn).
- **kusari-cli**: unchanged (no over-attachment was happening; the alpha.35 regen showed 12/12 already).
- **`postgres:16` image scan**: unchanged (no override → alias map empty → synth-id has zero outgoing edges before fallback → fallback fires as before → 31 dependsOn / DEPENDS_ON in both formats, as alpha.35 already had).

#### Added

- **Two regression tests** in `generate/spdx/document.rs::tests` mirroring the alpha.35 reproducer shapes — one for the Go `// indirect` over-attachment scenario (synth root with main-module-aliased direct + orphan indirect → asserts only the direct gets attached), one for the orphan-npm scenario (same shape, different ecosystem). New test helpers `mk_main_module` (constructs a component carrying the `mikebom:component-role: main-module` annotation that the emitter drops under `--root-name`) and `mk_artifacts_with_override` (constructs `ScanArtifacts` with `root_override` populated) for use by these and future override-related tests.
- The pre-existing 3 #236 unit tests (`synthesized_root_has_outgoing_depends_on_to_graph_roots`, `synthesized_root_purl_preserves_colon_like_cdx`, `synthesized_root_excludes_already_depended_on_components_from_fallback`) continue to pass — the gate is purely additive on top of the existing logic; the image-scan-shaped scenarios these tests cover have empty alias maps and so satisfy the new `synth_has_outgoing == false` condition.

#### Known issue (separately tracked)

The orphan-npm-resolution gap that creates the "lost parent linkage" for `sub/node_modules/X` in the first place is a real and separate bug — the npm reader should re-parent secondary-tree dependencies to their actual graph parents rather than leaving them as top-level orphans. That fix is orthogonal to this PR (which only ensures the two formats agree on what to do with the already-orphan components). Filed as a follow-up issue.

### Issue #234 — Multi-arch production container image

Adds an official multi-arch (linux/amd64 + linux/arm64) container image published to `ghcr.io/kusari-sandbox/mikebom` per release. Image is signed with cosign keyless via Sigstore.

#### Added

- **`Dockerfile`** at repo root. Distroless base (`gcr.io/distroless/cc-debian12:nonroot`); ~25 MB final image. Runs as non-root user 65532 (uid). No shell, no package manager — Pod Security Standards "restricted" profile compatible. The image is assembled from the existing per-arch release tarballs (the same `cross`-compiled binaries published to GitHub Releases), not recompiled — so the binary inside the image is byte-identical to the tarball binary. Includes the eBPF object at the loader's expected relative path, so `mikebom trace` works inside the container when run with `CAP_BPF` + `CAP_PERFMON`.
- **`publish-container-image` job** in `.github/workflows/release.yml`. Triggers on every release tag (`v*-alpha.*` / `v*-beta.*` / `v*-rc.*`). Depends on `build-linux-x86_64` + `build-linux-aarch64`. Steps: download both tarballs → extract into per-arch staging dirs → `docker buildx` multi-arch build → push to GHCR → `cosign sign` keyless via OIDC. Multi-arch via QEMU + buildx; pinned action SHAs match the existing repo convention.
- **Three tags per release**: `ghcr.io/kusari-sandbox/mikebom:v0.1.0-alpha.X` (full git tag), `:0.1.0-alpha.X` (version without `v`), and `:latest` (moves with every alpha release until 1.0).
- **Cosign keyless signing**: every published image is signed against the GitHub OIDC issuer; consumers verify with `cosign verify --certificate-identity-regexp 'https://github.com/kusari-sandbox/mikebom/.+' --certificate-oidc-issuer https://token.actions.githubusercontent.com <image>`.
- **`docs/user-guide/installation.md`** new "Production container image" section with pull/run/verify examples and platform-portability notes.

#### Compatibility

- Existing `Dockerfile.dev` is unchanged and remains the recommended developer tool for eBPF + cross-compile workflows.
- No Rust code changes. CI lane only.

### Issue #235 — Registry credential extension for in-cluster operation

Extends the OCI registry credential resolution at `scan_fs/oci_pull/auth.rs` so mikebom can pull from private registries when running in environments without a Docker config file at the conventional `~/.docker/config.json` path — e.g. inside a Kubernetes CronJob pod where credentials arrive via `imagePullSecrets`-derived volume mounts or environment variables.

#### Added

- **`--registry-credentials-dir <PATH>` CLI flag** on `mikebom sbom scan`. Probes K8s secret-mount filenames in order: `config.json` (plain Docker), `.dockerconfigjson` (K8s `kubernetes.io/dockerconfigjson` secret type), `.dockercfg` (legacy K8s `kubernetes.io/dockercfg` secret type). First readable + parseable file wins; standard Docker config shape applies.
- **Env-var credential sources**:
  - **Per-registry**: `MIKEBOM_REGISTRY_<HOST>_USERNAME` + `MIKEBOM_REGISTRY_<HOST>_PASSWORD`, where `<HOST>` is the registry hostname uppercased with `[^A-Z0-9]` replaced by `_` (e.g. `ghcr.io` → `MIKEBOM_REGISTRY_GHCR_IO_USERNAME`; `my-ecr.amazonaws.com` → `MIKEBOM_REGISTRY_MY_ECR_AMAZONAWS_COM_USERNAME`).
  - **Generic**: `MIKEBOM_REGISTRY_USERNAME` + `MIKEBOM_REGISTRY_PASSWORD` — applies to every registry as a catch-all (useful when a cluster scan only ever hits one registry).
- **`resolve_credentials_layered`** entry point in `scan_fs/oci_pull/auth.rs` with documented precedence: env vars → `--registry-credentials-dir` → default Docker config. Existing `resolve_credentials` is unchanged — the new function wraps it.
- **8 new unit tests** in `auth.rs::tests` covering env-var resolution (per-registry, generic, partial-pair fallback, precedence), credentials-directory probing (config.json first, `.dockerconfigjson` fallback, malformed-skip-and-retry, empty-dir-returns-None). Environment-mutating tests serialize on a `Mutex<()>` matching the convention from `cache.rs` and `attestation/signer.rs`.
- **`docs/user-guide/cli-reference.md`** documents the new flag inline + the full 4-layer credential-resolution priority chain.

#### Security

- The `Credential` type's redacting `Debug` impl (`username` / `secret` → `<redacted>`) is preserved; the new env-var path doesn't introduce any logging that could leak credentials. Partial env-var configurations (USERNAME without PASSWORD or vice versa) are treated as no-credentials rather than synthesizing half-complete credentials. The `--registry-auth <registry>=<user>:<password>` CLI flag from Mario's spec is **deliberately not implemented** in this PR because credentials on the command line are visible to other processes via `/proc/<pid>/cmdline` and end up in shell history; env vars + secret-mount cover production cases cleanly.

#### Compatibility

- No behavior change for existing users. When neither `--registry-credentials-dir` nor any `MIKEBOM_REGISTRY_*` env var is set, the resolver falls through to the default Docker config path with the existing precedence (`credHelpers` > `credsStore` > `auths.<reg>.auth` > `auths.<reg>.identitytoken`).

## [0.1.0-alpha.35] — 2026-05-25

This release closes the C/C++ binary-SBOM defect cluster the reporter surfaced after alpha.34. Four PRs ship together: three bug fixes addressing edge-orphan and root-identity defects in the synthesized-root code paths (#237 / #239) and the cross-format scope-edge parity (#238); plus milestone 104 fixing the binary-role typing inversion that was the root of the reporter's "feels off" observation about `/bin/ls`.

### Milestone 104 — Binary role classification (Application vs Library)

Every binary discovered by the file-level binary reader (`mikebom-cli/src/scan_fs/binary/`) was historically emitted with CycloneDX `type: "library"` regardless of whether the file was an executable or a shared library. SBOM consumers reading the `type` field on `/bin/ls` saw `library` — the inverse of reality: `ls` is an application, not something other components link into. This milestone classifies every binary-reader-discovered component into one of four roles (`Application`, `SharedLibrary`, `Object`, `Other`) by reading the file's format header and maps the role to the format-native component-type field in each of CycloneDX, SPDX 2.3, and SPDX 3.

**Classification rules** (per the [binary-role cross-format contract](./specs/104-binary-role-classification/contracts/binary-role-cross-format-mapping.md)):

- **Mach-O**: `MH_EXECUTE` → Application; `MH_DYLIB` → SharedLibrary; `MH_OBJECT` → Object; `MH_BUNDLE` / `MH_KEXT_BUNDLE` / `MH_CORE` → Other.
- **ELF**: `ET_EXEC` → Application; `ET_DYN` with `PT_INTERP` program-header → Application (PIE executables, the modern Linux default); `ET_DYN` without `PT_INTERP` → SharedLibrary; `ET_REL` → Object; `ET_CORE` → Other.
- **PE**: `IMAGE_FILE_DLL` characteristic bit unset → Application; set → SharedLibrary; `IMAGE_FILE_SYSTEM` → Other.
- **Universal/fat Mach-O**: classification taken from the first slice's filetype (FR-006), matching the existing milestone-030 convention for identity metadata extraction.

**Format mapping**:

| Role          | CycloneDX 1.6 `Component.type` | SPDX 2.3 `Package.primaryPackagePurpose` | SPDX 3.0.1 `software_Package.software_primaryPurpose` |
|---------------|---------------------------------|------------------------------------------|------------------------------------------------------|
| Application   | `application`                   | `APPLICATION`                            | `application`                                         |
| SharedLibrary | `library`                       | `LIBRARY`                                | `library`                                             |
| Object        | `file`                          | `FILE`                                   | `file`                                                |
| Other         | `library` (historic default)    | _omitted_                                | _omitted_                                             |

Per Constitution Principle V, all three target formats have purpose-built native fields for component typing — the role rides exclusively through those standards-native slots. **No `mikebom:binary-role` annotation is introduced**; the existing `mikebom:binary-class` annotation (carrying format `elf`/`macho`/`pe`) is preserved as a distinct signal.

#### Added

- **`BinaryRole` enum** in `mikebom_common::resolution` with four variants (`Application` / `SharedLibrary` / `Object` / `Other`). Serde-renames as `snake_case` for stable wire-format serialization.
- **`binary_role: Option<BinaryRole>` field** on `ResolvedComponent` and `PackageDbEntry`. `Some(_)` for components from the binary reader; `None` for manifest- and lockfile-driven readers.
- **`mikebom-cli/src/scan_fs/binary/role.rs` module** containing the four-way classifier, the ELF `PT_INTERP`-based PIE disambiguation helper, and 9 unit tests covering each role variant per format.
- **CDX emission**: `binary_role_to_cdx_type` helper in `generate/cyclonedx/builder.rs` replaces the hardcoded `"type": "library"` literal.
- **SPDX 2.3 emission**: `primary_package_purpose` derivation in `generate/spdx/packages.rs` extended to honor `binary_role` ahead of the existing main-module-tagged → APPLICATION fallback. `Library` and `File` variants of `SpdxPrimaryPackagePurpose` lose their `#[allow(dead_code)]` attributes (first real uses).
- **SPDX 3 emission**: `software_primaryPurpose` derivation in `generate/spdx/v3_packages.rs` mirrors the SPDX 2.3 logic.
- **Parity catalog row A13** in `parity/catalog.rs` (auto-loaded from the new `sbom-format-mapping.md` row) plus per-format extractors `cdx_binary_role` / `spdx23_binary_role` / `spdx3_binary_role`, scoped to binary-reader-emitted components only (detected via the existing `mikebom:binary-class` property/annotation). Marked `SymmetricEqual` so all three formats must agree component-by-component (FR-008).
- **Tracing audit logs** at scan time for the ambiguous-fallback cases (FR-004): ELF ET_DYN PIE classification and Mach-O `Unknown` (bundle / kext) fallback. Lets operators audit unexpected role classifications without source-level debugging.
- **Two new integration tests** in `mikebom-cli/tests/`: `binary_role_parity.rs` (5 tests covering CDX, SPDX 2.3, SPDX 3 per-format role typing + cross-format agreement) and `binary_role_disambiguation.rs` (3 tests covering ELF PIE vs shared-library disambiguation, fat Mach-O first-slice classification per FR-006, and the MH_BUNDLE → Other → CDX library fallback).
- **`docs/reference/sbom-format-mapping.md`** new row A13 documenting the role-mapping table and pointing at the milestone's contract file. Constitution Principle V documentation requirement satisfied.

#### Changed

- The CycloneDX `Component.type` field for binary-reader-emitted components is now role-aware. Pre-milestone-104 every binary-reader component emitted `type: "library"`; post-fix executables emit `type: "application"`, shared libraries continue to emit `type: "library"`, object files emit `type: "file"`, and the `Other` bucket preserves the historic `"library"` default to avoid breaking consumers reading components the spec can't classify further.
- The SPDX 2.3 `primaryPackagePurpose` and SPDX 3 `software_primaryPurpose` fields are now populated for binary-reader-emitted Packages (pre-milestone-104 both were omitted unless the component carried the main-module annotation).
- No goldens regenerated. Per R4, no existing ecosystem fixture (cargo, gem, golang, maven, npm, pip, rpm, deb, apk, bazel, cmake) exercises the binary reader path — they're all manifest-driven, so the existing byte-identity goldens are unchanged.

### Issue #236 — image-scan SPDX root no longer orphaned + cross-format root PURL parity

Fixes two related defects in mikebom's emission of a *synthesized* root for scans that have no natural single root (container images, OS-package-only scans, `requirements.txt`- / `Gemfile`-only Python and Ruby projects, and any other case where milestone-053-style main-module annotation isn't set).

**The orphaned-root bug.** Before the fix, when the SPDX 2.3 / SPDX 3 emitters synthesized a placeholder root Package for the scan subject (via `synthesize_root` / `pick_root_iri`), the placeholder had no outgoing `DEPENDS_ON` / `dependsOn` edges. The synthetic root was only the target of the document-level `DESCRIBES` relationship; every top-level package the scan discovered was a graph-top with no incoming dependency edge. A consumer walking the dependency graph from the declared root saw zero direct deps. CDX has not had this problem since milestone 084 (closed via the primary-dependency fallback in `cyclonedx/dependencies.rs:74-99`, which synthesizes `metadata.component.bom-ref → <every component that nothing else depends on>` when the bom-ref has no outgoing edges). This fix mirrors that fallback into both SPDX emitters: when a root is synthesized, mikebom now emits one `DEPENDS_ON` / `dependsOn` edge from the synthesized root SPDXID/IRI to every graph-root component, preserving cross-format parity.

**The root-PURL divergence.** Before the fix, the synthesized-root PURL differed across the three formats for the same scan target. For `postgres:16`, CDX produced `pkg:generic/postgres:16@0.0.0` (via `encode_purl_segment`, which preserves the colon literal — matching the Debian / dpkg convention), SPDX 2.3 produced `pkg:generic/postgres_16@0.0.0` (via `sanitize_for_coord`, which collapses non-alphanumeric to `_`), and SPDX 3 produced `pkg:generic/postgres-16@0.0.0` (via `url_friendly`, which collapses non-alphanumeric to `-`). Three different root identities for the same image. This fix switches both SPDX synthesize-root paths to use `encode_purl_segment` for the PURL (matching CDX), while keeping the format-specific sanitizers for the CPE field (CPE has its own grammar rules). Post-fix, all three formats emit the identical root PURL `pkg:generic/postgres:16@0.0.0`.

**Verified.** Reconfirmed on `alpine:3`: CDX, SPDX 2.3, and SPDX 3 now all show `pkg:generic/alpine:3@0.0.0` as the root identity with 8 outgoing edges each (one per top-level apk package). Both bugs gone.

#### Added

- **Synthetic-root → graph-root `DEPENDS_ON` edges in SPDX 2.3** (`generate/spdx/document.rs:465-507` post `build_relationships` call). Fires only when `synthesize_root` runs; emits edges in deterministic PURL-lex order. Graph roots are defined as the same set CDX uses: components with `parent_purl: None` that aren't the target of any other dep edge.
- **Synthetic-root → graph-root `dependsOn` `Relationship` elements in SPDX 3** (`generate/spdx/v3_document.rs:609-647`, between containment and license relationships, before the final sort). Same graph-root definition as SPDX 2.3.
- **Three new unit tests in `generate/spdx/document.rs`** covering the synthesized-root PURL form (colon literal preserved), the synth-root-has-outgoing-edges invariant, and the "already-depended-on components are excluded from the fallback" subtlety that mirrors CDX's `cyclonedx/dependencies.rs:91-95` filter.

#### Changed

- **SPDX 2.3 synthesized-root PURL** switched from `sanitize_for_coord` (collapses `:` → `_`, lowercases) to `encode_purl_segment` (preserves the colon literal). CPE field keeps `sanitize_for_coord` — different grammar rules.
- **SPDX 3 synthesized-root PURL** switched from `url_friendly` (collapses `:` → `-`) to `encode_purl_segment`. CPE field keeps `url_friendly`.
- **SPDX 2.3 + SPDX 3 byte-identity goldens regenerated for `apk`, `bazel`, `cargo`, `cmake`, `deb`, `gem`, `pip`** (14 files, +270 lines, -0 lines — purely additive). Each diff is a constant set of synthetic-root → graph-root `DEPENDS_ON` edges. The other 8 SPDX 2.3 / SPDX 3 goldens (`golang`, `maven`, `npm`, `rpm`) are byte-identical to alpha.34 because their fixtures all have a main-module annotation and never hit the `synthesize_root` branch.
- **Parity-extractor coverage extended** (`parity/extractors/common.rs` + `parity/extractors/cdx.rs`). The pre-fix extractors deliberately skipped synthetic roots from the dep-edge buckets because those roots had no edges to walk; post-fix they're load-bearing edge sources, so the new helper `walk_cdx_components_main_module_and_synth_subject` (used by `cdx_dependency_edges`) and a synth-root inclusion in `spdx_relationship_edges` close that gap. The component-count extractors (A1–A12) still exclude synthetic roots — those rows count real components, not placeholders.
- **`transitive_parity_gem` baseline bumped 196 → 217** (fastlane has no top-level `.gemspec`, so synthesize_root fires).
- **`transitive_parity_pip_plain` baseline bumped 0 → 13** (the synthetic 13-pkg requirements.txt fixture). FR-008's "all 3 tools agree on zero" invariant moves to a soft warning — mikebom's CDX has been emitting these edges since milestone 084, and post-fix SPDX agrees with mikebom's own CDX. The `cross_tool_parity_check` continues to surface the divergence-from-Trivy/Syft as informational output.
- **`transitive_parity_pip_poetry` baseline bumped 62 → 88** (same root cause: poetry fixture lacks the main-module annotation, so synthesize_root fires).

### Issue #228 — SPDX 2.3 cross-format parity for scoped deps

Adds a `mikebom:lifecycle-scope` annotation to every scoped Package in SPDX 2.3 output and introduces a new `--spdx2-relationship-compat {full|basic}` CLI flag that selects the SPDX 2.3 relationship-type vocabulary the emitter uses for scoped dependency edges (dev / build / test).

**Background.** Milestone 052/part-2 removed the legacy `mikebom:dev-dependency` annotation from SPDX 2.3 emission on the grounds that the spec-native typed reversed-direction relationship variants — `DEV_DEPENDENCY_OF` / `BUILD_DEPENDENCY_OF` / `TEST_DEPENDENCY_OF` — already carried the same signal. That decision is still defensible per Constitution Principle V, but it leaves SPDX 2.3 consumers that walk only `DEPENDS_ON` (the Trivy / Syft convention, which covers most deployed SBOM-consumer tooling — verified by source-code review of `aquasecurity/trivy` `spdxRelationshipType()` and `anchore/syft` `lookupRelationship()`) unable to see scope-on-edge. Issue #228's reporter ran exactly that walk against `go.mod` projects with test deps (e.g. `testify`) and observed the SPDX-vs-CDX edge-count delta.

**Both modes are spec-conformant, but they are not equivalent.** The SPDX 2.3 spec defines `DEV_DEPENDENCY_OF`, `BUILD_DEPENDENCY_OF`, and `TEST_DEPENDENCY_OF` for exactly the purpose of expressing dev/build/test scope on a dependency edge — the spec's intent is that the most specific applicable field should be used. Constitution Principle X (Transparency) further requires mikebom to default to the spec-native mechanism that preserves the most consumer-actionable signal. **We want more transparency in SBOM output, not less.** Default `full` honors both: it emits the typed scoped variants the spec built for this purpose, and a consumer that implements the full SPDX 2.3 relationshipType enum sees the dev/build/test distinction directly on every scoped edge.

The new flag exists because some downstream consumers — notably Trivy and Syft, plus tooling built on top of them — only implement the basic relationship vocabulary (`DEPENDS_ON` / `CONTAINS` / `DESCRIBES`) and silently ignore the typed scoped variants the spec also defines. The default doesn't change mikebom's existing behavior: typed variants stay the SPDX 2.3 default emission (`--spdx2-relationship-compat=full`). What does change is that the target Package now ALSO carries a `mikebom:lifecycle-scope: "development" | "build" | "test"` annotation, so consumers walking a flat `DEPENDS_ON` view can still recover the dev/build/test distinction by inspecting the Package itself. The annotation is the same one CDX has carried since milestone 052/part-2 (parity-catalog row C42) — issue #228 simply extends it to SPDX 2.3 under Constitution Principle V's documented parity-gap carve-out. SPDX 3 is unaffected — it carries scope natively via `LifecycleScopedRelationship.scope` and intentionally does not emit the annotation.

`--spdx2-relationship-compat=basic` is an explicit downshift, not an equivalent alternative. When set, every dep — runtime, dev, build, test — emits as a natural-direction `DEPENDS_ON` edge, and scope info lives entirely on the Package annotation. Operators who pick `basic` accept information loss on the edge in exchange for compatibility with tooling that doesn't read the typed variants. That choice is fine when targeting Trivy / Syft / similar downstreams; it is the wrong default for SBOMs that should be maximally informative.

**Why this matters to SBOM consumers.** Knowing whether a component is dev-only, build-only, or test-only — vs. a deployed-runtime dependency — is consumer-critical signal. It is the difference between an actionable CVE on a shipped artifact and one that doesn't affect production. Tooling that can't distinguish these scopes will over-report risk against test deps (`testify`, `junit`, `criterion`, `mocha`) and under-report against deployed runtime deps. Defaulting to the spec-rich `full` mode AND emitting the Package annotation guarantees the signal is recoverable from the document in either compat mode.

#### Added

- **`--spdx2-relationship-compat <PROFILE>`** CLI flag on `mikebom sbom scan`. Accepts `full` (default — current behavior, spec-native typed variants) or `basic` (every dep as natural-direction `DEPENDS_ON`, for downstream tooling that only implements the basic SPDX 2.3 relationship vocabulary). Only affects the `spdx-2.3-json` format; CDX and SPDX 3 emission are unaffected.
- **`mikebom:lifecycle-scope` Package annotation in SPDX 2.3 output**, emitted for every non-runtime scoped Package regardless of compat mode. The annotation field name and `development` / `build` / `test` value set match the existing CDX `mikebom:lifecycle-scope` property (parity-catalog row C42).
- **Five new unit tests** covering the new code paths: `basic_compat_collapses_dev_to_depends_on` and `full_is_the_default_relationship_compat` in `generate/spdx/relationships.rs`; `lifecycle_scope_annotation_emitted_for_test_scope`, `lifecycle_scope_annotation_emitted_for_dev_and_build`, and `lifecycle_scope_annotation_omitted_for_runtime_and_none` in `generate/spdx/annotations.rs`.
- **`docs/reference/sbom-format-mapping.md`** rows B2 and C42 extended with the SPDX 2.3 emission story, the compat flag, and the consumer-importance rationale per Constitution Principle V's documentation requirement.
- **`docs/user-guide/cli-reference.md`** new `--spdx2-relationship-compat` section with examples.

#### Changed

- **SPDX 2.3 byte-identity goldens regenerated for `maven` and `bazel`** — the only two ecosystem fixtures in the suite that exercise non-runtime scopes (maven `<scope>test</scope>` on junit; Bazel `dev_dependency` rules). Diff is purely additive: one new `mikebom:lifecycle-scope` annotation per scoped Package, no edge-type changes (default mode unchanged). The other 9 SPDX 2.3 goldens (apk, cargo, cmake, deb, gem, golang, npm, pip, rpm) are byte-identical to alpha.34.
- **`spdx3_annotation_fidelity.rs::collect_spdx23`** updated to skip the `mikebom:lifecycle-scope` field when computing cross-format-fidelity diffs — the annotation is SPDX-2.3-only by design (SPDX 3 carries the same signal natively), so the fidelity test would otherwise flag an intentional asymmetry as drift.

### Milestone 073 — identifiers (built-in + user-defined)

Adds dedicated identifier flags (`--repo`, `--git-ref`, `--image-id`, `--attestation`, `--id <scheme>=<value>`) to `mikebom sbom scan` (path + image modes) and `mikebom trace run`, plus auto-detected identifiers on `--path` scans (from the git origin remote, with `upstream` + first-listed fallbacks per the 3-step Q1 algorithm) and `--image` scans (from the resolved image reference + manifest digest, in the `image:<registry>/<name>:<tag>@sha256:<digest>` canonical Q3 shape). Built-in schemes (`repo:`, `git:`, `image:`, `attestation:`) ride per-format standards-native carriers (CDX `metadata.component.externalReferences[]`, SPDX 2.3 dual-carrier on main-module `Package.externalRefs[PERSISTENT-ID]` + `creationInfo.creators` redundant text line, SPDX 3 `Element.externalIdentifier[]`); user-defined schemes (e.g., `acme_corp_id:`, `internal_ticket:`) ride a `mikebom:identifiers` document-level annotation per Constitution Principle V's documented-exception path. Sets up milestone 074's `--bind-to-source <identifier>` resolution path with no additional emission-side work needed at that point.

The original draft of this milestone called the feature "source identifiers" and shipped a single `--with-source <scheme>:<value>` flag. Both the name and the CLI surface were refactored before the milestone shipped: "source" anchored on the most-common case (source repos) but the same mechanism handles image / attestation / user-defined identifiers — the SPDX 3 spec already calls these `Element.externalIdentifier[]`, so the term was generalized to "identifier". Separately, the `<scheme>:<value>` syntax was visually ambiguous when values contained colons (URL ssh forms, image `@sha256:` digests) and operator-hostile despite being mechanically correct; dedicated flags per built-in scheme are self-documenting and unambiguous. Milestone-072's `SourceDocumentBinding` is a DIFFERENT concept (binding back to a source-tier SBOM document) and intentionally retains its name.

### Added

- **Dedicated identifier flags** on `mikebom sbom scan` (path + image modes) and `mikebom trace run` (FR-002):
  - `--repo <url>` — emits a `repo:` identifier
  - `--git-ref <revision>` (with `--repo`) — upgrades to a `git:<repo>#<revision>` identifier (supersedes the bare `repo:`)
  - `--image-id <ref>` — emits an `image:` identifier (named `--image-id` to avoid colliding with the existing `--image <PATH>` scan-input flag)
  - `--attestation <iri>` — emits an `attestation:` identifier
  - `--id <scheme>=<value>` (repeatable) — user-defined-namespace identifier; built-in scheme names (`repo`/`git`/`image`/`attestation`) are REJECTED at clap parse time with a message pointing the operator at the dedicated flag.

  Scheme regex `^[a-z][a-z0-9_-]*$` (FR-004) is enforced on `--id` schemes. Empty values rejected at clap parse time (`IdentifierError::EmptyValue`); malformed schemes rejected (`IdentifierError::InvalidSchemeName`). Built-in scheme value validation runs and may soft-fail to opaque pass-through under `mikebom:identifiers` (research.md §1 / VR-005) — same behavior as the pre-refactor flag.
- **Auto-detected `repo:` identifier on `--path` scans** (FR-001) — when the scan root is a git checkout, `git remote get-url <name>` runs with a 3-step name fallback per Q1 clarification: `origin` → `upstream` → first-listed-alphabetical. The chosen remote name is recorded in the standards-native carrier's comment field for transparency (FR-007). Auto-detection failure is `tracing::info!` only — never fails the scan.
- **Auto-detected `image:` identifier on `--image` scans** (FR-008) — synthesizes the canonical `image:<registry>/<name>:<tag>@sha256:<digest>` form from the resolved image reference + extracted manifest digest. Components are omitted when not available per the Q3 clarification (tarball mode without registry; pre-distribution-spec without digest).
- **Per-format standards-native carriers** for the 4 built-in schemes (FR-005) per Constitution Principle V — CDX `metadata.component.externalReferences[]` with per-scheme `type` mapping (`vcs` for `repo:`/`git:`, `distribution` for `image:`, `attestation` for `attestation:`); SPDX 2.3 dual-carrier on main-module `Package.externalRefs[PERSISTENT-ID]` (typed primary) + `creationInfo.creators` text lines (`Tool: mikebom-<version> source: <full-identifier>`, free-form fallback) per Q2 clarification; SPDX 3 `Element.externalIdentifier[]` on the `SpdxDocument` element (perfect fit for SPDX 3's open-typed multi-identifier model).
- **`mikebom:identifiers` document-level annotation** (parity-catalog row C47) — user-defined-namespace identifiers ride a single annotation envelope (`MikebomAnnotationCommentV1` reused from milestone 071). The `value` is a JSON array of `{scheme, value}` objects sorted lex by `(scheme, value)` for determinism (FR-009). Emitted ONLY when the user-defined entry set is non-empty (VR-007 — preserves cross-format byte-identity for non-user-defined-namespace scans). SPDX 3 carries user-defined identifiers natively in `Element.externalIdentifier[]`, NOT in this annotation.
- **Override semantics** (FR-006) — manual identifier flags override auto-detected identifiers on `(scheme, value)` match. Override-position rule per analyze F1: when manual deduplicates against auto-detected on `(scheme, value)`, manual inherits the auto-detected position; when manual overrides on different value (true override), auto-detected is dropped and manual follows in supply order (NOT promoted to front).
- **Parity catalog row C47** registered for `mikebom:identifiers` with `Directionality::SymmetricEqual`. The cross-format-parity test suite passes against the new row across CDX / SPDX 2.3 / SPDX 3 — user-defined identifiers byte-equivalent across formats after canonicalization.
- **`docs/reference/identifiers.md`** (FR-010) — published external-consumer guide. Covers wire format, CLI surface (with a migration note from the pre-073 `--with-source` draft), the 4 built-in schemes' per-format carriers, the user-defined-passthrough rule + `mikebom:identifiers` envelope shape, the auto-detection algorithm (3-step git fallback + `image:` synthesis), the determinism contract, runnable `jq` decode recipes per format, a Python reference extractor, the V1 stability commitment, and a forward pointer to milestone 074. SC-006 deliverable: external auditor writes a working extractor from this doc alone.
- **`docs/design-notes.md`** updated with a new "Identifiers (milestone 073)" section pointing operators at the published guide and explaining operator-visible behavior.
- **5 new integration tests** under `mikebom-cli/tests/`: `identifiers_emission.rs` (US1 — auto-detect happy path + 3-step fallback in all 3 formats), `identifiers_manual.rs` (US2 — manual flag emission + override + dedup + parse-time errors + soft-fail-to-opaque + the new built-in-scheme rejection on `--id`), `identifiers_per_tier.rs` (US3 — image-tier auto-detect + cross-tier consistency), `identifiers_determinism.rs` (US4 — deterministic emission + cross-format consistency).

### Migration

- The 27 alpha.15 byte-identity goldens for non-git fixtures stay byte-identical (no auto-detection fires when no git remote is present in fixtures). All `cdx_regression`, `spdx_regression`, and `spdx3_regression` byte-identity tests pass without regen.
- The dedicated identifier flags are opt-in. Operators not passing them see no behavior change beyond the new auto-detection paths (which only fire on git checkouts and `--image` scans).
- The `SpdxExternalRef` struct in `mikebom-cli/src/generate/spdx/packages.rs` adds an optional `comment: Option<String>` field. Existing constructions get `comment: None` and emit identical bytes to alpha.15.
- **Pipelines that prototyped against the original `--with-source` draft must update**: there is no compatibility shim. Replace `--with-source repo:<url>` with `--repo <url>`, `--with-source image:<ref>` with `--image-id <ref>`, `--with-source attestation:<iri>` with `--attestation <iri>`, `--with-source acme_corp_id:abc` with `--id acme_corp_id=abc` (note `=` separator on `--id`, not `:`).

### Out of scope (deferred to milestone 074)

- **Identifier-keyed `--bind-to-source` resolution** — passing `--bind-to-source repo:git@...` and having mikebom find the matching source SBOM file path via a local lookup directory. Milestone 074's scope. This milestone (073) emits identifiers in a shape 074's resolution layer can consume; no emission-side work needed at 074 land time.

## [0.1.0-alpha.15] — 2026-05-05

The **milestone 072 closure release.** Three sequential PRs (#140, #141, #142) shipped end-to-end cross-tier SBOM binding. With this release, an operator can: (a) emit image-tier SBOMs with verifiable cross-tier binding metadata; (b) verify the binary in the image matches the source SBOM via `mikebom sbom verify-binding`; (c) propagate VEX statements safely from source to image, with binding-strength-aware caveats by default; (d) triage which source SBOM (if any) describes a build that produced an image-tier component via `mikebom sbom trace-binding`; (e) hand external auditors `docs/reference/cross-tier-binding.md` and the binding-fixtures/ reference set so they can write their own verifier without mikebom source-code access.

The user's two specific worries are both addressed: "we cannot verify the binary running in the image matches the version that the source or build SBOM is built for" → `verify-binding`; "a vulnerability in the source has a VEX against it but the image actually has the vuln through some other path" → per-instance VEX with `caveated`-mode propagation default.

### Milestone 072 — cross-tier SBOM binding (closed by PR-C)

With PR-C merged, **milestone 072 is fully closed**: US1 (verify image's foo == source's foo) + US2 (VEX propagation respects binding strength) + US3 (operator triage via `trace-binding`) + the published verifier guide. The three-PR sequence (PR-A foundation + US1, PR-B US2, PR-C US3 + docs) implements every requirement FR-001..FR-012, satisfies every success criterion SC-001..SC-008, and lands the SC-004 published reference fixture set + external-verifier guide that an external auditor can write a working verifier from with zero source-code access.

### Added (milestone 072 PR-C — operator triage + published verifier guide: US3)

Third and final PR closing out milestone 072. PR-A delivered the foundation + verification (`mikebom sbom verify-binding`). PR-B delivered VEX propagation respecting binding strength. PR-C delivers **US3** (operator triage) and the **FR-010 published verifier guide**.

User-visible scope this PR:

- **`mikebom sbom trace-binding`** subcommand (FR-006) — operator triage tool answering "which source-tier SBOM (if any) corresponds to this image-tier component?". Args: `--component-purl <purl>`, `--image-sbom <path>`, EITHER `--source-sbom <path>` OR `--candidate-sources-dir <dir>` (mutually exclusive), `--format {table,json}` (default `table`). Reports per-instance binding state for every instance of the supplied PURL in the image SBOM. **Always exits 0** (informational, not validating — contrast with `verify-binding` which exits non-zero on hash mismatch). Mirrors the JSON output shape from `quickstart.md` Recipe 6.
- **`mikebom-cli/tests/binding_trace.rs`** — 3 integration tests covering the Recipe 6 scenarios: (a) component with one bound instance → `verified` with the bound source ID, (b) component with no candidate match → `unknown` with `reason: "source-not-found-in-bind-target"`, (c) two instances (one bound + one unbound) → both returned with their respective binding states.
- **`docs/reference/cross-tier-binding.md`** (FR-010) — comprehensive published guide for external verifier authors. **The SC-004 deliverable**: an external auditor can write a working verifier from this document alone and validate ≥95% of bindings against the published reference fixture set. Sections cover the binding-hash-v1 algorithm with worked examples for all three strength outcomes, per-ecosystem input-table (cargo / npm / pip / gem / maven / golang), per-format carrier shapes (CDX `properties[]`, SPDX 2.3 `MikebomAnnotationCommentV1` envelope, SPDX 3 `Annotation.statement`, plus standards-native `externalReferences` / `externalDocumentRefs` / `built_from` siblings), the OpenVEX `Product.identifiers` per-instance extension contract, the three VEX propagation modes plus the `affected ⊕ unbound-and-not-explicitly-vexed = affected` aggregation rule, a runnable Python verifier reference implementation (~150 lines, standard-library only), the V1 stability commitment + algo-version policy, and a pointer to the published reference fixtures for verifier-author acceptance testing. Mirrors the milestone-071 `conformance-harness-guide.md` structural model.
- **`docs/design-notes.md`** updated with a new "Cross-tier SBOM binding (milestone 072)" section pointing operators at the published guide and explaining the operator-visible behavior — when to use `--bind-to-source`, what `verify-binding` / `trace-binding` answer (validation vs. triage), and the migration path for operators using `--vex-overrides` today.

### Migration

- No SBOM output shape change for callers that don't use the milestone-072 commands. All 27 alpha.14 byte-identity goldens remain byte-identical.
- `trace-binding` is purely additive — a new read-only subcommand that doesn't modify any SBOM.
- The published `cross-tier-binding.md` guide is documentation-only; it has no runtime impact.

### Added (milestone 072 PR-B — VEX propagation respects binding strength: US2)

Second of three sequential PRs implementing milestone 072. PR-A delivered the foundation + verification (`mikebom sbom verify-binding`). PR-B delivers **US2**: cross-tier VEX propagation that respects the binding strength PR-A established. PR-C will add operator triage (`mikebom sbom trace-binding`) + the published verifier guide.

User-visible scope this PR:

- **`mikebom sbom enrich --vex-propagation-mode {permissive,caveated,strict}`** (FR-007) — new flag with default `caveated`. Wires real VEX propagation (replacing the pre-072 no-op `--vex-overrides` stub). Pre-072 callers needing exact prior behavior pass `--vex-propagation-mode permissive`. The flag's help text + the `mikebom sbom enrich --help` output document the breaking-change opt-out.
- **`propagate_vex_with_binding`** engine in `mikebom-cli/src/sbom/mutator.rs` — handles all three modes, applies per-`Directionality` matching (one-to-one when source statement carries `Product.identifiers.cyclonedx-bom-ref` / `spdx-spdxid`; one-to-many broadcast when source is pre-072 PURL-only). Emits per-instance `affects[]` entries on the target CDX `vulnerabilities[]` array.
- **Per-instance VEX emission** (FR-008) — propagated OpenVEX statements populate `OpenVexProduct.identifiers` with the target instance's `cyclonedx-bom-ref` / `spdx-spdxid` keys. Pre-072 OpenVEX consumers see byte-identical wire shape (the field is `skip_serializing_if_empty` from PR-A); post-072 consumers can apply VEX statements at instance granularity.
- **`mikebom:vex-binding-status: unverified` caveat** (FR-009) — when `caveated` mode propagates onto a non-`verified` instance, every `affects[]` entry on the target CDX `vulnerabilities[]` row carries this sibling annotation per `contracts/openvex-instance-identifiers.md` C-5. Operators reading the SBOM see exactly which propagated statements lack verified bindings.
- **Refusal-rationale annotations** — `strict` mode refuses to propagate onto non-`verified` instances. The refused (vulnerability, instance) pairs are recorded under a new `mikebom:vex-propagation-refusals` document-level property carrying a structured per-refusal record (`vulnerability`, `purl`, `bom_ref`, `binding_strength`, `reason`). The command exits non-zero per VR-006 so CI pipelines can gate on strict-mode outcomes; the SBOM is still written so operators can audit the refusal rationale.
- **`mikebom-cli/tests/binding_drift.rs`** — 2 tests: strict-mode refusal on weak binding (exit non-zero, no `vulnerabilities[]` entry, refusal annotation present); strict-mode acceptance on verified binding.
- **`mikebom-cli/tests/vex_per_instance.rs`** — 1 test, the canonical worked-example regression (US2 AS#4 / SC-003): two instances of `pkg:golang/golang.org/x/net@v0.28.0` (verified-bound + unbound), `caveated`-mode propagation correctly produces clean propagation on the bound instance and caveated propagation on the unbound instance.

### Changed — VEX propagation default flips from no-op to `caveated`

- Pre-072: `mikebom sbom enrich --vex-overrides <path>` was a documented no-op — the legacy flag did nothing. Post-072: same flag combination triggers real propagation in `caveated` mode by default. Callers depending on the no-op behavior pass `--vex-propagation-mode permissive` to disable binding-strength-aware caveats. Strictly speaking, this is a behavior change rather than a "breaking change in output that previously existed" — pre-072 there was no propagation output at all. Documented in spec SC-008 + the `--vex-propagation-mode` flag's help text.

### Migration

- No SBOM output shape change for callers that don't pass `--vex-overrides`. The 27 alpha.14 byte-identity goldens remain byte-identical.
- Callers passing `--vex-overrides` previously got nothing in the output; now they get propagated VEX statements with binding-aware caveats by default.

### Out of scope (deferred to PR-C)

- `mikebom sbom trace-binding` operator-triage subcommand (FR-006 / US3) — PR-C.
- `docs/reference/cross-tier-binding.md` published verifier guide (FR-010) — PR-C, lands when the full contract surface is implemented.

### Added (milestone 072 PR-A — cross-tier SBOM binding: foundation + US1)

This is the first of three sequential PRs implementing milestone 072. PR-A delivers the **foundational binding contract + US1** (verify image's foo == source's foo). PR-B will add US2 (`mikebom sbom enrich --vex-propagation-mode` + per-instance OpenVEX). PR-C will add US3 (`mikebom sbom trace-binding`). User-visible scope this PR:

- **`mikebom-cli/src/binding/`** — new module owning the layered binding-hash algorithm (FR-002), per-component `SourceDocumentBinding` annotation shape (FR-001), per-ecosystem source-input extraction (cargo / npm / pip / gem / maven / golang per research.md §1), and consumer-side verification logic (FR-005). Public re-exports include `compute_binding_hash`, `extract_source_inputs_for_component`, `verify_binding`, plus the data types (`BindingHashInputs`, `BindingHash`, `BindingStrength`, `SourceDocumentId`, `SourceDocumentBinding`, `VexPropagationMode`).
- **`mikebom sbom scan --bind-to-source <path>`** flag (FR-011) — image-tier scans loaded with this option resolve the named source-tier SBOM and emit per-component `mikebom:source-document-binding` annotations carrying the layered-hash + `BindingStrength` (verified / weak / unknown) labels. Non-`verified` components carry a structured `reason` per FR-003. `--path` scans (source-tier) warn-and-skip emission to preserve alpha.14 source-tier byte-identity.
- **`mikebom sbom verify-binding`** subcommand (FR-005) — given an image-tier SBOM and a source-tier SBOM, recomputes per-component binding hashes from the source-tier inputs and reports verification pass/fail. `--format {table,json}` (default `table`); exits non-zero on any verification failure per VR-005.
- **Standards-native cross-document references** per Constitution Principle V (FR-004) — CDX `metadata.component.externalReferences[type:bom]`, SPDX 2.3 `externalDocumentRefs[]` + `BUILT_FROM` relationship, SPDX 3 `import[] ExternalMap` + `Relationship[built_from]` graph element. Every cross-tier reference rides through standards-native fields; only the per-component hash + strength label live in the `mikebom:source-document-binding` annotation.
- **`OpenVexProduct.identifiers: BTreeMap<String, String>`** field added at `mikebom-cli/src/generate/openvex/statements.rs:71` per contracts/openvex-instance-identifiers.md C-1. The field is `skip_serializing_if_empty` — pre-072 wire shape preserved for callers that don't populate it. PR-B will populate `cyclonedx-bom-ref` / `spdx-spdxid` keys at propagation time when both source-VEX and target-SBOM are paired.
- **Parity catalog row C46** registered for `mikebom:source-document-binding` with `Directionality::SymmetricEqual` per milestone-071's invariant. The cross-format-parity test suite passes against the new row across all 9 ecosystem fixtures × 3 formats — image-tier emission is symmetric across CDX / SPDX 2.3 / SPDX 3.
- **Reference fixture set** at `docs/reference/binding-fixtures/` (SC-004) — three fixture pairs (`cargo-verified`, `golang-verified`, `maven-weak`) with pinned input triples + expected SHA-256 hex values. External verifiers writing their own implementations use these as the published reference set.
- **Algo v1 contract pinned** by 3 unit-test pinned-vector cases per analyze-C2 / SC-007 — future canonicalization changes break these tests, surfacing version-drift before consumers see it.

### Migration

- No SBOM output shape change for source-tier scans. The 27 alpha.14 byte-identity goldens remain byte-identical.
- Image-tier scans that don't pass `--bind-to-source` are byte-identical to alpha.14.
- `--bind-to-source` is opt-in; absent the flag, image-tier scans emit no binding annotations.

### Out of scope (deferred to PR-B / PR-C)

- VEX propagation logic (FR-007 / `--vex-propagation-mode`) is **PR-B**. The `OpenVexProduct.identifiers` field exists in PR-A but is empty at every emit-site. Pre-072 OpenVEX consumers see byte-identical output.
- Per-instance VEX emission carrier population (FR-008) is **PR-B**.
- `mikebom sbom trace-binding` operator-triage subcommand (FR-006 / US3) is **PR-C**.
- `docs/reference/cross-tier-binding.md` published verifier guide (FR-010) lands when the full contract surface is implemented (PR-C). The contract is fully specified in `specs/072-cross-tier-sbom-binding/contracts/` already and external implementers can follow those today.

## [0.1.0-alpha.14] — 2026-05-04

The **conformance-tooling polish release.** Two user-visible
improvements since alpha.13: granular network-enrichment skip
flags for large-scale users, and a real value-equality upgrade
to the `mikebom sbom parity-check` subcommand backed by a
comprehensive conformance-harness-author guide.

### Added (PR #136 — granular enrichment control flags)

- Three new flags on `mikebom sbom scan` give operators sub-`--offline` control over which post-scan enrichment sources fire: `--no-clearly-defined` (skip ClearlyDefined; deps.dev still active), `--no-deps-dev-graph` (skip deps.dev transitive dep-graph; license enrichment stays active), `--enrich-sources <list>` (allowlist mode overriding the `--no-*` flags). `--offline` retains its all-network-off semantics. Motivation: ClearlyDefined enrichment can dominate wall-clock on 1000+-component scans (~6+ minutes / ~87% of total scan time); these flags give large-scale users a finer-grained escape hatch than `--offline`. Underlying CD performance gap tracked separately as #137.

### Added (milestone 071 — cross-format SBOM annotation parity)

- **Conformance harness author guide** at `docs/reference/conformance-harness-guide.md` — a reference for external SBOM-conformance harness maintainers explaining how mikebom carries `mikebom:*` metadata in each of the three supported formats (CDX 1.6 / SPDX 2.3 / SPDX 3.0.1), the 7 inherent format-spec asymmetries that should NOT be flagged as cross-format-inequivalence findings, and how to wire a harness to read the `MikebomAnnotationCommentV1` envelope correctly. Authored against milestone 071's catalog state.
- **Synthetic drift regression test** at `mikebom-cli/tests/parity_synthetic_drift.rs` — pins the post-071 value-equality semantics by constructing a synthesized SBOM triple where a `SymmetricEqual` row's set CONTENTS differ across formats. Asserts the post-071 logic catches the drift; demonstrates the pre-071 presence-only logic would have silently passed it.
- `ParityExtractor` struct gains a `pub order_sensitive: bool` field at `mikebom-cli/src/parity/extractors/common.rs` for future order-sensitive annotation rows. Default `false` for all 68 currently-catalogued rows; rationale: every currently-named key is an unordered set under the existing `BTreeSet<String>` extractor model.
- `canonicalize_for_compare(value: &Value, order_sensitive: bool) -> String` helper at the same path — sorts object keys lex, sorts arrays lex (default) or preserves order (override), normalizes whitespace via `serde_json::to_string`. Available for future per-row value-payload comparison work.

### Changed (milestone 071 — `mikebom sbom parity-check` upgrade)

- **`mikebom sbom parity-check` now does real value-equality checking** instead of presence-only checking. Pre-071 the subcommand reported `Parity gaps: 0` whenever all three formats had ≥1 entry per universal-parity row, regardless of whether the actual set CONTENTS matched across formats. Post-071 it applies the per-`Directionality` invariants: set equality for `SymmetricEqual`, `cdx_set ⊆ spdx23/3` for `CdxSubsetOfSpdx`, presence-parity for `PresenceOnly`, CDX-non-empty for `CdxOnly`. The same logic the canonical `tests/holistic_parity.rs` integration test uses; the CLI subcommand and the integration test now return the same verdict.
- The presence-only undercounting of unexercised rows is also fixed — universal-parity rows where no format carries data are now correctly counted as "passing by default" rather than "neither passing nor failing." Typical output on a small-fixture cargo-workspace scan goes from `Universal-parity rows: 16 / 67 ✓` (pre-071) to `Universal-parity rows: 67 / 67 ✓` (post-071), reporting the same number of real gaps (zero) but with cleaner accounting.
- **Harness implication**: external conformance harnesses that shell out to `mikebom sbom parity-check` to validate cross-format parity were missing real value-drift bugs pre-071. Upgrade to alpha.14 or later for the rigorous check.

### Migration

- No SBOM output shape change. CDX 1.6 / SPDX 2.3 / SPDX 3.0.1 emissions are byte-identical to alpha.13 (all 27 byte-identity goldens unchanged).
- No new `mikebom:*` annotation keys; no removed keys; no directionality changes in the parity catalog. Milestone 071 is purely an internal-verification + documentation milestone.

### Fixed (CI hardening — realistic-projects lane)

- The online knative-func scan step in the realistic-projects CI lane (`.github/workflows/realistic-projects.yml`) now skips ClearlyDefined + deps.dev-graph enrichment via `--no-clearly-defined --no-deps-dev-graph` (the new #136 flags). The step's purpose is exclusively the Go transitive-edge resolver gate (SC-003 from milestone 055); neither CD nor deps.dev-graph contributes to Go transitive edges. When CD's public API has a slow event (knative-func has 1000+ components and CD is ~87% of online wall-clock per #137), the lane was hitting the 15-minute job timeout. Skipping the irrelevant enrichment passes keeps the assertion focused and the job under budget. Fix-forward: #137 captures the underlying CD-perf work that would let the lane re-enable full enrichment without timing out.

## [0.1.0-alpha.13] — 2026-05-03

The **issue #104 closure release.** Three milestones since alpha.12
ship the final three per-ecosystem main-modules — pip, gem, and
maven — completing the project-self-identity coverage matrix
across every ecosystem mikebom supports.

### Changed (BREAKING — SBOM output shape, milestone 070 — closes #104 in full)

- **Maven project SBOMs now identify the project itself** via a
  synthetic main-module component for every project with a
  top-level `pom.xml`. Pre-070: maven SBOMs had no project-self
  component. Post-070: every Maven project emits a
  `pkg:maven/<groupId>/<artifactId>@<version>` component placed
  in standards-native "BOM subject" slots — CDX
  `metadata.component`, SPDX `primaryPackagePurpose: APPLICATION`
  plus `documentDescribes`, SPDX 3.0.1
  `software_primaryPurpose: application`. Carries
  `mikebom:component-role: main-module` (C40) supplementary
  signal per Constitution Principle V.

- **Multi-module reactor support (FR-002).** When the top-level
  `pom.xml` declares `<modules>`, mikebom emits one main-module
  component per submodule (and per the parent reactor POM
  itself) — leveraging the multi-DESCRIBES super-root
  infrastructure from milestone 064/#127. Each submodule POM is
  resolved through POM inheritance: missing `<groupId>` and
  `<version>` are filled from the `<parent>` block (FR-001
  step 2).

- **Property substitution (FR-012).** GAV coordinates containing
  Maven property references (`${project.version}`, `${revision}`,
  `${parent.groupId}`, custom keys defined in `<properties>`)
  are resolved at parse time. Unresolvable properties fall
  through verbatim with `tracing::warn!`, matching the
  cross-host determinism convention.

- **Install-state path exclusion (FR-003).** `pom.xml` files
  inside `target/`, `.m2/`, `node_modules/`, `vendor/` are NOT
  discovered for main-module emission. Those paths are handled
  by the existing dep-emission walker.

- **Same-PURL collision dedup** with operator-visible
  `tracing::warn!` per the cargo (064) / npm (066) / pip (068) /
  gem (069) Q1 convention.

### Migration (070)

- Consumers reading `metadata.component.purl` for Maven scans
  now receive `pkg:maven/<g>/<a>@<v>` instead of the pre-070
  `pkg:generic/...` placeholder.
- **Per-ecosystem main-module coverage matrix is now COMPLETE**:
  Go ✅ (053), cargo ✅ (064), npm ✅ (066), pip ✅ (068, PEP 621),
  gem ✅ (069, top-level `*.gemspec`), maven ✅ (070, top-level
  `pom.xml` + reactor + inheritance + property substitution).
  **Issue #104 fully closed.**

### Changed (BREAKING — SBOM output shape, milestone 069)

- **Ruby gem project SBOMs now identify the project itself** via a
  synthetic main-module component for every project with a
  top-level `*.gemspec`. Pre-069: gem SBOMs had no project-self
  component. Post-069: every Ruby gem project emits a
  `pkg:gem/<name>@<version>` component placed in standards-native
  "BOM subject" slots — CDX `metadata.component`, SPDX
  `primaryPackagePurpose: APPLICATION` plus `documentDescribes`,
  SPDX 3.0.1 `software_primaryPurpose: application`. Carries
  `mikebom:component-role: main-module` (C40) supplementary
  signal per Constitution Principle V.

- **Skip rule for application-style projects.** Per FR-002, Ruby
  projects with only `Gemfile` + `Gemfile.lock` (no top-level
  `*.gemspec`) emit NO main-module. Application-style projects
  don't have a project-self identity in the gem ecosystem; the
  existing `Gemfile.lock` dep emission is unaffected. This is
  gem-specific because Ruby explicitly distinguishes publishable
  gems (`*.gemspec` declares the gem) from applications
  (Gemfile-based dep management only).

- **Install-state path exclusion (FR-003).** `*.gemspec` files
  inside `vendor/`, `gems/`, `specifications/`, `.bundle/` are
  NOT discovered for main-module emission. Those paths are
  handled by the existing dep-emission walker.

- **Pure-Rust regex parsing.** Mikebom never executes the
  `*.gemspec` as Ruby code (Constitution Principle I). Literal-
  string assignments (`s.name = "foo"`, `s.version = "1.2.3"` —
  with optional `.freeze` chaining) are extracted; constant
  references (`s.version = Foo::VERSION`) and dynamic
  computations fall through to the `0.0.0-unknown` placeholder
  per the cross-host determinism convention from milestones
  053/064/066/068.

- **Same-PURL collision dedup** with operator-visible
  `tracing::warn!` per the cargo (064) / npm (066) / pip (068)
  Q1 convention.

### Migration

- Consumers reading `metadata.component.purl` for Ruby gem scans
  now receive `pkg:gem/<name>@<version>` instead of the pre-069
  `pkg:generic/...` placeholder.
- Per-ecosystem main-module coverage matrix: Go ✅ (053),
  cargo ✅ (064), npm ✅ (066), pip ✅ (068, PEP 621), gem ✅ (069,
  top-level `*.gemspec`); maven tracked in #104 (last remaining
  slice).

### Changed (BREAKING — SBOM output shape, milestone 068)

- **Python project SBOMs now identify the project itself** via a
  synthetic main-module component for every `pyproject.toml`
  containing PEP 621 `[project]` table. Pre-068: Python SBOMs had
  no project-self component. Post-068: every Python project scan
  emits a `pkg:pypi/<name>@<version>` component (with PEP 503
  name normalization — lowercase + underscore→hyphen) placed in
  standards-native "BOM subject" slots — CDX `metadata.component`,
  SPDX `primaryPackagePurpose: APPLICATION` plus
  `documentDescribes`, SPDX 3.0.1
  `software_primaryPurpose: application`. Carries
  `mikebom:component-role: main-module` (C40) supplementary
  signal per Constitution Principle V.

- **Skip rule for `[tool.poetry]`-only manifests.** Per issue
  #104's explicit guidance, Python projects using the pre-PEP-621
  Poetry schema (no `[project]` table) are skipped from main-
  module emission. Existing Poetry lockfile-driven dep emission
  is unaffected. `tracing::info!` notes the skip with a pointer
  to a future Poetry-extension follow-up issue. Manifests with
  BOTH `[project]` AND `[tool.poetry]` (Poetry 1.5+ shim case)
  emit from `[project]` — the standards-native PEP 621 source
  wins.

- **Editable-install merge precedence (FR-011).** When a venv
  `.dist-info` shares the same PURL as the milestone-068 main-
  module emitted from the project's own `pyproject.toml`, the
  augment-existing-entry logic preserves venv evidence
  (`mikebom:sbom-tier: deployed`, hashes from METADATA) while
  layering Phase A's C40 tag + `parent_purl: None` on top. The
  resulting main-module has both signals: project identity
  (Phase A) + installation evidence (venv). This is unique to
  pip — no equivalent in cargo/npm because their installation
  models differ.

- **`dynamic = ["version"]` → `0.0.0-unknown` placeholder.**
  When a PEP 621 manifest defers `version` resolution to
  setuptools-scm or similar, mikebom emits the literal
  `0.0.0-unknown` placeholder rather than shelling out to a
  Python toolchain. Cross-host determinism + zero-dependency
  posture preserved per the convention from milestones
  053/064/066.

- **PEP 508 dep-name extraction.** Direct deps from
  `[project.dependencies]` and `[project.optional-dependencies]`
  emit edges from the main-module via the same
  `name_to_purl`-resolution + dangling-target-drop convention
  as cargo + npm. PEP 508 markers, version specifiers, and
  extras are stripped — only the package name is used for edge
  resolution.

- **Same-PURL collision dedup** with operator-visible
  `tracing::warn!` per the cargo (064) / npm (066) Q1
  convention. Rare given `__pycache__/`, `.venv/`, `site-packages/`
  are excluded from manifest discovery.

### Migration

- Consumers reading `metadata.component.purl` for Python scans
  now receive `pkg:pypi/<pep503-name>@<version>` instead of the
  pre-068 `pkg:generic/...` placeholder.
- Per-ecosystem main-module coverage matrix: Go ✅ (053),
  cargo ✅ (064), npm ✅ (066), pip ✅ (068, PEP 621 only —
  Poetry-only `[tool.poetry]` deferred), maven / gem tracked in
  #104.

### Known gaps (filed for follow-up)

- **#104 Poetry coverage**: `[tool.poetry]`-only manifests
  currently skipped per #104's explicit guidance. If demand
  surfaces, a follow-up issue will extend the reader.
- **#103** — LICENSE detection for the new pip main-module
  (PEP 621 `[project].license` field, classifiers, LICENSE-file
  matching).
- **#125** — divergent-PURL detection extends to pip too.



## [0.1.0-alpha.12] — 2026-05-03

A focused per-ecosystem release expanding the main-module pattern
shipped for Go in alpha.10/053 to two more ecosystems (cargo + npm),
plus closing a pre-existing cargo workspace-root edge-emission gap
and a deps.dev enrichment perf bug surfaced during 064 review.

Three milestones since alpha.11 (~1 day later):

- **064**: cargo source-tree main-module component (closes the
  cargo slice of #104). Each `Cargo.toml` with `[package]` emits
  a `pkg:cargo/<name>@<version>` component in standards-native
  BOM-subject slots. Includes the multi-main-module super-root +
  plural-DESCRIBES wiring (#127) shipped as a side-fix —
  workspace scans correctly surface every member through
  `documentDescribes`/`rootElement`.
- **065**: cargo workspace-root direct-dep edges (closes #126).
  Pre-065 cargo SBOMs had a project-self component but no
  outgoing dep tree because `parse_lockfile` was skipping
  workspace-root entries. Post-065, those entries' `dependencies
  = [...]` declarations are harvested separately and merged into
  the milestone-064 main-module's `depends` field. Completes
  064's FR-007. Bundled with a deps.dev seed-skip for
  empty/`unknown`-version components — surfaced when knative-func
  realistic-project CI hit the 15-minute job timeout from dozens
  of guaranteed-404 maven calls.
- **066**: npm source-tree main-module component (closes the npm
  slice of #104). Each `package.json` with `name` (and not
  `private: true` + no version per #104's guidance) emits a
  `pkg:npm/<name>@<version>` (or `%40scope/name` encoded)
  component. Workspace handling extends the cargo pattern; no
  generator-side changes needed thanks to the C40-tag-driven
  hooks established in 053+064+#127.

### Changed (BREAKING — SBOM output shape, milestone 066)

- **npm SBOMs now identify the project itself** via a synthetic
  main-module component for every `package.json` with `name`.
  Pre-066: npm SBOMs had no project-self component. Post-066:
  every Node.js project scan emits a `pkg:npm/<name>@<version>`
  (or `pkg:npm/%40<scope>/<name>@<version>` for scoped names)
  component placed in standards-native "BOM subject" slots —
  CDX `metadata.component`, SPDX `primaryPackagePurpose:
  APPLICATION` plus `documentDescribes`, SPDX 3.0.1
  `software_primaryPurpose: application`. Carries
  `mikebom:component-role: main-module` (C40) supplementary
  signal per Constitution Principle V.

- **Skip rule for `private: true` + no `version`.** Per issue
  #104's explicit guidance, manifests with `private: true` AND
  no declared `version` are skipped from main-module emission —
  the author has signaled "not a publishable artifact." Common
  pattern: monorepo workspace roots. `private: true` + a
  declared `version` still emits (the flag is a publish guard,
  not an SBOM-presence signal).

- **Workspace handling.** npm 7+ `workspaces: ["packages/*"]`
  arrays are honored: each member `package.json` emits its own
  main-module. Workspace path-deps (`"<member>": "*"`) emit
  member-to-member `dependsOn` edges. `documentDescribes` is
  multi-target (one SPDXID per member, alphabetically sorted)
  via the milestone-064-#127 infrastructure that ships
  unchanged for npm.

- **Scoped name encoding.** `@scope/name` PURLs URL-encode the
  `@` to `%40` per PURL spec (`pkg:npm/%40scope/name@version`),
  reusing the existing `build_npm_purl` helper.

- **Same-PURL collision dedup.** When two-or-more `package.json`
  files yield identical PURLs (rare given `node_modules/` is
  excluded from manifest discovery, but defensive), exactly one
  main-module emits and a `tracing::warn!` lists dropped
  duplicate paths. Same convention as cargo (064) per spec
  Clarifications Q1.

- **No version-inheritance feature, no resolver ladder.** Unlike
  cargo's `version.workspace = true`, npm has no
  workspace-version-inheritance. The resolver collapses to two
  steps: literal version → `0.0.0-unknown` placeholder. The
  placeholder fires when `name` is declared but `version` is
  missing AND `private` isn't `true` (matching cargo's permissive
  behavior per spec Q1).

- **`node_modules/` excluded from manifest discovery.** Deliberate
  ecosystem-specific divergence from cargo's "emit excluded
  crates" rule (064 FR-003). `node_modules/` contains upstream
  dependencies, not project-internal artifacts; emitting
  main-modules for every `node_modules/*/package.json` would
  balloon SBOMs with thousands of FPs.

### Migration

- Consumers reading `metadata.component.purl` for npm scans now
  receive `pkg:npm/<name>@<version>` instead of the pre-066
  `pkg:generic/...` placeholder (or for the
  `tests/fixtures/npm/node-modules-walk` golden, the
  `package-json-only-fixture` synthetic identity).
- Per-ecosystem main-module coverage matrix: Go ✅ (053),
  cargo ✅ (064), npm ✅ (066), pip / maven / gem tracked in #104.

### Changed (BREAKING — SBOM output shape, milestone 064)

- **Cargo SBOMs now identify the project itself** via a synthetic
  main-module component for every `Cargo.toml` with `[package]`.
  Pre-064: cargo SBOMs had no project-self component, so consumers
  could not answer "what is this an SBOM for?" from the bytes alone.
  Post-064: every cargo crate scan emits a `pkg:cargo/<name>@<version>`
  component placed in standards-native "BOM subject" slots — CDX
  `metadata.component` (single-crate) or `components[]` siblings
  (workspace cases); SPDX `primaryPackagePurpose: APPLICATION` plus
  `documentDescribes`; SPDX 3.0.1 `software_primaryPurpose:
  application`. Carries `mikebom:component-role: main-module` (C40)
  as a supplementary signal per Constitution Principle V.

- **Workspace-only `Cargo.toml` files emit no main-module for the
  root.** A `Cargo.toml` declaring only `[workspace]` (no
  `[package]`) is not a publishable crate. Each `[workspace] members
  = [...]` entry emits its own main-module instead. Crates in
  `[workspace].exclude` directories that have their own `[package]`
  ARE emitted (the filesystem walker is authoritative; exclusion is
  a workspace-build concern, not an SBOM-coverage concern).

- **`version.workspace = true` resolution.** Member crates that
  inherit their version from `[workspace.package].version` now
  resolve to the actual workspace-root value in the main-module's
  PURL. Falls back to the literal `0.0.0-unknown` placeholder when
  the workspace root is outside the scan's filesystem boundary —
  same cross-host determinism convention as Go's milestone-053
  `git describe` ladder step 3.

- **Same-PURL collisions dedup with operator-visible warning.**
  When two-or-more `Cargo.toml` files resolve to the same
  `pkg:cargo/<name>@<version>` PURL (vendored copies, `examples/`
  mirrors, `target/package/` extractions), exactly one main-module
  emits (deterministic first-discovered-wins) and a `tracing::warn!`
  lists dropped duplicate paths. Divergent-PURL detection (same
  PURL, different content hashes — a potential supply-chain signal)
  deferred to follow-up issue #125.

- **Generator-side hooks generalized.** The milestone-053 CDX
  `metadata.component` selector and `components[]` exclusion
  predicates are now C40-tag-driven (filter by
  `mikebom:component-role: main-module`) instead of Go-PURL-prefix-
  driven. When the scan contains exactly 1 main-module, it is
  promoted to `metadata.component`; when N>1 (cargo workspaces,
  polyglot scans), all N emit as siblings in `components[]` under
  the existing synthetic super-root.

### Migration

- Consumers reading `metadata.component.purl` for cargo scans now
  receive `pkg:cargo/<crate-name>@<version>` instead of the
  pre-064 `pkg:generic/...` placeholder. Update vuln-intersection
  and licensing tools to recognize the new shape.
- The C40 supplementary annotation continues to emit unchanged on
  Go and now extends to cargo. The `mikebom:component-role:
  main-module` value identifies the project-self component for
  consumers that filter it from licensing-coverage denominators
  (sbomqs convention).
- Per-ecosystem main-module coverage matrix: Go ✅ (053),
  cargo ✅ (064), npm/pip/maven/gem tracked in #104.

### Resolved during alpha.12

- **#127** — multi-main-module workspaces (cargo + polyglot):
  closed by milestone 064 (#128). The synthetic super-root now
  emits plural DESCRIBES via SPDX 2.3 `documentDescribes[]` and
  SPDX 3 `rootElement[]`. Workspace scans correctly surface every
  member.
- **#126** — pre-existing cargo workspace-root edge-emission gap:
  closed by milestone 065 (#129). Workspace-root `[[package]]`
  entries' `dependencies = [...]` declarations are now harvested
  separately and merged into the main-module's `depends`.

### Known gaps (filed for follow-up)

- **#125** — divergent-PURL detection (cargo + npm): when two
  manifests claim the same PURL but have different content hashes
  (potential typosquatting / supply-chain signal), surface a
  stronger SBOM signal beyond the current `tracing::warn!`.
  Applies to the same-PURL dedup paths shipped in milestones 064
  + 066.
- **#103** — LICENSE detection for main-module components.
  Currently the C40 carve-out keeps sbomqs licensing-coverage
  scoring from regressing, but the main-module entries themselves
  have empty `licenses` fields. Real `license`/`license-file`
  reading + SPDX-License-Identifier header scanning + askalono
  content matching tracked here. Applies to all three
  main-module-bearing ecosystems.
- **#104** — remaining ecosystems (pip, maven, gem). The
  C40-tag-driven generator hooks are now ecosystem-agnostic, so
  the per-ecosystem effort is mostly reader-side (a few hundred
  LOC each, similar to milestones 064/066).




## [0.1.0-alpha.11] — 2026-05-02

A focused release on the Go ecosystem covering five milestones shipped
since alpha.10 (~1 day later). Closes the issue #102 residual gap
(transitive edges in offline / empty-cache scans), fixes a graph
topology lie identified during the #113 review, and adds the
Constitution-Principle-X transparency signal so consumers can
interpret partial graphs correctly. Also adds Layer-1 LICENSE
detection for the Go main-module per #103.

### Headline changes

- **Go transitive dependency edges work in offline + empty-cache
  scans.** Pre-055, scanning a Go project with `--offline` and an
  empty `$GOMODCACHE` produced an SBOM whose transitive components
  carried `dependsOn: []` — issue #102's residual gap after
  milestone 053. Post-055, a 4-step resolution ladder
  (`go mod graph` → `$GOMODCACHE` walk → proxy.golang.org fetch →
  graceful no-edges fallthrough) supplies edges from at least one
  source on every typical scan. Honors `$GOPROXY`, `$GOPRIVATE`,
  `$GONOSUMCHECK`. Differentiator vs. peers: Trivy / Syft / cdxgen
  all degrade in the "no `go` toolchain AND no cache" cell;
  mikebom is the only static SBOM tool that produces the full Go
  transitive graph there. (Milestone 055; PR #114.)

- **Go main-module dependency-graph topology no longer lies about
  direct vs indirect.** Pre-059, the synthetic main-module
  emitted ALL `require` lines from `go.mod` as direct edges
  (including those marked `// indirect`). Consumers asking "what
  does this project directly use?" got the wrong answer.
  Post-059, main-module emits ONLY non-`// indirect` requires as
  direct edges; `// indirect` modules are reached transitively via
  the milestone 055 resolver, or become orphans (Trivy-style
  trade-off) when the resolver can't supply transitive edges.
  Closes #113. (Milestone 059; PR #118.)

- **Graph-completeness transparency signal (Principle X).** The
  cost of milestone 059's correctness fix is that orphan
  components appear in offline + empty-cache scans. Pre-061,
  consumers couldn't tell "dead dep" from "mikebom couldn't
  resolve." Post-061, the SBOM signals the limitation natively:
  a document-level `mikebom:graph-completeness` annotation
  (`complete` / `partial`) + a `mikebom:graph-completeness-reason`
  free-text summary, plus per-component `mikebom:orphan-reason`
  on each orphan with the classification (`unresolved-indirect-
  require` / `private-module` / `proxy-fetch-failed`). Closes
  #119. (Milestone 061; PR #121.)

- **Go main-module LICENSE detection (Layer 1).** Closes
  milestone 053 FR-005's deferral. The synthetic main-module
  component now carries the project's own license expression
  populated from `LICENSE` / `LICENSE.md` / `LICENSE.txt` /
  `COPYING` / British `LICENCE` files at the workspace root via
  SPDX-License-Identifier header scan. Default-on; ~30–50% of
  high-profile Go projects ship the SPDX header. Closes #103.
  (Milestone 057; PR #116.)

- **Realistic-project CI gate now asserts Go transitive edges.**
  Milestone 054's `knative/func` regression job gains a second
  scan (without `--offline`) that counts `pkg:golang →
  pkg:golang` `dependsOn` edges and fails the gate if the count
  drops below the floor. Catches future regressions in milestone
  055's resolver. (T036/T037 of milestone 055; PR #115.)

### Breaking changes for SBOM consumers

- **Go main-module's `dependsOn` set shrinks** for every Go
  project that has `// indirect` requires in its `go.mod`. For
  the simple-module test fixture, main-module's outgoing edges
  drop from 10 to 5. SBOM consumers that count main-module's
  direct edges should expect smaller numbers per the new (and
  correct) topology. Indirect components are still present in
  `components[]`; consumers walking the graph beyond direct deps
  should follow `dependsOn` from each direct dep transitively.
  See #118 description for migration notes.

- **Two new CDX `metadata.properties[]` entries appear on every
  Go-touching scan**: `mikebom:graph-completeness` and (when
  partial) `mikebom:graph-completeness-reason`. Equivalents
  appear in SPDX 2.3 + SPDX 3 document-level annotations.
  Cross-format consumers that strict-validate `metadata.properties`
  / `annotations[]` length should expect 1–2 additional entries.
  See `docs/reference/sbom-format-mapping.md` C44.

- **New per-component property `mikebom:orphan-reason`** appears
  on Go components that the resolver couldn't reach. Three-state
  semantics: absent ⇒ component is reachable. Catalog row C45.

### Milestones in this release

- **055**: Go transitive dependency edges, anchored on go.sum
  (4-step ladder). Closes #102's residual gap. PR #114.
- **055 follow-up (T036/T037)**: realistic-project CI gate for
  Go transitive edges. PR #115.
- **057**: Go main-module LICENSE detection (Layer 1). Closes
  #103. PR #116.
- **059**: Go main-module dependency-graph topology fix —
  direct edges only. Closes #113. PR #118.
- **061**: SBOM graph-completeness transparency signal. Closes
  #119. PR #121.

### Follow-ups (open issues)

- **#104**: per-ecosystem main-modules for npm / cargo / maven /
  pip / gem (alpha.11 remains Go-only); when those land they'll
  inherit the Layer-1 LICENSE scanner from milestone 057 and
  the graph-completeness signal pattern from milestone 061.
- **#108**: migrate every filesystem walker to a single shared
  `safe_walk` helper (still deferred from milestone 054).
- **#109**: per-ecosystem expansion of the realistic-project CI
  matrix beyond knative/func.
- **#111**: umbrella transitive-dep correctness audit across
  all ecosystems (only Go addressed in alpha.11).
- **Layer 2** for Go LICENSE detection (content-based matcher
  via askalono or similar, for projects without SPDX headers
  like knative/func itself) — not yet tracked; file if real
  users hit a wall.

## [0.1.0-alpha.10] — 2026-05-02

A larger release covering seven milestones shipped since alpha.9
(~1 day later): three breaking changes to SBOM output shape PLUS
a critical hang fix that SBOM consumers should review before
upgrading.

### Headline changes

- **Filesystem-walker symlink-loop hang fixed.** Pre-054
  `mikebom sbom scan --path <project>` hung at 100% CPU
  indefinitely on any repo containing intentional symlink-loop
  test fixtures (e.g., knative/func @ `knative-v1.22.0` ships
  `pkg/oci/testdata/test-links/linkToRoot -> .` plus parent-loop
  variants). Root cause: `rpm_file::walk_dir` and
  `binary/discover::walk_dir` followed symlinks via
  `path.is_dir()` with no visited-set or depth limit. Per-walker
  hardening now applies a canonicalize-keyed visited-set + max-
  depth backstop to every walker in `mikebom-cli/src/scan_fs/`.
  New `realistic-projects.yml` CI workflow clones knative/func
  per CI run as a regression gate. (Milestone 054; PR #110.)

- **Go source-tree scans now produce a real dependency graph.**
  Pre-053, scanning a freshly-cloned Go project with empty
  `GOMODCACHE` produced an SBOM with **zero `DEPENDS_ON` edges**
  (issue #102). Post-053 it emits a synthetic main-module
  component (CDX `metadata.component`, SPDX
  `primaryPackagePurpose: APPLICATION`) with direct-require edges
  for every `require` in `go.mod`. Closes the parity gap with
  trivy / syft. (Milestone 053; PR #105.)

- **Default scan now emits ALL lifecycle scopes, not just
  Runtime.** Pre-052, the default `mikebom sbom scan` silently
  dropped Development / Build / Test components. Post-052 they
  emit by default with native scope tagging (CDX
  `scope: "excluded"`, SPDX 2.3 `*_DEPENDENCY_OF` typed
  relationships, SPDX 3 `lifecycleScope` on `dependsOn`).
  Consumers wanting the strict pre-052 prod-only view use the
  new `--exclude-scope dev,build,test` flag. (Milestone
  052/part-3; PR #100.)

- **Lifecycle-scope dependency tagging via standards-native
  fields.** The legacy `mikebom:dev-dependency` annotation is
  REMOVED; the dev-vs-build-vs-test distinction now travels via
  each format's standards-defined construct per Constitution
  Principle V (v1.4.0). (Milestone 052/part-2; PR #99.)

### Breaking changes for SBOM consumers

- **CDX `metadata.component.purl` for Go-only scans** shifts
  from synthetic `pkg:generic/<target>@0.0.0` to the real
  `pkg:golang/<module-path>@<version>`.
- **SPDX `documentDescribes` for Go-only scans** targets the
  Go main-module's SPDXID instead of a `SPDXRef-DocumentRoot-*`
  placeholder.
- **CDX `components[]` no longer contains the Go main-module**
  for Go-only scans (it lives in `metadata.component`).
- **`mikebom:dev-dependency` annotation** is gone everywhere —
  consumers filtering on it migrate to CDX
  `components[].scope = "excluded"` / SPDX 2.3
  `*_DEPENDENCY_OF` / SPDX 3 `lifecycleScope` on `dependsOn`.
- **`--include-dev` flag** is now a deprecated parse-and-warn
  no-op shim (will be removed in a future release per #101).
  Use `--exclude-scope dev,build,test` for the strict prod-only
  view.

### Milestones in this release

- **054**: filesystem-walker symlink-loop hang fix +
  realistic-project regression suite (closes #102 — second time;
  PR #110).
- **053**: Go main-module component + direct dependency edges
  (closes #102 — first time; PR #105).
- **052/part-3**: `--exclude-scope` flag + default scope flip +
  `--include-dev` deprecation (PR #100).
- **052/part-2**: native CDX/SPDX 2.3/SPDX 3 lifecycle-scope
  emission + edge rewrite (PR #99).
- **052/part-1**: `LifecycleScope` data model + behavior-
  preserving rename + Constitution Principle V codification
  (PR #98).
- **051**: polyglot dev/test tagging — cargo + gem + maven
  + python + npm dev-dep classification via
  `mikebom:dev-dependency` (legacy, removed in 052/part-2;
  PR #96).
- **050**: `mikebom:not-linked` annotation on Go source-tier
  entries not confirmed by binary BuildInfo + scope hint
  for source-tree-only Go scans (PR #93).

### Follow-ups (open issues)

- **#101**: remove the deprecated `--include-dev` parse-and-warn
  shim once the soak window completes (~3 weeks post-052/part-3).
- **#103**: LICENSE-file detection on the Go main-module
  (currently emits empty `licenses`; C40 role tag preserves
  sbomqs licensing-coverage parity).
- **#104**: per-ecosystem main-modules for npm / cargo / maven /
  pip / gem (Go-only in milestone 053).
- **#108**: migrate every filesystem walker to a single shared
  `safe_walk` helper (milestone 054 kept per-walker patches to
  minimize blast radius before this release).
- **#109**: per-ecosystem expansion of the realistic-project CI
  matrix beyond knative/func (cargo / npm / maven / pip / gem /
  rpm / deb / apk).

## [0.1.0-alpha.9] — 2026-05-01

A small targeted release covering one user-facing fix shipped
since alpha.8 (~1 day later): milestone 049's correction of the
Go source-tree component scope. Resolves an audit-grounded gap
where `mikebom sbom scan --path` on a Go project emitted only
the project's directly-imported modules (collapsing legitimate
transitive prod deps into the dropped-as-test-only bucket).

### Changed

- **Go source-tree scans now emit the full go.sum closure by
  default** (milestone 049). Previously the source-tree filter
  dropped every entry not directly imported by this project's
  non-`_test.go` files, collapsing legitimate transitive prod
  deps (e.g., aws-sdk internals, gin's middleware chain) into
  the test-only bucket. Audit on `apigatewayv2/config` showed
  6 components emitted vs. 55 in trivy / 56 in syft. The new
  default emits every `go.sum` entry as a component (matches
  trivy/syft) and only TAGS the small subset proven test-only
  by source-walking the project's `_test.go` imports. Test-only
  deps carry the existing `mikebom:dev-dependency = true`
  annotation when `--include-dev` is set; default-mode drops
  them (mirrors npm/Poetry/Pipfile semantics). No new flag,
  no new annotation, no new catalog row. CDX + SPDX 2.3 +
  SPDX 3 outputs all carry the new emission via existing
  parity wiring.

  Scope: Go-only. cargo / gem / maven test-tagging extension
  tracked as milestone 050 (see specs/049-go-source-scope/).

## [0.1.0-alpha.8] — 2026-04-30

A small targeted release covering one user-facing feature
shipped since alpha.7 (~1 hour after alpha.7): the
`mikebom:component-role` annotation surfacing
filesystem-position-classified component roles in CDX + SPDX 2.3
+ SPDX 3 outputs. Audit-grounded — addresses 3 false-positive
Maven build-tool JARs surfaced in the alpha.7 polyglot-builder-
image conformance run.

- **Build-tool and language-runtime components are now
  explicitly tagged** in every output format. Maven's own
  internals at `/usr/share/maven/lib/`, JDK system-installed
  JARs at `/usr/lib/jvm/*/lib/`, system Python packages at
  `/usr/lib/python*/site-packages/` and `dist-packages/`, and
  comparable build-tool / language-runtime paths now carry
  `mikebom:component-role = "build-tool"` or
  `mikebom:component-role = "language-runtime"`. Downstream
  consumers (vulnerability scanners, license auditors,
  conformance ground-truths) can filter on the annotation
  without re-implementing the path-heuristic.

### Added

- **`mikebom:component-role` annotation** (048). Components
  whose `evidence.occurrences[]` paths match a curated
  filesystem heuristic now carry a `mikebom:component-role`
  annotation classifying them as `build-tool` (under
  `/usr/share/maven/lib/`, `/usr/share/gradle/lib/`, `/opt/sbt/`)
  or `language-runtime` (under `/usr/lib/jvm/*/lib/`,
  `/usr/lib/node_modules/`, `/usr/lib/python*/site-packages/`,
  `/usr/lib/python*/dist-packages/`). Three-state semantics:
  components without a heuristic match get NO annotation —
  absence does NOT mean "definitely application code", it
  means the heuristic didn't classify. Emitted symmetrically
  across CDX `properties[]`, SPDX 2.3 `packages[].annotations[]`,
  and SPDX 3 top-level `annotations[]` (catalog row C40,
  SymmetricEqual). Lets downstream consumers (vulnerability
  scanners, license auditors, conformance suites) filter
  build-tooling and platform runtime libraries from
  application-deps reporting without mikebom dropping any
  component from the SBOM.

## [0.1.0-alpha.7] — 2026-04-30

A small docs + SPDX-parity release. Two days after alpha.6, with
a focus on closing user-facing gaps surfaced during alpha.6
adoption: SPDX consumers' scope context, README staleness, and
CI flake hardening so the next milestone lands cleanly.

- **SPDX-side document-level scope hint** (047). SPDX 2.3 +
  SPDX 3 outputs now self-describe scope at the document level,
  closing the parity gap with CDX's `metadata.lifecycles[]`.
  Closes the user-reported "is mikebom undercounting?"
  conversational ambiguity by making scope explicit in every
  format.
- **README post-alpha.6 docs refresh** (046). Closes 10 audited
  drift items in user-facing docs: stale version pin,
  `--image-src` flag missing from CLI reference, registry-first
  framing for `--image` (default is now docker-daemon-first),
  internal-milestone-number jargon leaking into user docs,
  `--include-legacy-rpmdb` "deferred" framing for shipped
  behavior.
- **CI test-suite flake hardening** (045 + 044-followon).
  Diagnosed two genuine flake patterns from a 60-run audit:
  macOS-runner perf-test variance (now median-of-5 sampling)
  and a timestamp-race on byte-identity tests (now pinned via
  `MIKEBOM_FIXED_TIMESTAMP` in subprocess-spawning helpers).
  Plus a new gated end-to-end integration test for the
  docker-daemon image source. Test-only — no production
  behavior change.

### Added

- **SPDX-side document-level scope hint** (047). SPDX 2.3
  `creationInfo.comment` and SPDX 3 `SpdxDocument.comment` now
  carry a free-text scope summary naming the scope mode
  (artifact vs manifest, derived from `--include-declared-deps`),
  the observed lifecycle phases (mirroring CDX
  `metadata.lifecycles[]`), and a pointer to the per-component
  `mikebom:sbom-tier` annotation for finer-grained detail. SPDX
  consumers reading metadata-only now get the same scope
  context CDX consumers already had via
  `metadata.lifecycles[]`. CDX output unchanged.
- **README "What kind of SBOM does mikebom emit?" section**
  (047). New top-level section between "Why" and "Install"
  explaining mikebom's two scope axes (document-level
  artifact-vs-manifest mode + per-component lifecycle tier),
  how each format self-describes its scope, and how mikebom's
  default scopes map to industry / NTIA-style terminology — so
  operators comparing component counts to trivy / syft can see
  the question being asked rather than wonder whether mikebom
  is undercounting.
- **End-to-end docker-daemon integration test** (044
  follow-on). Gated on `docker --version` + `docker info`
  succeeding; pulls `alpine:3.19`, runs `mikebom sbom scan
  --image alpine:3.19 --image-src docker`, asserts the SBOM
  was produced via the docker-daemon path and contains ≥5
  components. Skips cleanly on CI lanes without docker
  (macOS-latest).

### Changed

- **README + `docs/user-guide/cli-reference.md` reflect
  post-alpha.6 reality** (046). Status pin updated to alpha.6;
  `--image-src docker,remote` flag documented; `--image`
  description updated to describe docker-daemon-first default;
  `--include-legacy-rpmdb` description rewritten to drop
  "deferred until that code lands" framing for long-shipped
  BDB rpmdb reading; OCI-cache flag rows cross-link to the
  `OCI layer caching` section. Also drops internal milestone
  numbers from user-facing docs (CHANGELOG and design-notes
  retain them as appropriate).

### Fixed

- **macOS perf-test flake** (045). `dual_format_perf` and
  `triple_format_perf` failed intermittently on macos-latest
  CI runners (observed 9.0% / 14.4% / 19.9% reduction vs the
  25% gate, while local distribution sits around 50%). Bumped
  median-of-3 → median-of-5 sampling — cuts the median's
  variance by ≈40% so macOS CPU contention spikes don't push
  the measurement below the gate. CI gate (25%) and spec
  target (30%) unchanged.
- **SPDX byte-identity test flake** (045). Three byte-identity
  tests (`spdx_3_alias_bytes_are_byte_identical_to_stable_
  identifier`, `scenario_7_alias_bytes_are_byte_identical`,
  `scenario_8_mikebom_no_deprecation_notice_env_suppresses_
  stderr_warning`) compared raw bytes across two sequential
  subprocess invocations. When the two invocations straddled
  a second-boundary, `creationInfo.created` diverged at
  second precision, surfacing as a CI flake on unrelated
  branches. Pinned `MIKEBOM_FIXED_TIMESTAMP` in the two
  subprocess-spawning helpers (the env var was added in
  milestone 011 specifically for this case but the helpers
  weren't using it).

## [0.1.0-alpha.6] — 2026-04-29

A small, focused release: makes `mikebom sbom scan --image <ref>`
behave the way users coming from trivy and syft expect, and
unblocks AWS ECR pulls that were previously failing on a Basic
auth challenge.

- **Docker daemon as a default image source.** When `--image
  <ref>` is an OCI reference, mikebom now checks the local docker
  daemon's cache first and falls back to a registry pull only on
  miss. Matches trivy's `--image-src` and syft's auto-detection
  convention. The new `--image-src docker,remote` flag (default
  in that order) controls the resolution sequence; pass
  `--image-src remote` to force a fresh registry fetch.
- **AWS ECR support for the registry path.** The OCI-pull's
  401-retry now handles `Basic` auth challenges in addition to
  `Bearer`, applying cached `~/.docker/config.json` credentials
  directly. ECR's `aws ecr get-login-password | docker login`
  flow now works end-to-end with `--image-src remote`.

Together these resolve the reported case where an ECR image was
already cached locally and `docker login`'d, but mikebom errored
out with `WWW-Authenticate is not a Bearer challenge: Basic ...`.

### Added

- **`Basic` auth challenge support for the OCI registry pull** (044
  commit 2). The 401-retry path now accepts both `Bearer` (existing
  Docker Hub / GHCR / gcr.io flow) and `Basic` (AWS ECR's flavor)
  `WWW-Authenticate` challenges. For `Basic`, mikebom applies the
  cached docker-config credentials directly on the original request
  — no token-realm round-trip. Resolves the previous
  `WWW-Authenticate is not a Bearer challenge: Basic ...` error
  on `mikebom sbom scan --image <ecr-ref> --image-src remote`. The
  `~/.docker/config.json` lookup is unchanged (already supported
  `auths.<host>.auth`, `credHelpers`, `credsStore` since milestone
  034); only the challenge parser was Bearer-only.
- **Local docker daemon as a default image source** (044 commit 1).
  `mikebom sbom scan --image <ref>` now consults the local docker
  daemon before reaching for a registry pull, matching trivy and
  syft conventions. New `--image-src docker,remote` flag controls
  the source-resolution order; default is `docker,remote`. Force a
  fresh registry fetch with `--image-src remote`. Docker source
  shells out to `docker image inspect` + `docker save`, so the
  user's existing `DOCKER_HOST` / contexts are honored. Resolves
  the case where an ECR image is already cached locally but the
  registry pull is failing (e.g. on a Basic-auth challenge).

## [0.1.0-alpha.5] — 2026-04-29

Cuts a new pre-release covering everything merged since
alpha.3 (the alpha.4 tag was a CHANGELOG-less mechanical
bump). Ships milestones 010, 023–030, and 034–042 together.
Highlights:

- **Container per-file evidence trilogy** (037 → 040 → 041):
  deb, apk, and rpm components all carry populated
  `evidence.occurrences[]` blocks now, plus matching
  upstream-cross-ref checksums (`md5` / `sha1` /
  `rpm_filedigest`) in `additionalContext`.
- **Direct OCI registry image scanning** (034 → 036):
  `mikebom sbom scan --image alpine:3.19` now pulls from
  registries directly, including authenticated private pulls
  via the standard Docker keychain, cross-arch selection via
  `--image-platform`, and SHA-256-content-addressed disk
  caching for fast repeat scans.
- **Distroless / chainguard / Bazel-built minimal-image
  coverage** (037 → 038): the per-package
  `/var/lib/dpkg/status.d/` layout and its `.md5sums`
  companion files are now read; deb minimal images go from
  zero components to a full SBOM with per-file evidence.
- **Mach-O binary identity + codesign + Go VCS metadata**
  (024 → 025 → 030): macOS and Apple-platform binaries now
  emit `LC_UUID`, `LC_RPATH`, codesign identifier / flags /
  team-id, and Go-binary VCS commit-SHA + build-time
  metadata.
- **Maven sidecar Debian layout** (042): in addition to
  Fedora's `/usr/share/maven-poms/`, mikebom now reads
  Debian's `/usr/share/maven-repo/` GAV-tree layout, so
  `lib*-java`-installed JARs surface as
  `pkg:maven/<group>/<artifact>@<version>` PURLs.
- **Two cross-ref-symmetry milestones** (040 US2 and 041)
  bring apk and rpm to parity with deb's longstanding `md5`
  cross-ref carrier on per-file occurrences.

Detailed entries below.

### Added
- **Milestone 042 — Post-041 small follow-ons.** Two unrelated
  legacy-deferral items closed:
  - **US1 (housekeeping)**: dropped a stale comment in
    `binary/predicates.rs:88` that named rpm file-list
    extraction from HeaderBlob `BASENAMES` / `DIRNAMES` /
    `DIRINDEXES` as "deferred to a follow-on milestone." That
    work shipped in milestone 040 US3; the comment now
    accurately credits 040 US3 as the authoritative claim
    source and explains the directory-heuristic's role as a
    defense-in-depth fallback for corrupt / partial rpmdb cases.
  - **US2 (Maven sidecar Debian layout)**: extends
    `maven_sidecar.rs` with a parallel `DebianSidecarIndex`
    that walks `/usr/share/maven-repo/` (the GAV-tree layout
    populated by Debian's `maven-repo-helper` during
    `apt-get install lib*-java`). Debian-shaped Java images
    that previously emerged as `pkg:generic/<filename>` PURLs
    now resolve to `pkg:maven/<group>/<artifact>@<version>` —
    matching the milestone-007 Fedora-side coverage.
    Implementation introduces a small `SidecarIndex` trait so
    `resolve_coords` works generically over either layout.
    Fedora wins on basename collision (FR-005). Alpine
    layouts remain out of scope (Alpine ships no documented
    system-wide maven repo convention).
  - 6 new inline tests for the Debian sidecar reader; 27
    byte-identity goldens regen with zero diff (no fixture
    contains `/usr/share/maven-repo/` content).
- **Milestone 041 — Rpm FILEDIGESTS cross-reference.** Closes
  the milestone-040 Q1 deferral. Every populated rpm
  `evidence.occurrences[]` entry's `additionalContext` JSON-
  string now carries `rpm_filedigest` alongside the existing
  `sha256`, in algorithm-prefixed form (e.g.
  `"sha256:abc..."` for modern rpm packages,
  `"md5:def..."` for legacy ones). The algorithm matches the
  package's `FILEDIGESTALGO` value (or defaults to MD5 when
  absent per the rpm spec). Brings rpm to full cross-ref
  symmetry with deb (`md5`, since milestone 037) and apk
  (`sha1`, since milestone 040 US2).
  Verified end-to-end against `fedora:40`: 6938 of 6966
  total file occurrences carry the cross-ref (99.6%; the
  28 remainder are non-regular files whose `FILEDIGESTS`
  entry is empty by rpm-spec convention). Sample value
  `rpm_filedigest = "sha256:7544bd..."` matches the
  mikebom-observed `sha256` for the same file — the
  integrity-check arrow goes both ways. New
  `rpm_file_digest: Option<String>` field on
  `mikebom_common::resolution::FileOccurrence` (additive,
  `#[serde(default, skip_serializing_if = "Option::is_none")]`).
  No new top-level dependencies. 27-fixture goldens regen
  with zero diff. See `specs/041-rpm-filedigests/spec.md`.
- **Milestone 040 — Package-DB follow-ons (trifecta).** Three
  sequenced follow-on items closing coverage and hygiene gaps
  after milestones 037 / 038 / 039:
  - **US1 (housekeeping)**: dropped a stale "deferred to
    milestone 031.y" framing in `oci_pull/mod.rs::host_oci_arch`
    that named `--image-platform` as deferred. The flag shipped
    in milestone 035 (PR #72); the error message now positively
    references it with an example invocation.
  - **US2 (apk SHA-1 cross-ref)**: extends milestone 039's apk
    per-file evidence with the apk-provided SHA-1 from each `Z:`
    line in `/lib/apk/db/installed`. Surfaced as `sha1` in the
    per-occurrence `additionalContext` JSON-string alongside the
    mikebom-computed `sha256`. Mirrors deb's `md5` cross-ref
    contract from milestone 037. New `ApkFileEntry` struct in
    apk.rs, new optional `apk_sha1: Option<String>` field on
    `mikebom_common::resolution::FileOccurrence` (additive,
    `#[serde(default, skip_serializing_if = "Option::is_none")]`).
    Verified end-to-end against `alpine:3.19`.
  - **US3 (rpm per-file deep-hash)**: completes the OS-package
    per-file-evidence trilogy. rpm-based images (fedora,
    almalinux, rocky, centos:stream, redhat/*) now produce
    populated `evidence.occurrences[]` blocks at parity with
    deb (037/038) and apk (039). New
    `rpm::read_file_lists(rootfs)` exposes the per-package
    file-list map decoded from the rpmdb HeaderBlob's
    `BASENAMES` / `DIRNAMES` / `DIRINDEXES` triple via the
    existing `iter_rpmdb` helper; new `hash_rpm_package_files`
    + `hash_rpm_db_only` mirror the apk pattern; new `is_rpm`
    branch in `scan_fs/mod.rs::read_all`. Verified end-to-end:
    `fedora:40` produces 147 components with 6966 total file
    occurrences (was 0). Per the milestone-040 Q1
    clarification, rpm FILEDIGESTS cross-ref is OUT of scope
    and deferred to a separate follow-on milestone — rpm-side
    `additionalContext` carries SHA-256 only.
  - No new top-level Cargo dependencies. 27 byte-identity
    goldens regen with zero diff (the goldens use
    `--no-deep-hash` so they're insulated from the deep-hash
    path by design).
  - See `specs/040-pkg-db-followups/spec.md`.
- **Milestone 039 — Per-file evidence for apk components
  (alpine + chainguard apko + Wolfi).** Closes the asymmetry
  surfaced during milestone 038's recon (#75): apk-based images
  now produce per-file `evidence.occurrences[]` blocks at the
  same quality as deb-based images. Implementation mirrors the
  dpkg deep-hash path: a new `apk::read_file_lists` extracts
  per-package paths from the `F:` (directory) and `R:` (regular
  file) lines that the apk installed-db carries inline; a new
  `hash_apk_package_files` walks those paths, opens each file,
  and SHA-256s the content (same 256 MB cap as the dpkg path).
  A parallel `--no-deep-hash` fast path
  (`hash_apk_db_only`) hashes the package's stanza bytes
  in-place. Verified end-to-end:
  `alpine:3.19` produces 79 file occurrences across 15
  components (was 0); `cgr.dev/chainguard/static:latest`
  produces 1217 occurrences across 3 components (was 0). 27
  byte-identity goldens regen with zero diff (those goldens use
  `--no-deep-hash` so they're insulated from the deep-hash path
  by design). Apk-side `additionalContext` carries SHA-256 only;
  the apk-provided SHA-1 (`Z:` lines) is a future extension. No
  new top-level dependencies. Closes #75. See
  `specs/039-apk-deep-hash/spec.md`.
- **Milestone 038 — Per-file evidence for distroless /
  Bazel-built minimal-image deb scans.** Closes the deferred
  milestone-037 item: distroless deb images
  (`gcr.io/distroless/*`, rules-distroless, similar Bazel-built
  minimal images) now produce populated
  `evidence.occurrences[]` blocks with per-file paths and
  SHA-256 + MD5 hashes — matching the evidence quality
  full-fat-image scans have produced since the early
  milestones. Implementation: extended
  `file_hashes.rs::read_info_file{,_bytes}` lookup chain to
  fall back to `var/lib/dpkg/status.d/<pkg>.<ext>` after the
  legacy `info/` paths, and synthesized the path list from the
  second column of `<pkg>.md5sums` when `<pkg>.list` is absent.
  Stanzas in this layout legitimately omit the `Status:` field
  (no dpkg daemon manages install state in the image), so a
  relaxed parse path was added that treats the stanza file's
  existence as the installation marker; strict filtering is
  preserved for the legacy `status` file source. Verified
  end-to-end: `gcr.io/distroless/static-debian12:latest` now
  produces 4 components with 938 total file occurrences (was
  0). 27 byte-identity goldens regen with zero diff.
  Out-of-scope concurrent finding: apk per-file evidence is
  empty for both `alpine:3.19` and chainguard apko/wolfi
  images — mikebom's `file_hashes.rs` is dpkg-only. Filed as
  follow-on issue
  [#75](https://github.com/kusari-sandbox/mikebom/issues/75)
  for a future milestone. See
  `specs/038-minimal-image-deep-hash/spec.md`.
- **Milestone 037 — distroless / chainguard / Bazel minimal-image
  dpkg coverage.** mikebom now reads per-package metadata from
  `/var/lib/dpkg/status.d/<pkgname>` files in addition to the
  legacy single-file `/var/lib/dpkg/status`. Closes the
  milestone-031-surfaced gap where mikebom reported 0 deb
  components for `gcr.io/distroless/static-debian12:latest` and
  similar minimal images that ship per-package metadata files
  instead of the monolithic dpkg-daemon-managed `status` file.
  Same coverage syft and trivy already provided. Filtering uses
  parse-success-or-skip so companion files (`<pkg>.md5sums`,
  `.conffiles`, etc.) naturally drop out without breaking on
  package names that contain dots (`python3.11`). When both
  layouts are present (defensive — never seen in practice), the
  `status.d/` source wins. No new dependencies, no SBOM-shape
  changes, no parity-catalog impact. Closes #64. See
  `specs/037-dpkg-status-d/spec.md`.
- **Milestone 036 (031.z) — On-disk cache for pulled OCI image blobs.**
  Repeat-scans of the same image now skip the network fetch and
  read from a SHA-256-content-addressed cache on disk, completing
  in seconds rather than tens of seconds for non-trivial images.
  The cache lives at `$MIKEBOM_OCI_CACHE_DIR` →
  `$XDG_CACHE_HOME/mikebom/oci-layers` →
  `$HOME/Library/Caches/mikebom/oci-layers` (macOS) →
  `$HOME/.cache/mikebom/oci-layers` (fallback). Default size cap
  10 GB with mtime-based LRU eviction; configurable via
  `--oci-cache-size <bytes>` or `MIKEBOM_OCI_CACHE_SIZE=<bytes>`.
  Disable with `--no-oci-cache` or `MIKEBOM_OCI_CACHE=0`. Every
  cache read re-verifies SHA-256 against the digest, so silent
  corruption is detected and recovered (drop entry + re-fetch).
  Atomic-rename writes (tempfile + persist) keep concurrent scans
  safe. Best-effort posture: any IO failure (read-only fs, missing
  $HOME) falls through to network-only behavior; scans complete
  either way. Manifests are NOT cached (floating tags like
  `:latest` need to re-fetch). No new dependencies. Closes #68.
  See `specs/036-oci-layer-cache/spec.md` and the new
  ["OCI layer caching"](docs/user-guide/cli-reference.md#oci-layer-caching)
  section in the user guide.
- **Milestone 035 (031.y) — `--image-platform` CLI flag for cross-arch
  image scans.** New `mikebom sbom scan --image <ref>
  --image-platform linux/<arch>[/<variant>]` selects a specific
  platform from a multi-arch image index instead of auto-resolving
  to `linux/<host-arch>`. Common shapes: `linux/amd64`,
  `linux/arm64`, `linux/arm/v7`, `linux/386`, `linux/ppc64le`,
  `linux/s390x`. The variant segment is honoured for indexes that
  carry it (e.g. arm v6 vs v7 vs arm64 v8). Closes the macOS-arm64
  dev / Linux-x86_64 CI workflow gap that previously required
  `docker pull --platform <X> && docker save` to scan a non-host
  image. Registry-only — passing `--image-platform` alongside a
  pre-extracted tarball errors clearly. Non-`linux` OS values
  reject with an explanation that mikebom's package-DB readers are
  linux-rootfs-shaped. No SBOM-shape changes (the byte-identity
  goldens regen produces zero diff). Closes #67. See
  `specs/035-image-platform-flag/spec.md` and the new flag row in
  `docs/user-guide/cli-reference.md`.
- **Milestone 034 (031.x) — Authenticated OCI registry pulls.**
  `mikebom sbom scan --image <ref>` now supports private registries
  via the standard Docker keychain — the same `~/.docker/config.json`
  (or `$DOCKER_CONFIG/config.json`) that `docker pull` uses. All four
  documented credential sources resolve in Docker's documented
  precedence order: per-registry `credHelpers` > registry-wide
  `credsStore` > direct `auths.<reg>.auth` (base64 user:password) >
  `auths.<reg>.identitytoken`. Credential helpers are invoked as
  subprocesses (`docker-credential-<helper> get`) per the published
  protocol — covers ECR (`docker-credential-ecr-login`), Google
  Artifact Registry (`docker-credential-gcloud`), macOS keychain,
  Windows credential store, GNOME Secret Service, and `pass`. When
  credentials resolve, they're sent as Basic auth on the
  bearer-token realm GET; the resulting bearer token authorizes the
  manifest + blob fetches. Anonymous fallback is preserved: no
  config.json + public image works exactly as it did in milestone
  031. Credentials never leak to stdout, stderr, `--verbose` output,
  or `RUST_LOG=debug` traces — `Credential::Debug` redacts both
  fields and the helper subprocess's stderr is dropped to /dev/null.
  No new top-level dependencies; the
  `no_c_dependencies_in_oci_registry_feature_tree` regression test
  still passes. See `specs/034-authenticated-registry-pulls/spec.md`
  and the new ["Authenticating to private registries"](docs/user-guide/cli-reference.md#authenticating-to-private-registries)
  section in the user guide. Closes #66.
- **Milestone 030 — Mach-O codesign metadata.** Every Mach-O scan
  now extracts three identity-flavored signals from the
  `LC_CODE_SIGNATURE` (cmd `0x1D`) SuperBlob's CodeDirectory blob:
  `mikebom:macho-codesign-identifier` (e.g. `com.apple.ls` —
  universal across Apple-signed binaries),
  `mikebom:macho-codesign-flags` (JSON array decoded from
  `CodeDirectory.flags` — `hardened-runtime`, `library-validation`,
  `adhoc`, etc.; unrecognized bits emit as `unknown-0x<hex>`), and
  `mikebom:macho-codesign-team-id` (10-char Apple Team ID for
  developer-signed binaries; absent for Apple-system signatures
  whose `TeamIdentifier=not set` and for ad-hoc signatures). This
  is what `codesign -dvv` reads. Fat / universal binaries report
  from the first slice (matching milestone 024's convention).
  **Sixth amortization-proof consumer of the milestone-023
  `extra_annotations` bag** (after 023/024/025/028/029 — 026 was
  a coverage-breadth milestone that didn't touch the bag). No new
  crate dependencies. CMS PKCS#7 cert-chain decoding (which would
  extract the leaf-cert subject CN, signing time, intermediate
  cert hashes — requires ASN.1 DER parsing) and entitlements XML
  extraction explicitly deferred to a follow-on milestone (likely
  unified with PE Authenticode, which has the same DER-parsing
  requirement). See `specs/030-macho-codesign-metadata/spec.md`
  and catalog rows C37/C38/C39 in
  `docs/reference/sbom-format-mapping.md`.
- **Milestone 029 — cargo-auditable extraction.** Extracts the
  zlib-compressed JSON manifest from Rust binaries' `.dep-v0` linker
  section ([cargo-auditable](https://github.com/rust-secure-code/cargo-auditable)
  format) and surfaces the full build-time crate dependency closure as
  per-crate `pkg:cargo/<name>@<version>` components with
  `evidence-kind = "cargo-auditable"`, `confidence = "high"`,
  `parent_purl` cross-linking back to the file-level binary, and
  index-based `dependencies` resolved into `depends` edges. The binary
  itself gains a `mikebom:detected-cargo-auditable = true` cross-link
  annotation (Rust analog of milestone 005's `mikebom:detected-go =
  true`). Cargo wrappers in Debian Trixie+, Fedora 40+, Alpine Edge,
  and the official Rust container images auto-enable the embedding —
  so most Rust binaries built in those environments now surface their
  full statically-linked crate closure without source access. Cross-
  format: ELF / Mach-O / PE. Optional bag annotations
  `mikebom:cargo-auditable-source` (non-registry sources) and
  `mikebom:cargo-auditable-kind` (non-runtime kinds) preserve
  manifest detail. **Fifth amortization-proof consumer of the
  milestone-023 `extra_annotations` bag** (after 023/024/025/028 —
  026 was a coverage-breadth milestone that didn't touch the bag).
  No new crate dependencies — `flate2` and `serde_json` were already
  in the workspace. See `specs/029-cargo-auditable-extraction/spec.md`
  and catalog row C36 in `docs/reference/sbom-format-mapping.md`.
- **Milestone 026 — curated version-string scanner expansion (easy-4
  cohort).** Extends `version_strings.rs`'s curated scanner from 7 to
  **11 self-identifying native libraries**. Four new detectors with
  clean self-identifying signatures in the binary's read-only string
  region:
  - **GnuTLS** (`GnuTLS X.Y.Z`) — common in curl-with-GnuTLS, wget,
    GnuPG, GNU-stack tools.
  - **LibreSSL** (`LibreSSL X.Y.Z`) — macOS system tools (system curl
    was LibreSSL-backed for years), OpenBSD-derived utilities.
  - **LLVM** (`LLVM version X.Y.Z`) — strict prefix; bare `LLVM ` is
    too noisy (matches `LLVM ERROR:`, `LLVM IR ...` etc.).
  - **OpenJDK** — two-scheme parser handling both modern JEP 322
    (`21.0.1+12`) and legacy Java 8 (`8u362-b09`).

  Each match emits a `pkg:generic/<library>@<version>` component with
  `mikebom:evidence-kind = "embedded-version-string"` and
  `mikebom:confidence = "heuristic"`, flowing through the existing
  `version_match_to_entry` machinery (no downstream wiring change).
  9 new inline tests cover positive + negative cases per library
  plus a `libressl_distinct_from_openssl` cross-validation test.

  Three additional libraries from the original wishlist (glibc, musl,
  V8) are deferred to a 026.x research-and-attempt follow-on because
  they don't have clean self-identifying strings in `string_region` —
  glibc's `GLIBC_X.Y` lives in the `.gnu.version_r` ELF section, musl
  rarely self-identifies in compiled output, and V8's version strings
  are buried in stack-trace formatting code. Tracked via
  `TODO(milestone-026.x)` in `version_strings.rs` and the
  "Deferred backlog" section of `docs/design-notes.md`. See
  `specs/026-version-string-library-expansion/spec.md`.

  Note: this milestone is **not** a `extra_annotations` bag consumer —
  it produces new components rather than annotations on existing
  components. The bag-amortization streak from 023/024/025/028 stays
  at four; 026 is purely scanner coverage breadth.
- **Milestone 028 — PE binary identity.** Every Windows-binary scan
  now surfaces three identity signals via `object` 0.36's typed PE
  accessors: `mikebom:pe-pdb-id` (the `<guid-hex-lowercase>:<age>`
  pair from the CodeView Type-2 record in `IMAGE_DIRECTORY_ENTRY_DEBUG`
  — the canonical PE binary identity used by Microsoft Symbol Server,
  Mozilla / Chromium symbol stores, WinDbg, drmingw; analog of
  Linux's NT_GNU_BUILD_ID and macOS's LC_UUID), `mikebom:pe-machine`
  (lowercase `IMAGE_FILE_HEADER.Machine` — `amd64` / `i386` /
  `arm64` / `armnt` / `ia64` / `riscv32` / `riscv64` / `unknown`),
  and `mikebom:pe-subsystem` (lowercase
  `IMAGE_OPTIONAL_HEADER.Subsystem` — `console` / `windows-gui` /
  `efi-application` / `native` / etc., with `WINDOWS_CUI` rendering
  as `console` per Microsoft toolchain idiom). PE32 vs PE32+
  bit-width is auto-dispatched by reading
  `IMAGE_OPTIONAL_HEADER.Magic` (`0x10B` vs `0x20B`). With ELF (023)
  and Mach-O (024) already shipping, this completes the binary-
  identity trifecta — every compiled binary mikebom scans now
  carries cross-platform identity in the SBOM. Surfaced via the
  milestone-023 generic annotation bag — the **fourth** amortization-
  proof consumer, with zero churn in `package_db/`, `mikebom-common/`,
  `cli/`, `resolve/`, `generate/`, `elf.rs`, or `macho.rs`. See
  `specs/028-pe-binary-identity/spec.md` and catalog rows
  C33/C34/C35 in `docs/reference/sbom-format-mapping.md`.
- **Milestone 024 — Mach-O binary identity.** Every macOS-binary
  scan now surfaces three identity signals from byte-level Mach-O
  load-command parsing: `mikebom:macho-uuid` (16-byte LC_UUID
  hex-encoded lowercase — the macOS analog of NT_GNU_BUILD_ID; used
  by `dwarfdump`, `xcrun symbolicatecrash`, the macOS crash reporter,
  and every `*.dSYM` bundle for symbol matching),
  `mikebom:macho-rpath` (LC_RPATH paths in declaration order, dedup'd
  — `@executable_path` / `@loader_path` / `@rpath` recorded raw,
  runtime-context-dependent expansion deferred to consumers), and
  `mikebom:macho-min-os` (`<platform>:<version>` shape — e.g.
  `macos:14.0`, `ios:17.5` — preferring `LC_BUILD_VERSION`, falling
  back to `LC_VERSION_MIN_MACOSX` / `LC_VERSION_MIN_IPHONEOS` /
  `LC_VERSION_MIN_TVOS` / `LC_VERSION_MIN_WATCHOS`). Fat / universal
  Mach-O binaries report from the FIRST slice's bytes (per-slice
  arch-divergence is uncommon in practice; consumers needing it can
  fall back to `otool -l <slice>`). SC-002 verified on the macOS CI
  lane: `/bin/ls` scan emits a non-empty 32-lowercase-hex
  `mikebom:macho-uuid` and a non-empty `<platform>:<version>`
  `mikebom:macho-min-os` — both universal on every supported macOS
  version. Surfaced via the milestone-023 generic annotation bag,
  with zero PackageDbEntry-init churn (the bag's amortization
  payoff). 3 atomic commits; see `specs/024-macho-binary-identity/spec.md`
  and catalog rows C30/C31/C32 in `docs/reference/sbom-format-mapping.md`.
- **Milestone 025 — Go BuildInfo VCS metadata.** Every Go-binary scan
  now surfaces the source-tree VCS state recorded at build time. The
  main-module entry (`pkg:golang/<module>@<version>`) gains three new
  annotations across CDX / SPDX 2.3 / SPDX 3:
  `mikebom:go-vcs-revision` (commit SHA from `vcs.revision`),
  `mikebom:go-vcs-time` (RFC 3339 commit timestamp from `vcs.time`),
  `mikebom:go-vcs-modified` (dirty-tree boolean from `vcs.modified`,
  preserved as the literal string `"true"` / `"false"` matching Go's
  wire format). The data was already present in BuildInfo's vers_info
  blob; pre-025 the parser read only the first line (Go version) and
  discarded the rest. Dep modules don't carry VCS info — it's a
  main-module concern. Surfaced via the milestone-023 generic
  annotation bag, with zero PackageDbEntry-init churn or generate/
  plumbing changes (the bag's amortization payoff). 4 atomic commits;
  see `specs/025-go-vcs-metadata/spec.md` and catalog rows C27/C28/C29
  in `docs/reference/sbom-format-mapping.md`.
- **Milestone 023 — ELF binary identity + per-component generic
  annotation bag.** Two cohorts in one milestone. (a) ELF identity:
  every Linux-binary scan now surfaces `NT_GNU_BUILD_ID` (the
  canonical Linux binary-identity hash used by `eu-unstrip`,
  `coredumpctl`, `debuginfod`, `*-dbgsym` packaging), `DT_RPATH` /
  `DT_RUNPATH` (embedded library search paths the dynamic loader
  consults — `$ORIGIN` etc. recorded raw), and `.gnu_debuglink`
  (pointer to the stripped-debug sibling file). Three new annotations
  on the file-level binary component: `mikebom:elf-build-id`,
  `mikebom:elf-runpath`, `mikebom:elf-debuglink`. SC-002 is satisfied
  on Linux CI: `/bin/ls` scan emits a non-empty hex build-id (every
  modern distro stamps build-ids by default). (b) Per-component
  annotation bag: `extra_annotations: BTreeMap<String, Value>` on
  `PackageDbEntry` and `ResolvedComponent` provides a generic per-
  component annotation channel that future per-binary-metadata
  milestones (024 Mach-O LC_UUID, 026 version-string library
  expansion, 027 container layer attribution) can populate without
  per-field schema migration. Determinism is preserved by `BTreeMap`
  iteration order. Catalog rows C24/C25/C26.

- **Milestone 010 — SPDX 2.3 output + OpenVEX sidecar + SPDX 3.0.1
  experimental stub.** SPDX 2.3 JSON is now a peer of CycloneDX across
  all 9 supported ecosystems. A single `mikebom sbom scan` invocation
  can emit both formats from one pass over the target; the new
  `--format` flag accepts a comma-separated list and is repeatable,
  and `--output` accepts either a bare path (single-format, legacy)
  or repeated `<fmt>=<path>` (per-format). Every data element that
  CDX emits has a documented target in SPDX — native field where the
  spec has one, `annotations[]` entry with a `mikebom-annotation/v1`
  JSON envelope for the rest; the full map is at
  `docs/reference/sbom-format-mapping.md`. When a scan produces
  advisory data, SPDX 2.3 emission co-emits a companion OpenVEX 0.2.0
  JSON sidecar referenced from the SPDX document via
  `externalDocumentRefs` with a SHA-256 of the sidecar bytes;
  `--output openvex=<path>` retargets it (legal only alongside an
  SPDX format). A third, opt-in format `spdx-3-json-experimental`
  emits a minimal SPDX 3.0.1 JSON-LD document for npm components —
  clearly labeled `[EXPERIMENTAL]` in `--help`, in error messages,
  and in the document's own `CreationInfo.comment`. Typing bare
  `spdx-3-json` offers a did-you-mean hint. No behavior change for
  users who don't request SPDX output: CycloneDX emission is
  byte-identical to the pre-milestone baseline, guarded by pinned
  golden fixtures and a dedicated regression test.
  See `specs/010-spdx-output-support/spec.md` for the full
  requirement list and `docs/reference/sbom-format-mapping.md` for
  the cross-format data-placement contract.
- **Feature 009 refinement — bytecode-presence gating for Maven
  shade-relocation.** Shade-relocation entries are now emitted only when
  an ancestor's bytecode is verifiably present in the enclosing JAR
  (either at its original group path or at a shade-relocated path whose
  leaf matches a distinctive artifact-id fragment). Apache's
  `maven-dependency-plugin` emits `META-INF/DEPENDENCIES` into any JAR
  it is configured on, not only shade fat-jars, so the pre-gating
  emission path reported ancestors as "present in" JARs whose bytecode
  was never relocated there. New unit + integration tests exercise
  every disposition. See `specs/009-maven-shade-deps/spec.md` FR-002b.

### Changed
- **`oci-registry` Cargo feature is now on by default.** Direct
  registry pulls (`mikebom sbom scan --image alpine:3.19`) work
  out of the box on a stock `cargo install mikebom` — matches
  syft / trivy UX without requiring `--features oci-registry`.
  The post-milestone-032 substrate (`oci-spec` types-only +
  workspace `reqwest 0.12` + mikebom-owned thin HTTP client) is
  small enough + durable enough that the milestone-031 default-off
  framing no longer pays for itself. Users embedding mikebom in a
  context that needs a minimal-deps build can opt out via
  `cargo install mikebom --no-default-features`; the local
  `--path <dir>` and `--image <foo.tar>` paths still work in that
  configuration. The dep-audit guardrail
  (`no_c_dependencies_in_oci_registry_feature_tree` regression
  test) continues to enforce zero new C-bound transitive deps in
  the now-default tree.

### Removed
- **`mikebom sbom compare` subcommand** and the `demos/` directory.
  The head-to-head comparison story is now owned by a separate test
  suite outside this repo; keeping the in-tree version invited drift
  between the two. Any workflow that depended on `sbom compare`
  should move to the external suite.

## [0.1.0-alpha.3] — 2026-04-23

### Added
- **Feature 009 US1 — shade-relocation ancestor emission.** When a JAR
  contains `META-INF/DEPENDENCIES`, mikebom emits one nested
  `pkg:maven/...` component per declared ancestor, nested under the
  enclosing JAR's primary coord and tagged with
  `mikebom:shade-relocation = true`. Ancestor licenses are parsed from
  the adjacent `License:` lines. Classifier-bearing coords preserve
  `?classifier=<value>` in the PURL. Self-references are dropped
  (`com.example:outer` cannot shade itself). Commit `cdf29b0`.
- **Feature 008 US3 — Maven target/-dir path heuristic** for
  suppressing `target/`-staged development artifacts from image scans.
  Commit `701ea50` (#14).
- **Feature 008 US2 — cache-ZIP Go component filter.** Emissions from
  Go module-cache ZIPs are cross-checked against the linked binary's
  `runtime/debug.BuildInfo`, suppressing ZIPs that never made it into
  the shipped binary. Commit `db6fbab` (#13).
- **Feature 007 US1 — Fedora sidecar POM reading.** JARs installed by
  `dnf` that have stripped embedded `META-INF/maven/` metadata now
  fall back to `/usr/share/maven-poms/` sidecar POMs (JPP-prefixed
  and plain). Commit `a06b7ff` (#8).
- **Feature 007 US2+US3 — Go test-scope and main-module filters.**
  go.sum and BuildInfo emissions are filtered against non-`_test.go`
  import closure and against the primary module's self-coord. Commit
  `b06eda8` (#10).
- **Feature 007 US4 — Main-Class executable-JAR self-reference
  suppression.** JARs whose `META-INF/MANIFEST.MF` names a `Main-Class`
  no longer re-emit their own primary coord as a generic-binary
  `pkg:generic/...` entry. Commit `89a334f` (#11).
- **Feature 006 US5 — SBOM enrichment (`mikebom sbom enrich`).**
  RFC 6902 JSON Patch applier with per-patch provenance recorded as
  `mikebom:enrichment-patch[N]` properties on the BOM metadata. Replaces
  a previously stubbed bail.
- **Feature 006 US4 — in-toto policy layouts (`mikebom policy init`
  and `mikebom sbom verify --layout`).** Single-step functionary-keyed
  layouts. Multi-step deferred.
- **Feature 006 US3 — real artifact subjects.** Attestation subjects
  are resolved via a 5-stage resolver (operator override → artifact-dir
  walk → suffix match → magic-byte detect for ELF / Mach-O / PE →
  synthetic fallback).
- **Feature 006 US2 — DSSE signing + verification** via `sigstore-rs`
  0.10 (pinned below 0.13 to stay off `aws-lc-rs` per Constitution
  Principle I). `mikebom sbom verify` replaces the never-shipped `sbom
  validate` stub; exit contract: 0 pass / 1 crypto / 2 envelope /
  3 layout.
- **Feature 006 foundation — DSSE verify MVP + witness-v0.1 emission.**
  `mikebom trace run` emits in-toto statements compatible with
  `go-witness` / `sbomit generate`.
- **ClearlyDefined license enrichment.** Post-scan enricher querying
  `api.clearlydefined.io` for `npm`, `cargo`, `gem`, `pypi`, `maven`,
  `golang` components. CD's `licensed.declared` becomes an
  `acknowledgement: "concluded"` license entry. `--offline` disables.
- **Per-ecosystem manifest hashes.** Maven sidecar hashes
  (`.jar.sha512` > `.sha256` > `.sha1`) and PyPI `requirements.txt
  --hash=alg:hex` flags now thread through to `components[].hashes[]`.
- **`metadata.component` carries synthetic `purl` + `cpe`** for sbomqs
  schema validity (`pkg:generic/<name>@<version>` +
  `cpe:2.3:a:mikebom:<name>:<version>:...`).
- **`--include-legacy-rpmdb` flag** (feature 004 US4) enables reading
  legacy Berkeley-DB `/var/lib/rpm/Packages` on pre-RHEL-8 /
  CentOS-7 / Amazon-Linux-2 rootfs. Off by default; also configurable
  via `MIKEBOM_INCLUDE_LEGACY_RPMDB=1`.

### Changed
- **`mikebom trace` reclassified as experimental.** Primary SBOM
  surface is now `mikebom sbom scan`. Trace-mode output format
  (witness-v0.1 + DSSE envelope) remains stable; the capture pipeline
  itself is opt-in, Linux-only (kernel ≥ 5.8), and adds 2–3× wall-clock
  overhead on syscall-heavy builds. Commit `45da74d`.
- **Artifact vs. manifest SBOM scope** is now explicit.
  `sbom scan --image` defaults to artifact scope (on-disk presence
  required). `sbom scan --path` defaults to manifest scope (declared
  deps included). `--include-declared-deps` is the explicit override.
  Gated in three Maven emission paths: deps.dev graph enricher,
  pom.xml direct-dep loop, and the `.m2` BFS cache-miss branch.
- **Dual-identity Maven coords.** JARs at `/usr/share/java/*` owned by
  an OS package-db reader (RPM / dpkg / apk) now emit both identities:
  the `pkg:rpm/...` NEVRA (for distro CVE feeds) and the
  `pkg:maven/<g>/<a>@<v>` GAV (for Maven Central advisories). The
  Maven coord is tagged `mikebom:co-owned-by = rpm` (or equivalent);
  `archive_sha256` is dropped since the archive bytes belong to the
  owning OS component. Pre-fix, the Maven coord was skipped entirely
  under a claim-based heuristic, which cost 53 polyglot GT matches.
- **CycloneDX 1.6 conformance pass.** `evidence.identity` is now an
  array (single-object form deprecated in 1.5→1.6);
  `evidence.identity[].tools` is no longer emitted (the previous
  payload wasn't `tools` by the spec's definition); `mikebom:
  source-connection-ids` + `mikebom:deps-dev-match` now land on the
  component as properties. License shape emits
  `{"license": {"id": "<SPDX-id>"}}` for simple IDs and
  `{"expression": "..."}` for compound expressions.
- **PURL canonicalization.** Qualifiers are now sorted
  lexicographically per purl-spec. `+` is percent-encoded across
  every ecosystem. RPM `epoch=0` is dropped (semantically equivalent
  to no epoch; `rpm -qa` omits it).
- **Compositions emit both `assemblies` and `dependencies`** for each
  `complete` ecosystem record, plus a dep-completeness composition so
  sbomqs's `comp_with_dependencies` credits the primary component.
- **Primary-dependency fallback.** When the scanned project's root
  entry was filtered out (npm `path_key == ""`, cargo `source = None`)
  mikebom now synthesizes edges from the primary metadata.component to
  every orphan root. Without this, sbomqs reported "no dependency
  graph present" even when transitives were populated.
- **OS-release reader** prefers `<rootfs>/etc/os-release`, falls back
  to `<rootfs>/usr/lib/os-release` — fixes Ubuntu images where
  `/etc/os-release` is a relative symlink that dangles after
  tar-extraction.
- **Binary-scanner version-string scanner gated on
  `skip_file_level_and_linkage`** to suppress claimed-binary
  self-identification (curl reporting libcurl from `/usr/bin/curl`).
  Trade-off: static-library version detection inside claimed binaries
  is lost; see `docs/design-notes.md`.

### Fixed
- **Pre-PR verification gate** (Constitution v1.2.1). CI runs
  `cargo +stable clippy --workspace --all-targets` and
  `cargo +stable test --workspace`; skipping either locally before
  opening a PR now yields a reject cycle. Commit `6ec1cf3` (#9).
- **Cross-source deduplication + scan-target filter.** Resolves
  duplicate emissions when the same coord surfaces via multiple
  readers (e.g. Maven JAR walker + `.m2` cache + deps.dev). Commit
  `5c98ed2` (#3).
- **Go `go.sum` vs. BuildInfo divergence.** `go.sum` emissions are
  filtered against the companion binary's BuildInfo so dev-only
  transitives don't surface as runtime components. Commit `5b38b98`
  (#7).
- **Go component name alignment** across the source-tree and binary
  emission paths. Commit `ffa7d9f` (#6).
- **Maven version-aware artifact-presence gate** (M6). Commit
  `b4a9041` (#5).
- **Fat-jar heuristic gated on `co_owned_by.is_none()`** to avoid
  double-reporting. Commit `cb7f14e` (#4).
- **ELF-note ghost emissions.** Previously unconditional — a claimed
  Fedora binary emitted both `pkg:rpm/fedora/<subpackage>` (from
  rpmdb) and a ghost `pkg:rpm/rpm/<source-package>` (from the ELF
  `.note.package` section). Now gated on
  `skip_file_level_and_linkage`; unclaimed binaries respect a
  precedence `note.distro > os-release ID > hardcoded default`.
  Commit `3e5ab91`.
- **Cargo workspace-root false positive.** Commit `3e5ab91`.
- **`declared-not-cached` components dropped from `components[]` by
  default.** They remain in the dependency graph as references but are
  no longer materialized as standalone components. Commit `7688ddb`.
- **sbom-conformance findings + CDX 1.6 evidence serialization.**
  Commit `3cd55e3`.

## [0.1.0-alpha.2] and earlier

Earlier alpha milestones landed as a bootstrap commit
(`b0f31c1 feat: bootstrap mikebom + milestones 001-005`) and ship the
foundational work below. CHANGELOG entries below are a roll-up, not a
per-release breakdown.

### Feature 005 — PURL & scope alignment
- Distro qualifier shape standardized as `distro=<ID>-<VERSION_ID>`
  (matches packageurl-python reference tests); codename-required
  claims dropped from docs + tests.
- npm internals scoping: image scans include
  `node_modules/npm/node_modules/**`; path scans exclude.
  Always-on; not user-gated.
- RPM version-string normalization for canonical round-trip.

### Feature 004 — RPM binary SBOMs
- Standalone `.rpm` file scanning (feature 004 US1/US2).
- Generic binary reader for ELF / Mach-O / PE: linkage
  (`DT_NEEDED`, `LC_LOAD_DYLIB`, PE `IMPORT`) plus embedded
  version-string scanning for a curated 7-library list
  (OpenSSL / BoringSSL / zlib / SQLite / curl / PCRE / PCRE2).
- Legacy Berkeley-DB rpmdb parsing gated behind
  `--include-legacy-rpmdb` (feature 004 US4). Default-off.

### Feature 003 — multi-ecosystem expansion
- Go source + binary readers (`go.mod`, `go.sum`, module cache,
  `runtime/debug.BuildInfo` inline format).
- RPM rpmdb.sqlite pure-Rust reader (page/record/schema).
- Maven pom.xml parser with `<properties>` + `<dependencyManagement>`
  + BOM import resolution (`EffectivePom`, cycle-guarded memo).
- Cargo v3/v4 lockfile parser; v1/v2 refused.
- Gem `Gemfile.lock` indent-6 parser; `specifications/*.gemspec`
  walker catches Ruby stdlib/default gems invisible to Gemfile.lock.

### Feature 002 — Python + npm
- Python venv `dist-info/METADATA` reader; `poetry.lock`,
  `Pipfile.lock`, `requirements.txt` support with dev/prod
  distinction.
- npm `package-lock.json` v2/v3 + `pnpm-lock.yaml` + `node_modules/`
  tree walker. v1 lockfiles refused.

### Feature 001 — build-trace pipeline (experimental)
- eBPF capture of syscall + network events during a build. Requires
  CAP_BPF + CAP_PERFMON and Linux kernel ≥ 5.8. Produces in-toto
  attestations bound to the build event.

---

[Unreleased]: https://github.com/kusari-sandbox/mikebom/compare/v0.1.0-alpha.3...HEAD
[0.1.0-alpha.3]: https://github.com/kusari-sandbox/mikebom/releases/tag/v0.1.0-alpha.3
