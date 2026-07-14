# Feature Specification: Fix Graph-Completeness Over-Firing on Operator-Supplied Roots

**Feature Branch**: `192-operator-root-completeness`
**Created**: 2026-07-14
**Status**: Draft
**Input**: User description: "let's just fix A" (option A from the graph-completeness regression triage: when the root is an operator override producing a `pkg:generic/` PURL, synthesize per-ecosystem placeholder roots so `ecosystems_without_root` becomes empty and the `MultiEcosystemPartialRoot` classifier stops incorrectly forcing `partial` on Go/npm/etc. source-tree scans).

## Clarifications

### Session 2026-07-14

- Q: What tracing log level should the placeholder-root synthesis emit at? → A: INFO level (Option A). One line per scan reporting `synthesized N per-ecosystem placeholder roots for operator-override scan`. Consistent with the existing `graph completeness computed` INFO log at `builder.rs:498` and matches the m190/m191 observability convention. Visible by default so operators debugging "why did my SBOM's graph-completeness value change?" don't need `RUST_LOG=debug`.
- Q: How should the fix detect "the ecosystem the operator picked" when `--root-purl-type <eco>` is set? → A: Parse the root PURL's ecosystem segment (Option A). If the target root's PURL is non-generic (e.g., `pkg:golang/X` set via `--root-purl-type golang`), SKIP synthesis for that ecosystem so we don't emit a duplicate placeholder root. Matches operator intent directly by reading what the operator wrote; avoids coupling synthesis to the `ResolvedRootSubject` enum's internal variants (future-variant-proof).

## User Scenarios & Testing *(mandatory)*

### User Story 1 - CI SBOM generation with `--root-name` no longer reports `partial` on well-formed source scans (Priority: P1)

An operator running mikebom in a CI pipeline against a source-tree repository (Go / npm / pip / cargo / any single-ecosystem or multi-ecosystem project) with `--root-name <stable-name> --root-version <build-id>` receives an SBOM whose `mikebom:graph-completeness` annotation is `complete` when the dep graph is actually complete — i.e., when every emitted component is reachable from the root via the emitted `dependencies[]` edges (CDX 1.6), `DEPENDS_ON` relationships (SPDX 2.3), or `dependsOn` graph elements (SPDX 3). Downstream tools (vulnerability scanners, policy engines, SBOM quality graders, consumers of the `mikebom:graph-completeness` signal) then correctly identify well-formed SBOMs as complete instead of misclassifying them as `partial`.

