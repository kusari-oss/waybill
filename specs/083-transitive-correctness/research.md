# Research — milestone 083 Transitive dep correctness audit per ecosystem

Six implementation-level decisions to pin before Phase 1 design + the per-ecosystem audit fixture-selection table that's the central deliverable.

## §1 — Tool version pinning

**Decision**: trivy `0.69.3` + syft `1.27.0` (the versions installed on the developer workstation at `/speckit.plan` time, 2026-05-07). Pin in research.md; CI installs the same versions; future re-audits document any version bumps.

**Rationale**: pinning prevents silent comparison-baseline drift. Edge counts change between trivy/syft releases (e.g., a trivy release that adds a new transitive resolver step would shift mikebom's "matches expected" classification to "minor differences" without any mikebom code change). Pinning makes the audit reproducible.

**Alternatives considered**:
- "Latest" rolling version — Rejected: edge counts shift silently; regression tests become flaky as upstream tools update.
- Multiple version-matrix testing — Rejected: scope creep; one pinned version per tool is sufficient for the audit's purpose.

## §2 — Per-ecosystem fixture selection

**Decision**: 11 vendored fixtures (pip splits into 3: poetry/pipfile/plain) under `mikebom-cli/tests/fixtures/transitive_parity/<ecosystem>/`, manifest+lockfile-only per the Q1 clarification. Each fixture's `README.md` cites the source URL + commit SHA the manifest was extracted from.

**Per-ecosystem fixture candidates** (final selection happens at fixture-extraction time during T-tasks; this list is a shortlist):

| Ecosystem | Candidate source repo | Why |
|---|---|---|
| **Go** | `kubernetes/cri-tools` (~300 deps in go.sum) | Real-world Kubernetes-ecosystem project; large transitive closure; well-pinned releases |
| **Cargo** | `clap-rs/clap` workspace (~80 deps) | Idiomatic Rust workspace project; non-trivial dep tree |
| **npm** | `expressjs/express` (~200 deps in package-lock.json v3) | Canonical npm reference; large transitive closure; lockfile-v3 (post-milestone-008 reader target) |
| **Maven** | `apache/commons-lang` | Real Apache project with parent POM chain |
| **pip-poetry** | `pypa/poetry` itself (poetry.lock) | Self-hosting; ~100 deps |
| **pip-pipfile** | A simple Pipfile.lock-using project (selection deferred) | Less common but encoded edges |
| **pip-plain** | A small `requirements.txt`-only project | Documents upstream limitation per FR-008; expected zero transitive edges |
| **gem** | `rubocop/rubocop` (Gemfile.lock; ~60 deps) | Real Ruby tooling project |
| **dpkg** | Debian 12 base container `/var/lib/dpkg/status` extract | Standard debian rootfs |
| **rpm** | Fedora 39 base container rpmdb extract | Standard fedora rootfs |
| **apk** | Alpine 3.20 base container `/lib/apk/db/installed` extract | Standard alpine rootfs |

**Rationale**: real-world coverage with bounded fixture size. Each fixture meets the ≥50 components / ≥100 edges threshold per FR-002.

**Alternatives considered**:
- Generated synthetic fixtures — Rejected per Q1 (less representative).
- Vendoring full source trees — Rejected per Q1 (unbounded repo growth).

## §3 — Audit harness implementation strategy

**Decision**: shared helper module at `mikebom-cli/tests/transitive_parity_common.rs` with the four invocation functions (run_mikebom / run_trivy / run_syft / run_source_format_direct) + the `diff_edge_sets` helper + the `assert_graceful_skip` env-var hook. Mirrors milestone-078's `spdx3_conformance.rs` graceful-skip pattern.

**Per-tool invocation pattern**:
```rust
fn run_trivy(fixture_path: &Path) -> Result<Vec<Edge>, AuditError> {
    let output = Command::new("trivy")
        .args(["fs", "--format", "spdx-json", fixture_path.to_str().unwrap()])
        .output()
        .context("invoking trivy")?;
    let sbom: SpdxDocument = serde_json::from_slice(&output.stdout)?;
    Ok(extract_edges_from_spdx_relationships(&sbom))
}
```

Symmetric for `run_syft` (uses `syft <path> -o spdx-json`) + `run_mikebom` (`cargo run -p mikebom-cli -- sbom scan --path <fixture> --format spdx-3-json --output -`).

**Edge extraction**: SPDX 2.3 `relationships[]` filtered to `relationshipType: "DEPENDS_ON"` → `(spdxElementId, relatedSpdxElement)` tuples. SPDX 3 `software_dependsOn` arrays → `(from_iri, to_iri)` tuples. PURL-normalized for cross-tool equality (since each tool may emit slightly different SPDX-IDs but the underlying PURLs match).

**Rationale**: minimum new infrastructure; reuses existing milestone-078 patterns; PURL-based comparison is the lowest-common-denominator across tools.

## §4 — Source-format direct-read tiebreaker dispatch (per Q2)

**Decision**: per-ecosystem dispatch in `run_source_format_direct(fixture_path, ecosystem)`:

| Ecosystem | Tiebreaker source | Implementation |
|---|---|---|
| Go | `go mod graph` if `go` on PATH | Subprocess; same as milestone-055 step 1 |
| Cargo | Parse `Cargo.lock` `dependencies = [...]` | TOML parser (`toml = "0.8"` already in deps); ~30 LOC |
| npm | Parse `package-lock.json` `packages[].dependencies` | `serde_json` parser; ~40 LOC |
| Maven | `mvn dependency:tree -DoutputType=text` if `mvn` on PATH | Subprocess |
| pip-poetry | Parse `poetry.lock` `[[package]]` blocks | TOML parser |
| pip-pipfile | Parse `Pipfile.lock` `default` + `develop` JSON | `serde_json` parser |
| gem | Parse `Gemfile.lock` GEM block | Custom parser (~30 LOC; gem lockfile is YAML-adjacent custom format) |
| dpkg | `dpkg-query --show -f='${Package} ${Depends}\n'` | Linux only; subprocess |
| rpm | `rpm -q --requires --all` | Linux only; subprocess |
| apk | `apk info -R` | Linux only; subprocess |

The tiebreaker is invoked **only when mikebom + trivy + syft disagree** on a specific edge. When unanimous agreement among the 3 SBOM tools, the tiebreaker is skipped (saves wall-time).

**Rationale**: the tiebreaker captures peer-tool bugs (e.g., trivy has known issues with certain Maven `<dependencyManagement>` cases); without it the audit only flags mikebom's deviations FROM trivy/syft consensus, missing cases where the peer tools are wrong.

## §5 — Graceful-skip + CI strict-mode pattern

**Decision**: env var `MIKEBOM_REQUIRE_TRANSITIVE_PARITY=1` set by CI lane; absent otherwise. When unset + a required tool is missing, the test prints `WARN tool not on PATH; skipping` + returns OK. When set + tool missing, the test fails with a clear diagnostic.

**Mirrors**: milestone-078's `MIKEBOM_REQUIRE_SPDX3_VALIDATOR` pattern verbatim. Same code shape; same operator UX.

**Per-test skip rules**:
- Tests requiring `trivy`: skip when trivy not on PATH (unless env var set).
- Tests requiring `syft`: same.
- Tests requiring `dpkg-query` / `rpm` / `apk`: skip on macOS unconditionally (these tools don't exist there); on Linux, follow the env-var rule.
- Tests requiring `mvn` (Maven tiebreaker): skip when mvn not on PATH.

**Rationale**: preserves developer-workstation experience (no forcing trivy/syft installs); CI lane enforces strict mode for regression detection.

## §6 — Indirect-vs-direct decision rubric (US3 / FR-004)

**Decision**: per-ecosystem decision matrix:

| Ecosystem | Source-format distinction | Mikebom current behavior | Audit decision |
|---|---|---|---|
| Go | `// indirect` marker in go.mod | All edges under root identically | **Defer** to follow-up issue. Mikebom's "all-edges-under-root" is operator-comprehensible; not a P1/P2 gap. |
| npm | `dependencies` vs `devDependencies` in package.json | Already mapped to milestone-052 lifecycle scope | **Verified — no new work**. Audit confirms milestone-052's mapping covers this case. |
| Cargo | `[dependencies]` vs `[dev-dependencies]` | Already mapped to milestone-052 lifecycle scope | **Verified — no new work**. Same as npm. |
| Other ecosystems | No native distinction | N/A | N/A |

**Rationale**: most distinctions are already covered by milestone-052's lifecycle-scope work. Go's `// indirect` is the one open question, but the gap is small enough to defer rather than gate this milestone on it.

**Alternatives considered**:
- Implement Go `// indirect` distinction in this milestone — Rejected: scope creep; out per FR-010.
- Document Go indirect as "deliberate divergence" with rationale — Considered; deferred to follow-up issue with the freedom to reverse if downstream tooling (e.g., dependency-track) gains a strong dependency on the distinction.

## §7 — Audit-row JSON-shape contract

**Decision**: each per-ecosystem entry in `research.md` follows the same structure (used by `data-model.md` + `contracts/audit-harness.md`):

```text
### Ecosystem: <name>

**Fixture**: tests/fixtures/transitive_parity/<ecosystem>/
**Source URL**: https://github.com/<org>/<repo>
**Commit SHA**: <40-char hex>
**Tool version**: trivy 0.69.3 / syft 1.27.0 / mikebom alpha.23

**Edge counts** (PURL-normalized):
- mikebom: N edges
- trivy: M edges
- syft: K edges
- source-format direct (when tiebreaker invoked): T edges

**Diff classification**: matches expected | minor differences | gap surfaced
**Tiebreaker resolution** (when invoked): mikebom correct | trivy correct | syft correct | source-format-says-X
**Indirect-vs-direct decision**: implement #N | document-as-divergence | deferred to #N | N/A

**Specific edge differences** (sample of up to 10 per category):
- Mikebom-only edges: ...
- Trivy-only edges: ...
- Syft-only edges: ...

**Follow-up disposition**: matches → no action | minor → tracked in regression test | gap → filed as #N
```

**Rationale**: consistent format makes the audit machine-readable AND human-readable. Operators reading any per-ecosystem entry know exactly what "the audit found" and what (if anything) is being done about it.

## §8 — Per-ecosystem audit findings

This section is populated incrementally as each per-ecosystem fixture's audit completes. Format per §7. Audit-only per FR-010 — gaps surface as follow-up issues; per-ecosystem-reader fixes ship as separate milestones.

### Ecosystem: cargo

**Fixture**: `mikebom-cli/tests/fixtures/transitive_parity/cargo/` — `Cargo.toml` + `Cargo.lock` only (per FR-002 + Q1).
**Source URL**: https://github.com/clap-rs/clap
**Commit SHA**: `2920fb082c987acb72ed1d1f47991c4d157e380d` (tag `v4.5.21`)
**Tool versions**: trivy 0.69.3 / syft 1.27.0 / mikebom alpha.24 (originally observed); post-milestone-087 numbers measured against alpha.26.

**Edge counts** (PURL-normalized, SPDX 2.3 `DEPENDS_ON` + `DEPENDENCY_OF` reverse-direction):
- mikebom: 317 (post-087; was 319 pre-087 — version-disambiguation fix and workspace-member emission shift the count)
- trivy: 85
- syft: 721
- source-format direct (tiebreaker not yet invoked): N/A

**Diff classification**: **gap surfaced** (one remaining; gap #1 closed by milestone 087)

The 3 SBOM tools disagree massively (317 / 85 / 721 post-087). Set-theoretic decomposition (alpha.24 baseline; post-087 numbers shift slightly because workspace-internal edges now resolve to the correct version, increasing agreement with trivy + syft):
- Agreement (all 3): 41 edges
- mikebom-only: 56 edges
- trivy-only: 0 edges (every trivy edge is also in mikebom or syft)
- syft-only: 721 edges (most of syft's set is unique to syft — likely transitive-edge over-emission per cargo-package source-tree heuristics rather than lockfile structure)
- mikebom + trivy (not syft): 0 edges
- mikebom + syft (not trivy): 200 edges
- trivy + syft (not mikebom): pre-087: 22 edges (included the workspace-internal `clap@4.5.21 → clap_builder@4.5.21` edge — see gap #1 below). Post-087: drops by ≥1 because mikebom now emits the workspace-internal edge correctly.

**Specific gaps surfaced (mikebom-side)**:

1. ~~**Workspace-member version mismatch**: mikebom emits `clap@4.5.21 → clap_builder@4.5.9` instead of `clap@4.5.21 → clap_builder@4.5.21`.~~ **Closed by milestone 087** (issue #172). Root cause: the cargo reader's `package_to_entry` and `workspace_root_deps` builder both stripped the disambiguating version from `dependencies = ["name version"]` lockfile entries, AND the `parse_lockfile` source-None skip dropped workspace members from the component set so the multi-version-same-name lookup couldn't resolve. Fix: preserve the `name version` form in both depends parsers + dual-key insert into `name_to_purl` + emit workspace members as components. See `specs/087-fix-cargo-workspace-version/`.
2. **clap_derive emits zero outgoing edges**: mikebom emits no DEPENDS_ON entries from `clap_derive`, despite Cargo.lock showing it depends on `proc-macro2`, `quote`, `syn`. Trivy + syft both emit those edges. Suggests the cargo reader skips proc-macro crate dep extraction entirely for procedural-macro-typed Cargo.toml entries. (Issue #173 — open.)

**Specific gaps surfaced (cross-tool)**:

3. **syft over-emits 721 edges trivy + mikebom don't see** — likely because syft's cargo classifier walks `Cargo.toml` `[dependencies]` of every package in the source tree (including dev-deps + build-deps), where mikebom + trivy filter to runtime-deps via the lockfile structure. Not a mikebom gap — more an upstream classification difference.
4. **trivy under-emits relative to mikebom** (85 vs 317) — trivy filters more aggressively than mikebom on cargo. Not a mikebom gap.

**Tiebreaker resolution** (planned for follow-up): re-derive ground truth from `Cargo.lock` by parsing `[[package]] dependencies = [...]` directly via the `toml` crate. Pre-implementation hypothesis: the Cargo.lock direct read will match mikebom's set on the agreement edges + match trivy + syft on the workspace-internal edges (gap #1, now closed) + match trivy + syft on the proc-macro edges (gap #2). Tiebreaker work tracked in follow-up issue.

**Indirect-vs-direct decision**: **N/A — already covered by milestone-052/part-2 lifecycle scope work** (per research §6). cargo's `[dev-dependencies]` vs `[dependencies]` distinction is mapped to CDX `scope: excluded` and SPDX 2.3 typed `DEV_DEPENDENCY_OF`.

**Follow-up disposition**: gap #1 closed by milestone 087 (issue #172). Gap #2 remains open as issue #173. The audit's regression test (`mikebom-cli/tests/transitive_parity_cargo.rs`) pins mikebom's post-087 317-edge count + 4 representative edges (the new one being `clap → clap_builder`, which encodes the version-disambiguation invariant via PURL-prefix matching); future cargo-reader fixes bump the baseline per quickstart.md Recipe 3.

### Ecosystem: npm

**Fixture**: `mikebom-cli/tests/fixtures/transitive_parity/npm/` — `package.json` + `package-lock.json` (lockfile generated via `npm install --package-lock-only` since express the library doesn't commit its own lockfile).
**Source URL**: https://github.com/expressjs/express
**Commit SHA**: `7e562c6` (tag `4.21.0`)
**Tool versions**: trivy 0.69.3 / syft 1.27.0 / mikebom alpha.24

**Edge counts** (cache-empty CI baseline, PURL-normalized):
- mikebom: 150
- trivy: 94
- syft: 0 (syft extracts no transitive edges from npm `package-lock.json`-only fixtures — likely needs `node_modules/` source-tree presence)

**Diff classification**: **minor differences** (mikebom + trivy roughly agree on shape; syft missing entirely)

mikebom over-emits relative to trivy (150 vs 94) — likely walking `package-lock.json` more aggressively. The over-emission is not obviously wrong, but worth investigating against a `npm ls --all` source-of-truth tiebreaker. syft emitting zero is a syft quirk (manifests-only fixtures aren't in its sweet spot).

**Indirect-vs-direct decision**: **N/A — already covered by milestone-052/part-2** (`devDependencies` vs `dependencies`).

**Follow-up disposition**: tracked in regression test (`transitive_parity_npm.rs`) at the 150-edge baseline. No follow-up issue filed yet — the over-emission relative to trivy needs source-format-direct tiebreaker work to confirm whether it's a mikebom over-emission or a trivy under-emission.

### Ecosystem: Go

**Fixture**: `mikebom-cli/tests/fixtures/transitive_parity/go/` — `go.mod` + `go.sum`.
**Source URL**: https://github.com/kubernetes-sigs/cri-tools
**Commit SHA**: `b5cf674` (tag `v1.32.0`)
**Tool versions**: trivy 0.69.3 / syft 1.27.0 / mikebom alpha.24

**Edge counts** (cache-empty CI baseline):
- mikebom: 31 (cache-empty — only direct deps from go.mod's `require` block)
- mikebom: 260 (when developer's box has `$GOMODCACHE` populated — milestone-055's 4-step ladder steps 1+2 fire)
- trivy: 142
- syft: 0

**Diff classification**: **gap surfaced** (mikebom cache-empty under-emits relative to trivy)

trivy emits 142 transitive edges from go.sum content alone, no module-cache lookups. mikebom's cache-empty fallback emits only the 31 direct edges from `require`. The 4-step ladder (milestone 055) was supposed to handle this via the go-mod-proxy fetch step, but that's disabled by `--offline`. Result: in offline + cache-empty mode (the CI scenario), mikebom misses the bulk of transitive edges.

**Specific gap**: mikebom's go reader, when offline + cache-empty, should be able to reconstruct transitive edges from `go.sum` content the same way trivy does (each `go.sum` entry encodes a package version — full transitive closure can be inferred from the union). This is exactly the "no-edges-fallback" case research §3 addresses, and it's NOT optimal.

**Indirect-vs-direct decision**: per research §6 — **defer** (Go's `// indirect` marker; mikebom's "all-edges-under-root" is operator-comprehensible; not P1/P2).

**Follow-up disposition**: **gap surfaced** — file follow-up for "Go reader: synthesize transitive edges from go.sum content when offline + cache-empty (currently emits direct-deps-only fallback)". Regression test pins the 31-edge cache-empty baseline.

### Ecosystem: pip-poetry

**Fixture**: `mikebom-cli/tests/fixtures/transitive_parity/pip_poetry/` — `pyproject.toml` + `poetry.lock` (poetry self-hosting per research §2).
**Source URL**: https://github.com/python-poetry/poetry
**Commit SHA**: `6a071c1` (tag `1.8.4`)
**Tool versions**: trivy 0.69.3 / syft 1.27.0 / mikebom alpha.24

**Edge counts** (cache-empty CI baseline):
- mikebom: 62
- trivy: 36
- syft: 138 (DEPENDENCY_OF — reverse direction; normalized in the extractor)

**Diff classification**: **minor differences** with one notable surprise

mikebom 62 vs trivy 36 — mikebom emits ~2× more edges. syft emits 138 (after reverse normalization). Three different edge counts across three tools on the same lockfile suggests each tool walks `poetry.lock` slightly differently — likely around handling of optional/extras dependencies, marker-conditional edges, etc.

**Indirect-vs-direct decision**: poetry's `[tool.poetry.dependencies]` is roughly equivalent to direct deps; `poetry.lock` blocks include source/dev classification. Already covered by milestone-052/part-2 lifecycle scope work.

**Follow-up disposition**: tracked in regression test at the 62-edge baseline. No follow-up issue filed — the 62/36/138 spread is documented but a source-format-direct tiebreaker is needed to resolve which classifier is most correct. Candidate follow-up: "audit pip-poetry edge classification across all 3 tools against `poetry show --tree` source-of-truth output."

### Ecosystem: Maven

**Fixture**: `mikebom-cli/tests/fixtures/transitive_parity/maven/` — `pom.xml` only (commons-lang has no parent POM at this version — root of its release lineage).
**Source URL**: https://github.com/apache/commons-lang
**Commit SHA**: `c8774fa` (tag `rel/commons-lang-3.14.0`)
**Tool versions**: trivy 0.69.3 / syft 1.27.0 / mikebom alpha.24

**Edge counts** (cache-empty CI baseline):
- mikebom: 0
- trivy: 0
- syft: 8 (DEPENDENCY_OF — reverse direction; normalized to forward)

**Diff classification**: **gap surfaced** — both mikebom + trivy emit zero edges from a real Maven project's pom.xml on cache-empty CI runs

mikebom + trivy both fail to emit dep edges from `pom.xml` alone when `~/.m2/repository/` is empty — they need the cached parent + dependency POMs to resolve transitive declarations. syft emits 8 edges, possibly via a different parsing strategy. mikebom additionally has the milestone-085 + earlier-discovered Maven-reader bugs (commons-lang3 emits a malformed `version: "64"` when `~/.m2/` IS populated, surfaced in the local-cache-populated audit run earlier).

**Specific gap**: mikebom's Maven reader needs (a) inline parent-POM-less transitive resolution OR (b) a deps.dev / Maven Central fallback for cache-empty cases. Currently it emits zero in this fixture.

**Indirect-vs-direct decision**: Maven `<scope>compile/test/provided/runtime</scope>` is mapped to lifecycle-scope work in milestone-052/part-2. No new decision.

**Follow-up disposition**: **gap surfaced** — file follow-up for "Maven reader: cache-empty offline mode emits zero transitive edges (and version-extracted-incorrectly when cache populated)". Regression test pins the 0-edge cache-empty baseline.

### Ecosystem: pip-plain

**Fixture**: `mikebom-cli/tests/fixtures/transitive_parity/pip_plain/` — synthetic 13-package `requirements.txt`.
**Source**: synthesized for milestone 083 per FR-008 (no real-world fixture needed — the upstream limitation is the entire point of the test).
**Tool versions**: trivy 0.69.3 / syft 1.27.0 / mikebom alpha.24

**Edge counts** (cache-empty CI baseline):
- mikebom: 0
- trivy: 0
- syft: 0

**Diff classification**: **matches expected** (the milestone's only matches-expected classification)

Per FR-008: plain `requirements.txt` has no native way to express transitive structure. All 3 tools correctly emit zero transitive edges from this fixture. Future tools that synthesize edges heuristically (e.g., querying PyPI for each package's runtime deps) would deviate — that's exactly what the regression test catches.

**Indirect-vs-direct decision**: N/A — `requirements.txt` doesn't encode the distinction.

**Follow-up disposition**: **no action** — matches expected behavior; tracked in regression test as a stability anchor against future heuristic over-emission.

### Ecosystem: Gem

**Fixture**: `mikebom-cli/tests/fixtures/transitive_parity/gem/` — `Gemfile` + `Gemfile.lock`. fastlane committed its lockfile at HEAD, sidestepping the rubocop bundle-lock-needs-Ruby-3+ issue (macOS dev box has system Ruby 2.6).
**Source URL**: https://github.com/fastlane/fastlane
**Commit SHA**: tag `2.224.0`
**Tool versions**: trivy 0.69.3 / syft 1.27.0 / mikebom alpha.24

**Edge counts** (cache-empty CI baseline):
- mikebom: 196
- trivy: 196
- syft: 0

**Diff classification**: **minor differences** (mikebom + trivy in tight agreement; syft missing entirely)

mikebom + trivy agree on 196 edges (tightest agreement we've seen across the audit). Off-by-one on package count (151 vs 152) — likely a single component classification difference, not a dep-graph issue. syft emits zero edges from `Gemfile.lock` alone — same pattern as npm and Go (manifests-only fixtures aren't syft's sweet spot).

**Indirect-vs-direct decision**: Gemfile's `:development` / `:test` group classification is mapped to lifecycle-scope work in milestone-052/part-2. No new decision.

**Follow-up disposition**: tracked in regression test at the 196-edge baseline. The mikebom/trivy agreement here is encouraging — gem is one of the better-audited ecosystems by mikebom's reader. No follow-up issue filed.

### Ecosystem: pip-pipfile (deferred)

**Fixture status**: deferred — research §2 shortlisted "selection deferred" originally; finding a real-world Pipfile-using project with an active commit to a stable tag wasn't completed in this session. Candidates for the follow-up cycle: simple flask apps, real-python tutorials, or a synthesized Pipfile + Pipfile.lock fixture generated via `pipenv lock` on a small dependency set.

**Diff classification**: pending fixture extraction.

### Ecosystems: dpkg / rpm / apk (deferred to Linux CI)

**Fixture status**: deferred to Linux CI runs per FR-009 + the macOS-skip pattern. Extraction requires container-image rootfs access which Docker Desktop on macOS can't reliably mount across `/tmp` (file-sharing config issue surfaced in this session). The Linux CI lane will:

1. Pull `debian:12`, `fedora:39`, `alpine:3.20` base images.
2. Extract `/var/lib/dpkg/status`, `/var/lib/rpm/`, `/lib/apk/db/installed` into the respective fixture dirs.
3. Run mikebom + trivy + syft against each fixture (trivy + syft both have native OS-package classifiers).
4. Populate per-ecosystem audit rows here.

**Diff classification**: pending fixture extraction in Linux CI.

## §9 — Summary table (incremental)

Per-ecosystem audit progress as of milestone-083 in-flight commit. Updated as each ecosystem's row lands above.

| Ecosystem | Status | mikebom edges | trivy edges | syft edges | Classification |
|---|---|---|---|---|---|
| cargo | ✅ done | 319 | 85 | 721 (DEP_OF) | gap surfaced (×2) |
| npm | ✅ done | 150 | 94 | 0 | minor differences |
| Go | ✅ done | 31 (cache-empty) | 142 | 0 | gap surfaced |
| pip-poetry | ✅ done | 62 | 36 | 138 (DEP_OF) | minor differences |
| Maven | ✅ done | 0 (cache-empty) | 0 | 8 (DEP_OF) | gap surfaced |
| pip-plain | ✅ done | 0 | 0 | 0 | **matches expected** |
| gem | ✅ done | 196 | 196 | 0 | minor differences |
| pip-pipfile | ⏳ deferred | TBD | TBD | TBD | (follow-up session) |
| dpkg | ⏳ deferred | TBD | TBD | TBD | (Linux CI) |
| rpm | ⏳ deferred | TBD | TBD | TBD | (Linux CI) |
| apk | ⏳ deferred | TBD | TBD | TBD | (Linux CI) |

**Filed follow-up issues** (post-audit):
- **#172** — cargo: workspace-member version mismatch (clap@4.5.21 → clap_builder@4.5.9). **Closed by milestone 087.**
- **#173** — cargo: proc-macro crates emit zero outgoing edges (clap_derive case)
- **#174** — Go: cache-empty offline mode emits direct-only fallback (31 vs trivy's 142)
- **#175** — Maven: cache-empty offline mode emits zero transitive edges + version-extraction bug

**Open questions deferred to follow-up source-format-tiebreaker work**:
- **npm** — mikebom's 150 edges vs trivy's 94 — needs `npm ls --all` source-of-truth comparison
- **pip-poetry** — 62 / 36 / 138 spread — needs `poetry show --tree` source-of-truth comparison

**Tight-agreement ecosystems** (no follow-up needed):
- **gem** — mikebom 196 vs trivy 196 (one-component-count off-by-one), classification: minor differences
- **pip-plain** — unanimous zero (matches expected per FR-008)
