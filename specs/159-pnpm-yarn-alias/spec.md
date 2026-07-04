# Feature Specification: pnpm/yarn npm-alias syntax support in dep-graph edges

**Feature Branch**: `159-pnpm-yarn-alias`
**Created**: 2026-07-04
**Status**: Draft
**Input**: User description: "493" (implement fix for [issue #493](https://github.com/kusari-oss/mikebom/issues/493))

## Motivation

Both pnpm-lock.yaml (v9 snapshots) and yarn.lock (v1) support a lockfile syntax where a local dep name aliases to a different real package. mikebom currently emits the alias-name as the edge target — no matching component exists → the graph resolver silently drops the edge. Discovered during the milestone-157 Round-2 audit of `kusari-sandbox/test-*` repos.

Real-world alias shapes observed 2026-07-04:

**Pnpm v9 (test-podman-desktop)**:

```yaml
snapshots:
  '@docusaurus/core@3.10.1(...)':
    dependencies:
      react-helmet-async: '@slorber/react-helmet-async@1.3.0(react-dom@18.3.1(react@18.2.0))(react@18.2.0)'
      react-loadable: '@docusaurus/react-loadable@6.0.0(react@18.2.0)'

  '@isaacs/cliui@8.0.2':
    dependencies:
      string-width: 5.1.2
      string-width-cjs: string-width@4.2.3
      strip-ansi: 7.2.0
      strip-ansi-cjs: strip-ansi@6.0.1
```

**Yarn v1 (test-guac-visualizer)**:

```
"@cosmograph/cosmos@^1.1.1", "@cosmograph/cosmos@npm:@cosmos.gl/graph":
  version "2.6.4"
  resolved "https://registry.yarnpkg.com/@cosmos.gl/graph/-/graph-2.6.4.tgz#..."
```

In both cases, the LOCAL name (e.g. `react-helmet-async`, `string-width-cjs`, `@cosmograph/cosmos`) is not the actual installed package. The RESOLVED package (`@slorber/react-helmet-async@1.3.0`, `string-width@4.2.3`, `@cosmos.gl/graph@2.6.4`) is what code runs against — and what vulnerability scanners need to check.

mikebom's current behavior:

- Emits a component under the LOCAL name (`pkg:npm/string-width-cjs@4.2.3`) or under an alias-name-plus-empty-version phantom PURL (per issue #498).
- Emits edges FROM the depender pointing at the local name.
- Graph resolver looks up `(ecosystem, name)` → no matching component → edge silently dropped.

Empirical impact (from the milestone-157 Round-2 audit against 3 `kusari-sandbox/test-*` repos):

- `test-podman-desktop` (pnpm v9): **6 dropped edges** (0.22% of 2668 snapshots)
- `test-guac-visualizer` (yarn v1): **1 dropped edge** (0.11% of 909 entries)
- `test-rails` (yarn v1): **3 dropped edges** (0.39% of 762 entries)

Low percentage, high cross-cutting importance. Every dropped alias-edge is a supply-chain analysis miss — a consumer scanning for CVEs against `@slorber/react-helmet-async` gets nothing because mikebom emitted the edge under the wrong name.

## User Scenarios & Testing

### User Story 1 - SBOM consumer sees resolved-package identity for aliased deps (Priority: P1)

A supply-chain consumer (Kusari Inspector, a vulnerability scanner, a compliance auditor) loads mikebom's SBOM for a repo containing pnpm-lock v9 alias syntax and looks up the vulnerability status of `@slorber/react-helmet-async` — the ACTUAL installed package that `react-helmet-async` aliases to. They MUST find a component with that resolved-package PURL AND MUST find `@docusaurus/core@3.10.1` depending on it.

**Why this priority**: This is the observed bug's user-visible symptom. Without this fix, security-critical alias-resolutions are silently dropped from the SBOM's dep-graph, producing false-negative vulnerability scans. Given that mikebom's core value proposition is supply-chain correctness (Constitution Principle VIII — Completeness), this is a P1 correctness fix.

**Independent Test**: Scan a pnpm v9 repo where a snapshot entry has `react-helmet-async: '@slorber/react-helmet-async@1.3.0(...)'`. In the emitted CDX SBOM, assert:

1. A component exists with `purl = pkg:npm/%40slorber/react-helmet-async@1.3.0`.
2. The depender's `dependsOn` list includes `pkg:npm/%40slorber/react-helmet-async@1.3.0`.
3. No orphaned edge pointing at `pkg:npm/react-helmet-async@` (with the local name) survives to the emitted SBOM.

**Acceptance Scenarios**:

1. **Given** a pnpm v9 snapshot entry with `local-name: 'aliased@version(peer-suffix)'` (quoted-string value form), **When** mikebom scans and produces a CDX SBOM, **Then** the depender's dependsOn MUST reference the aliased package's canonical PURL (e.g. `pkg:npm/%40slorber/react-helmet-async@1.3.0`) — NOT the local-name PURL.

2. **Given** a pnpm v9 snapshot entry with `local-name: aliased@version` (unquoted-string value form, e.g. `string-width-cjs: string-width@4.2.3`), **When** mikebom scans, **Then** the depender's dependsOn MUST reference the aliased package's canonical PURL (e.g. `pkg:npm/string-width@4.2.3`).

3. **Given** a yarn v1 lockfile key `"local-name@spec", "local-name@npm:aliased-name":`, **When** mikebom scans, **Then** the emitted component's PURL MUST be the aliased-name-based canonical PURL (e.g. `pkg:npm/%40cosmos.gl/graph@2.6.4`), NOT the local-name PURL (`pkg:npm/%40cosmograph/cosmos@...`).

4. **Given** a yarn v1 lockfile entry emitted as `@cosmos.gl/graph@2.6.4` per acceptance scenario 3, **When** another package (`hosted-server-mgmt` in the test-guac-visualizer testbed) declares a dep on the LOCAL name (`@cosmograph/cosmos "^1.1.1"`), **Then** the depender's dependsOn MUST reference the aliased-canonical PURL (`pkg:npm/%40cosmos.gl/graph@2.6.4`) — so the graph is fully connected via the RESOLVED identity.

5. **Given** the test-podman-desktop pnpm-lock (2668 snapshots, 6 known alias-affected snapshots per the milestone-157 audit), **When** mikebom scans and BFS-traverses the emitted graph, **Then** ALL 6 previously-dropped alias-edges MUST be present in the emitted dependencies array, targeting their resolved canonical PURLs.

---

### User Story 2 - SBOM consumer sees the alias-provenance signal for auditability (Priority: P2)

A compliance auditor reviewing the mikebom SBOM wants to know that `pkg:npm/%40slorber/react-helmet-async@1.3.0` was reached via the `react-helmet-async` alias (rather than being a direct dep). mikebom emits a `mikebom:pnpm-alias` OR `mikebom:yarn-alias` component-scope annotation carrying the original local-name so the audit trail survives PURL canonicalization.

**Why this priority**: Constitution Principle X (Transparency). Consumers auditing the emitted graph should be able to programmatically identify which components originated from an alias, so they can (a) verify the resolution is correct, (b) trace back to the lockfile source, and (c) understand the ecosystem's naming indirection. Standards-native precedence per Principle V: no CDX 1.6 / SPDX 2.3 / SPDX 3.0.1 native property expresses "this package identity was reached via a lockfile alias," so a `mikebom:*` annotation is the correct carrier.

**Independent Test**: For every component whose resolution went through an alias, verify:

- The component's `properties[]` (CDX) / `annotations[]` (SPDX 2.3) / `Annotation` element (SPDX 3) MUST include `mikebom:pnpm-alias = <local-name>` OR `mikebom:yarn-alias = <local-name>` (whichever lockfile ecosystem triggered).
- The value MUST be the exact local-name string as it appeared in the lockfile (byte-identical, including any special characters like `-cjs` suffixes or scoped-name `@` prefixes).

**Acceptance Scenarios**:

1. **Given** a pnpm-alias-resolved component (`pkg:npm/%40slorber/react-helmet-async@1.3.0`), **When** mikebom emits the SBOM, **Then** the component's properties MUST include `mikebom:pnpm-alias = "react-helmet-async"`.

2. **Given** a yarn-alias-resolved component (`pkg:npm/%40cosmos.gl/graph@2.6.4`), **When** mikebom emits the SBOM, **Then** the component's properties MUST include `mikebom:yarn-alias = "@cosmograph/cosmos"`.

3. **Given** a component that has multiple aliases pointing at it (theoretically possible in a monorepo where two workspace peers alias the same real package under different local names), **When** mikebom emits the SBOM, **Then** the annotation MAY appear multiple times OR MAY carry a comma-joined value (implementation choice, documented in the annotation's parity contract).

4. **Given** any component NOT reached via an alias, **When** mikebom emits the SBOM, **Then** the component MUST NOT carry `mikebom:pnpm-alias` OR `mikebom:yarn-alias` (avoids noise for the healthy majority case).

---

### User Story 3 - Non-alias repos continue to work unchanged (Priority: P3)

Users scanning repos with NO alias syntax (the vast majority of npm/pnpm/yarn projects) see **byte-identical** SBOM output vs. pre-159. No new annotations, no PURL changes, no edge re-attribution.

**Why this priority**: Regression guard. The alias-support code path MUST be dormant when no aliases are present — SC-002 milestone-157/158 dual-side byte-identity precedent.

**Independent Test**: Regenerate all 11 milestone-090 goldens with the milestone-159 code. Diff against pre-159 goldens. Zero diff bytes on any golden (aliases are NOT present in milestone-090 fixtures per audit).

**Acceptance Scenarios**:

1. **Given** the milestone-090 npm fixture (single-package pnpm v6, no aliases), **When** mikebom scans, **Then** the emitted CDX diff vs. pre-159 is exactly ZERO bytes.

2. **Given** any milestone-090 fixture from a non-npm ecosystem, **When** mikebom scans, **Then** same as above.

3. **Given** any milestone-158-emitted golden (post-158 baseline that has the graph-completeness annotation), **When** mikebom scans with milestone-159 code, **Then** the emitted output for a no-alias input MUST be byte-identical to the milestone-158 baseline.

### Edge Cases

- **Pnpm v9 alias where the aliased name is itself scoped (`@slorber/react-helmet-async`)** — the alias value string starts with `@` AND contains a further `@version` separator. Parser MUST correctly split at the LAST `@` after the scope prefix (matches the existing `parse_pnpm_key` convention at `pnpm_lock.rs:129`).

- **Pnpm v9 alias with peer-dep suffix (`'@slorber/react-helmet-async@1.3.0(react-dom@18.3.1(react@18.2.0))(react@18.2.0)'`)** — parser MUST strip the parenthesised peer-dep suffix per the existing pnpm peer-suffix convention, resulting in the canonical `pkg:npm/%40slorber/react-helmet-async@1.3.0`.

- **Yarn v1 alias where the local-name spec is ALSO used elsewhere without alias (`"@cosmograph/cosmos@^1.1.1", "@cosmograph/cosmos@npm:@cosmos.gl/graph":` where SOME dep declares `@cosmograph/cosmos "^1.1.1"` and OTHER dep declares the same range)** — every one of those deps MUST resolve to the aliased package's PURL, not the local-name PURL. Yarn's semantics: the LOCAL-NAME spec resolves through the alias.

- **Circular alias reference (theoretically: `A: B@1.0.0`, `B: A@1.0.0`)** — should never occur in a valid lockfile (npm registry doesn't accept it), but defensive: mikebom MUST NOT infinite-loop; detect the cycle + emit a warn diagnostic + skip the alias-resolution.

- **Alias to a package that isn't itself in the lockfile snapshot** — the local name `react-helmet-async` aliases to `@slorber/react-helmet-async@1.3.0`, but `@slorber/react-helmet-async@1.3.0` might not be a top-level snapshot entry (only reached via the alias). mikebom MUST emit the aliased-canonical component even if it's ONLY referenced through aliases.

- **Alias-across-ecosystem** (e.g., pnpm alias points at a yarn-shape package spec) — MUST reject with a parse-warn; each ecosystem's alias syntax is well-defined within its own lockfile grammar.

- **Value shape that IS the local name (`react: react@18.2.0`) — not really an alias, just self-reference** — this is a lockfile normalization thing that pnpm does sometimes. mikebom's canonical resolution treats these as normal edges to the canonical PURL, no special alias annotation needed (per US2 P2's "no `mikebom:pnpm-alias` when no alias was used" rule).

## Requirements

### Functional Requirements

- **FR-001**: mikebom's pnpm-lock parser (`mikebom-cli/src/scan_fs/package_db/npm/pnpm_lock.rs`) MUST detect the pnpm alias syntax in dep VALUES: `local-name: 'aliased-name@version(peer-suffix)'` (quoted form) AND `local-name: aliased-name@version` (unquoted form). Detection MUST NOT rely on a `@` count heuristic; instead, MUST parse the value using the existing `parse_pnpm_key` convention (LAST `@` after any scope prefix as the version separator).

- **FR-002**: mikebom's yarn-lock parser MUST detect the yarn alias syntax in KEY specs: `local-name@npm:aliased-name` where the alias is the KEY's version-spec portion. Detection MUST parse the yarn v1 key line (comma-separated quoted specs) and identify any spec whose version-part starts with `npm:`.

- **FR-003**: When an alias is detected, mikebom MUST emit the RESOLVED-package component (with the aliased-name-based canonical PURL, e.g. `pkg:npm/%40slorber/react-helmet-async@1.3.0`) INSTEAD OF the local-name component. The local-name PURL MUST NOT appear in the emitted `components[]` array.

- **FR-004**: When an alias is detected, mikebom MUST emit dependency edges FROM the depender TO the RESOLVED-package canonical PURL. Edges pointing at the local-name PURL MUST NOT appear in the emitted `dependencies[]` array.

- **FR-005**: When another dep references the local-name (e.g. `hosted-server-mgmt` depending on `@cosmograph/cosmos "^1.1.1"` per yarn edge case), mikebom MUST rewrite that edge's target to the aliased-canonical PURL so the graph is fully connected via the RESOLVED identity.

- **FR-006**: For every alias-resolved component, mikebom MUST emit a component-scope annotation:
  - CDX 1.6: `properties[]` entry `{"name": "mikebom:pnpm-alias" | "mikebom:yarn-alias", "value": "<original-local-name>"}`.
  - SPDX 2.3: per-package `Annotation` with `comment = "mikebom:pnpm-alias=<local-name>"` OR `"mikebom:yarn-alias=<local-name>"`.
  - SPDX 3.0.1: per-package `Annotation` with `statement = "mikebom:pnpm-alias=<local-name>"` OR `"mikebom:yarn-alias=<local-name>"`.

- **FR-007**: The `<original-local-name>` value in FR-006 annotations MUST be the exact byte-identical string as it appeared in the lockfile (preserving scope prefix `@`, hyphens, `-cjs` suffixes, etc.). No URL-encoding, no case normalization.

- **FR-008**: When NO alias is detected in a lockfile (the vast majority case), mikebom MUST NOT emit the FR-006 annotations. Byte-identity vs pre-159 is preserved (SC-002 regression guard).

- **FR-009**: Both `mikebom:pnpm-alias` and `mikebom:yarn-alias` MUST be registered as new parity-catalog rows in `mikebom-cli/src/parity/extractors/` with `Directionality::SymmetricEqual` and `order_sensitive: false` — matching the milestone-127 / milestone-134 / milestone-158 per-format annotation-emission pattern.

- **FR-010**: When mikebom detects an alias-value string that FAILS to parse (e.g. malformed pnpm alias, invalid yarn spec), mikebom MUST emit a warn-level tracing log with fields `lockfile`, `local_name`, `raw_value`, and gracefully degrade by SKIPPING that entry (no edge emitted for the malformed alias) rather than crashing OR emitting an incorrect PURL.

- **FR-011**: mikebom MUST emit an info-level tracing log line at the end of alias-resolution per lockfile with fields `lockfile_path`, `alias_count`, `alias_ecosystem` (`pnpm` or `yarn`). The message MUST be the literal string `"npm-alias resolution completed"`. Grep-friendly for CI-log analysis; follows the milestone-157 FR-007 / milestone-158 FR-013 precedent.

- **FR-012**: If the same component is reached via MULTIPLE aliases (theoretically possible in a monorepo where two workspace peers alias the same real package under different local names), mikebom MUST emit the FR-006 annotation MULTIPLE times on the component — one per distinct local-name. Wire format: each format's native property/annotation MAY appear multiple times with the same `name` field per the format's spec.

- **FR-013**: The two new annotations MUST comply with FR-010 of milestone 158 (standards-native precedence per Constitution Principle V). If either CDX 1.6 or SPDX 3.0.1 later introduces an official "package-alias" property, mikebom MUST prefer that property. As of 2026-07-04, no such standard property exists; the `mikebom:*` prefix is used.

### Key Entities

- **Local name**: The dep name as it appears in the depender's `dependencies:` section. May differ from the package's actual installed identity (that's the whole point of an alias).

- **Aliased name (a.k.a. Resolved name)**: The actual npm-registry package name that the local name resolves to. This is what code runs against and what vulnerability scanners need to check.

- **`mikebom:pnpm-alias` / `mikebom:yarn-alias`**: Component-scope annotations naming the local-name a component was reached via. Ecosystem-specific to distinguish pnpm's value-side alias syntax from yarn's key-side npm-alias syntax (they're different lockfile grammars).

## Success Criteria

### Measurable Outcomes

- **SC-001 (test-podman-desktop alias coverage)**: After milestone 159 ships, mikebom's CDX output on `kusari-sandbox/test-podman-desktop` MUST include ALL 6 alias-affected snapshot entries identified in the milestone-157 Round-2 audit, with edges correctly pointing at the aliased-canonical PURLs. Specific spot-checks:
  - `pkg:npm/%40docusaurus/core@3.10.1` `dependsOn` MUST include `pkg:npm/%40slorber/react-helmet-async@1.3.0` (via `react-helmet-async` alias).
  - `pkg:npm/%40docusaurus/core@3.10.1` `dependsOn` MUST include `pkg:npm/%40docusaurus/react-loadable@6.0.0` (via `react-loadable` alias).
  - `pkg:npm/%40isaacs/cliui@8.0.2` `dependsOn` MUST include `pkg:npm/string-width@4.2.3` (via `string-width-cjs` alias) AND `pkg:npm/strip-ansi@6.0.1` (via `strip-ansi-cjs` alias).

- **SC-002 (test-guac-visualizer + test-rails yarn alias coverage)**: After milestone 159 ships, mikebom's CDX output on `kusari-sandbox/test-guac-visualizer` MUST emit `pkg:npm/%40cosmos.gl/graph@2.6.4` as a component (not `pkg:npm/%40cosmograph/cosmos@...`), and the top-level scan-target's dep on `@cosmograph/cosmos` MUST resolve to `pkg:npm/%40cosmos.gl/graph@2.6.4`. Similar checks apply to test-rails' 3 alias-affected entries.

- **SC-003 (dual-side byte-identity guard, mirrors milestone 158)**: For every milestone-090 non-alias-containing golden fixture (all 11 ecosystems), the emitted CDX / SPDX 2.3 / SPDX 3 SBOMs MUST be byte-identical to pre-159. Zero diff bytes. The milestone-090 fixtures don't contain alias syntax (verified during specification-authoring 2026-07-04) so this SC is easily achievable.

- **SC-004 (alias-provenance annotation on 100% of alias-resolved components)**: 100% of components emitted via an alias-resolution MUST carry the `mikebom:pnpm-alias` OR `mikebom:yarn-alias` annotation with the correct local-name value. Zero false negatives on this annotation.

- **SC-005 (BFS reachability improvement on test-podman-desktop)**: After milestone 159 ships, running `mikebom sbom scan --path test-podman-desktop --format cyclonedx-json` and BFS-traversing from `metadata.component.bom-ref` MUST reach **≥708 npm components** (a +10-component improvement from the milestone-158 baseline of 698). The number 6 comes from the observed alias count in the audit; the +10 buffer accounts for transitive-of-alias reachability (edges downstream of newly-connected alias targets that were previously unreachable).

- **SC-006 (pre-PR gate)**: `cargo +stable clippy --workspace --all-targets -- -D warnings` and `cargo +stable test --workspace` MUST both pass with zero errors before the PR is opened. The mandatory `./scripts/pre-pr.sh` gate must be green.

- **SC-007 (unit test coverage)**: The pnpm-lock + yarn-lock alias-parsing code paths MUST have at least 12 unit tests covering: (a) pnpm quoted-value alias (`react-helmet-async: '@slorber/react-helmet-async@1.3.0(peers)'`); (b) pnpm unquoted-value alias (`string-width-cjs: string-width@4.2.3`); (c) pnpm alias with scoped local-name (`@some-scope/name: @other-scope/other@2.0.0`); (d) pnpm alias with scoped aliased-name; (e) yarn v1 key-spec alias (`"@cosmograph/cosmos@^1.1.1", "@cosmograph/cosmos@npm:@cosmos.gl/graph":`); (f) yarn v1 alias where local-name is scoped; (g) yarn v1 alias where aliased-name is scoped; (h) malformed pnpm alias (FR-010 warn-and-skip); (i) malformed yarn alias (FR-010 warn-and-skip); (j) alias-provenance annotation emission on CDX; (k) alias-provenance annotation on SPDX 2.3; (l) alias-provenance annotation on SPDX 3.

- **SC-008 (integration test)**: A new integration test at `mikebom-cli/tests/npm_alias_resolution.rs` MUST synthesize a mixed-alias workspace (pnpm + yarn) via `tempfile::tempdir` + `std::fs::write` (matches milestone-157/158 pattern), scan it via the release binary, and assert BFS reachability from the primary root is 100% through the alias-resolved edges.

- **SC-009 (CHANGELOG entry)**: `CHANGELOG.md` MUST document the alias-resolution fix + FR-006 annotation vocabulary + the 3-repo empirical audit numbers + a consumer jq recipe for gating on `mikebom:pnpm-alias` / `mikebom:yarn-alias`.

- **SC-010 (parity catalog registration)**: The two new annotations MUST have parity-catalog entries with `Directionality::SymmetricEqual` per FR-009. Milestone-071 parity check MUST pass symmetrically across CDX / SPDX 2.3 / SPDX 3.

- **SC-011 (issue #493 closure)**: Issue #493 MUST reference this milestone (`closes #493` in the impl commit message) and the milestone MUST demonstrably resolve the reported symptom (6 + 1 + 3 = 10 previously-dropped alias-edges now emitted correctly across the 3 test repos).

## Assumptions

- **Aliased-name is authoritative for PURL identity**: When an alias is detected, mikebom emits the component under the ALIASED-NAME PURL, not the LOCAL-NAME PURL. Rationale: the aliased name is the actual npm-registry identity, and consumers doing vulnerability lookup key on the registry identity. The `mikebom:*-alias` annotation preserves the local-name for audit trail.

- **The pnpm alias-value shape is unambiguous with existing `parse_pnpm_key` semantics**: The existing `parse_pnpm_key` function at `pnpm_lock.rs:129` already handles `<name>@<version>` parsing with the LAST-`@`-after-scope-prefix convention. Alias-VALUE parsing reuses this without introducing a new grammar. Verified by empirical inspection of test-podman-desktop's 6 alias entries.

- **The yarn v1 alias-KEY shape uses `@npm:` as the specifier prefix**: The observed shape `"@cosmograph/cosmos@npm:@cosmos.gl/graph"` uses `npm:` as the alias marker inside the version-spec. Other alias forms (e.g. `github:`, `file:`) also exist in yarn but are OUT OF SCOPE for this milestone (they're not npm-alias syntax; they're different resolution mechanisms).

- **Milestone-090 fixtures don't contain alias syntax**: Verified 2026-07-04 by grep against all `pnpm-lock.yaml` / `yarn.lock` / `package-lock.json` files in milestone-090's fixture tree. This means SC-003 byte-identity guard is trivially achievable — no fixture regeneration needed.

- **No new Cargo dependencies**: Following the milestone-157/158 precedent, this work uses existing crates only (`serde_yaml`, `serde_json`, `tracing`, `anyhow`, `regex` if any, `clap` — no new flags needed).

- **BFS reachability metric alignment**: SC-005's ≥708 target on test-podman-desktop is empirically-adjustable per the milestone-156/157/158 revision pattern. If T-level measurement shows a different actual number, spec updates inline at T-completion time.

- **`mikebom:pnpm-alias` and `mikebom:yarn-alias` are ecosystem-specific**: Rather than a single `mikebom:npm-alias` for both. Rationale: the two lockfile grammars are DIFFERENT (pnpm's value-side vs yarn's key-side syntax), and downstream tooling may want to filter by ecosystem for audit. Consumers who don't care can `.startswith("mikebom:") and .endswith("-alias")` to catch both.

## Out of Scope

- **The phantom empty-version edges fix (issue #498)** — separate milestone. Even after 159 lands, the 159-unaffected phantom edges (`pkg:npm/%40docusaurus/core@` shape) will still cap BFS reachability on test-podman-desktop.

- **The Go workspace-mode false-edge fix (issue #494)** — separate milestone.

- **The Go transitive coverage gap (issue #495)** — separate milestone.

- **The Ruby built-in gem edge fix (issue #496)** — separate milestone.

- **Non-npm alias forms in yarn (`github:`, `file:`, `link:`)** — these are DIFFERENT resolution mechanisms than npm-aliases; a package resolved via `github:` isn't sourced from the npm registry. Separate milestone if consumer demand emerges.

- **npm-shim aliases in `package-lock.json`** — npm's package-lock format supports a similar aliasing syntax (`"depname": {"name": "aliased-name", ...}`). This is a DIFFERENT lockfile grammar and a separate reader. Out of scope for milestone 159 unless empirical audit surfaces uncovered cases.

- **Aliasing that spans MULTIPLE ecosystems** (e.g., pnpm alias points at yarn spec) — theoretically undefined and never observed in the wild. Rejected at parse time per Edge Case bullet.

- **Alias-provenance-based BFS gating** — consumers might want to skip alias-resolved components during BFS traversal for specific analyses. Not a mikebom concern; consumers can filter on the `mikebom:*-alias` property themselves.

- **Cross-repo alias-detection consistency** — mikebom-158 emits alias annotations per-scan; there's no across-scan aggregation. Users comparing SBOMs from two similar-but-distinct repos won't get a diff signal specific to alias-usage — they'll see per-component annotation drift on individual components (which is fine).