**Why this priority**: Reported by a downstream consumer (Kusari's pico test corpus) that ALL 4 of their source-repo SBOM fixtures now show `partial` — plus their pico-server image (alpine-based) — leaving only pure OS-image scans (postgres:16) showing `complete`. This is a customer-visible regression that broke a stable consumer contract: consumers built pipelines that expect the `mikebom:graph-completeness` signal to distinguish well-formed vs incomplete SBOMs. Since m158 shipped the classifier (2026-06), the signal has degenerated into "essentially all SBOMs are partial" — the signal is stuck-at-partial for the common CI-generation pattern (operator supplies `--root-name` for stable identity).

**Independent Test**: Scan a Go source repo with `mikebom sbom scan --path <repo> --root-name X --root-version Y --format cyclonedx-json`. Assert the emitted document's `metadata.properties[?(@.name=="mikebom:graph-completeness")].value == "complete"` AND that `metadata.properties[?(@.name=="mikebom:graph-completeness-reason")]` is either absent or empty. Repeat across `--format spdx-2.3-json` and `--format spdx-3-json` — the same signal MUST fire in all three formats.

**Acceptance Scenarios**:

1. **Given** a Go source repo with a valid `go.mod` and `go.sum` where all transitive modules resolve, **When** scanned with `--root-name X --root-version Y --format cyclonedx-json`, **Then** `mikebom:graph-completeness` is `complete` and no `graph-completeness-reason` annotation is emitted.
2. **Given** an npm source repo with a valid `package.json` and `package-lock.json` where all transitive deps resolve, **When** scanned with `--root-name X --root-version Y`, **Then** `mikebom:graph-completeness` is `complete`.
3. **Given** a mixed-ecosystem source repo (e.g., a Go project with a small Node/npm test harness), **When** scanned with `--root-name X --root-version Y`, **Then** `mikebom:graph-completeness` is `complete` — the operator-supplied root MUST NOT cause per-ecosystem-root gaps to fire on Go OR npm.
4. **Given** the same repo, **When** scanned WITHOUT `--root-name` (native root detection — Go picks the module name), **Then** the `mikebom:graph-completeness` value is byte-identical to the pre-fix output (no behavior change on the native-root path).
5. **Given** a source repo with a truly incomplete dep graph (e.g., a `go.mod` that references a module not in `go.sum` and can't be fetched), **When** scanned with `--root-name X --root-version Y`, **Then** `mikebom:graph-completeness` STILL reports `partial` with an appropriate reason code — the fix doesn't paper over real gaps.

---

### Edge Cases

- **Single-ecosystem repo with operator override**: pico-style Go-only repo scanned with `--root-name pico --root-version <sha>`. The synthetic root is `pkg:generic/pico@<sha>`. Per-ecosystem placeholder synthesis MUST add a `golang` placeholder root so `ecosystems_without_root` doesn't contain `golang`.
- **Multi-ecosystem repo with operator override**: pico-server-style scan with Go + npm + alpine (from a ko-built image). Synthesis MUST add placeholders for `golang`, `npm`, AND `apk` — one per ecosystem present in `components[]`.
- **Operator override with `--root-purl-type golang`**: when the operator picks a non-generic ecosystem for the root (e.g., `pkg:golang/X` via `--root-purl-type golang`), the synthesis MUST detect this by parsing the root PURL's ecosystem segment (per Q2 answer A) and SKIP synthesis for that ecosystem — no duplicate placeholder. The root PURL itself already fills the per-ecosystem-root slot for its ecosystem.
- **Native root detection (no `--root-name`)**: the fix MUST be a no-op when the root selector picks a `MainModule` from `components[]` — the pre-existing per-ecosystem-root logic already handles that case correctly. Byte-identity of goldens on the native-root path is a hard requirement.
- **Empty component list**: an empty SBOM is trivially complete; the synthesis MUST NOT introduce a false-positive root or fire the classifier.
- **Only file-tier components** (no PURL-carrying components, m133 file-tier only): file-tier components lack ecosystem semantics; synthesis MUST NOT fire for the `generic` ecosystem specifically (only ecosystems with real PURL-typed components should get placeholder roots).
- **Container image scans**: pure OS-image scans (dpkg-only, rpm-only, apk-only) that currently return `complete` MUST continue to return `complete` — those scans don't use `--root-name` in the same way and their `ecosystems_without_root` is already correctly empty via native root selection.
- **Real gaps still surface**: if BFS actually can't reach every component even AFTER the placeholder-root synthesis (e.g., a Go module in `components[]` but with no incoming edge and no outgoing edge in the assembled Relationships), the classifier MUST STILL fire `OrphanedComponentsDetected` for those specific components. The fix targets the false-positive rate, not the classifier's ability to detect real orphans.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: When the primary root selection result's `subject` variant is NOT a `MainModule` (i.e., it is `OperatorOverride`, `SyntheticPlaceholder`, `MavenCoord`, or any other non-component-derived variant), System MUST synthesize a per-ecosystem placeholder root for EVERY ecosystem present in the emitted component list, such that `ecosystems_without_root` becomes empty for those ecosystems.
- **FR-002**: The per-ecosystem placeholder root's identity MUST be the operator-supplied (or synthetic) root's PURL string — i.e., the same string threaded as `target_ref` — so BFS traversal from that identity via the existing primary-dep-fallback mechanism reaches every component that IS reachable from the root.
- **FR-003**: The synthesis MUST NOT introduce a new component into the emitted `components[]` list — the placeholder root is a purely internal BFS-seeding construct that lives only in the graph-completeness pass. Emission byte-shape of the SBOM (components + dependencies + metadata) is unchanged.
- **FR-004**: The synthesis MUST NOT fire when the primary root selection result's `subject` IS a `MainModule` variant — the native-root path already picks a per-ecosystem root correctly and this milestone's fix is a no-op on that path (native-root byte-identity gate per SC-004).
- **FR-005**: The `generic` ecosystem MUST continue to be excluded from `ecosystems_without_root` per the existing carve-out logic (mod.rs:171). File-tier components (no PURL ecosystem) MUST NOT trigger synthesis for a `generic` ecosystem entry.
- **FR-006**: When the classifier no longer fires the false-positive `MultiEcosystemPartialRoot` for an operator-override scan, and no other reason code applies, the emitted `mikebom:graph-completeness` value MUST be `complete` and the `mikebom:graph-completeness-reason` annotation MUST be absent.
- **FR-007**: When BFS still detects actual orphaned components AFTER placeholder-root synthesis (residual orphans not reachable from the synthesized roots via any edge path), the classifier MUST STILL fire `OrphanedComponentsDetected` for those specific components — the fix must not suppress real orphan detection.
- **FR-008**: The fix MUST NOT introduce a new `mikebom:*` annotation — the existing `mikebom:graph-completeness` and `mikebom:graph-completeness-reason` annotations carry all the needed signal. Per CLAUDE.md Principle V (standards-native fields take precedence over `mikebom:*` properties).
- **FR-009**: A single INFO-level `tracing` log line MUST fire once per scan whenever the operator-override synthesis path executes (per Q1 answer A, Session 2026-07-14). Message shape: `synthesized N per-ecosystem placeholder roots for operator-override scan` (with `N` = the count of ecosystems synthesized). Emitted at the same tier as the existing `graph completeness computed value=... reachable_count=... total_count=... reason_codes=...` log at `builder.rs:498`. No DEBUG-per-ecosystem detail line — the summary count is sufficient for CI-log analysis; operators wanting per-ecosystem detail can grep the components list.
- **FR-010**: Existing goldens where the ROOT is a native `MainModule` (Go module, npm workspace root, etc.) MUST pass byte-identically — the fix is scoped strictly to the operator-override path.
- **FR-011**: Goldens (or test fixtures) that previously exercised the `MultiEcosystemPartialRoot` code path on operator-override scans MAY be updated as a documented part of this milestone — the expected value flips from `partial` (false positive) to `complete` (correct signal).

### Key Entities *(include if feature involves data)*

- **RootSelectionResult**: the existing struct returned by `mikebom-cli/src/generate/root_selector/`. Consumed by the graph-completeness pass; its `subject: ResolvedRootSubject` variant drives the fix's dispatch (fire on non-`MainModule`, no-op on `MainModule`).
- **Per-ecosystem placeholder root**: an ephemeral in-memory record that lives only inside the graph-completeness pass. Represented as an entry `(ecosystem_name, target_ref_purl)` in the existing `per_ecosystem_root: HashMap<String, String>` inside `build_ecosystem_root_set`. NOT persisted, NOT emitted into the SBOM.
- **`ecosystems_without_root`**: the existing `Vec<String>` field on `EcosystemRootSet`. Post-fix, this list is empty for every scan whose root is an operator-supplied override — the classifier's `MultiEcosystemPartialRoot` guard depends on this list being non-empty to fire.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Scanning the Kusari pico test corpus (or any equivalent Go source repo scanned with `--root-name X --root-version Y`) produces `mikebom:graph-completeness: complete` when the underlying dep graph is complete. Measured by re-scanning the 4 pico source-repo fixtures (kusari-cli, pico, guac, molcajete) and asserting all 4 flip from `partial` → `complete`.
- **SC-002**: The pico-server-image scan (mixed-ecosystem: apk + go + npm from a ko-built alpine image) produces `complete` when its dep graph is complete. Currently reports `partial: multi-ecosystem-partial-root: apk`.
- **SC-003**: No false-negative regression on the mikebom golden corpus — every existing golden that previously reported `complete` MUST continue to report `complete` after the fix.
- **SC-004**: Byte-identity gate for the native-root path: every existing golden whose root is a native `MainModule` variant produces byte-identical emission before and after m192. Measured by running the workspace regression suite: zero drift on any golden that doesn't touch the operator-override path.
- **SC-005**: Real-orphan detection preserved: a fixture with a genuinely orphaned component (e.g., an npm design-tier component with no source-tier resolution AND no incoming edges) STILL reports `partial` with an `OrphanedComponentsDetected` reason code — the fix doesn't over-correct and hide real gaps.
- **SC-006**: Consumer observability: the reason-code annotation is either absent (clean-complete case) OR contains only ACTIONABLE codes (real orphans, real transitive gaps) — not spurious `multi-ecosystem-partial-root` firings caused by operator-supplied roots.

## Assumptions

- The graph-completeness classifier's `MultiEcosystemPartialRoot` reason code fires ONLY when `ecosystems_without_root` is non-empty AND orphans exist in those ecosystems. Making `ecosystems_without_root` empty for operator-override scans eliminates the false positive without needing to touch the reason-code emission logic itself.
- The primary-dep-fallback logic at `mod.rs:186-199` already synthesizes edges from `target_ref` to every "graph-top" component (component not depended-on by any relationship). Once BFS traverses through those synthesized edges, it reaches every component that's reachable via the assembled dep graph. Empty `ecosystems_without_root` means the classifier trusts that traversal instead of second-guessing it based on "missing per-ecosystem root" heuristics.
- The `RootSelectionResult.subject` enum's variants (`MainModule`, `OperatorOverride`, `SyntheticPlaceholder`, `MavenCoord`, others) are stable across the codebase; the fix's dispatch on `!MainModule` will remain correct through future variant additions unless a new "native-root" variant is added without updating this pass.
- No new Cargo dependencies are required. The fix touches `mikebom-cli/src/generate/graph_completeness/bfs.rs::build_ecosystem_root_set` and possibly `mod.rs` — pure in-memory transformation over existing data structures.
- No new `mikebom:*` annotations are introduced (FR-008); the existing signal channels carry the corrected value.
- The Kusari pico test corpus is the primary real-world validation target; if the corpus itself isn't accessible for automated verification in mikebom's test suite, a synthetic Go source fixture with the same shape (Go module + operator-override root) serves as the in-repo validation.
- Per Q&A during the triage session: this fix targets option A (per-ecosystem placeholder synthesis) rather than option B (threshold-based partial), C (classifier-emitter fallback alignment), D (downgrade to informational), or E (revert m177). The operator-intent argument for option A: when the operator passes `--root-name X`, they're explicitly saying "the root of this SBOM is X" and asking mikebom to accept that as authoritative; the classifier should honor that intent rather than fight it.
- The fix is orthogonal to m177's `TransitiveEdgesUnresolvable` classifier (which fires on design-tier / analyzed-tier components without source-tier siblings — a different, real signal). m177 may continue to fire independently for its intended cases; this milestone only patches the `MultiEcosystemPartialRoot` false-positive path.
