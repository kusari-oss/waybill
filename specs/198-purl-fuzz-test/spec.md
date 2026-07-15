# Feature Specification: Versionless PURL Round-Trip Fuzz Test

**Feature Branch**: `198-purl-fuzz-test`
**Created**: 2026-07-15
**Status**: Draft
**Input**: User description: "pr-c" (m197 US4 carved out as a standalone milestone — the fuzz-test slice deferred from m197's split-PR delivery)

## Overview

Milestone 197 spec called for a fuzz-style round-trip test covering all 11
ecosystems mikebom emits (npm, cargo, maven, gem, pip, composer, dart,
cocoapods, scala, haskell, erlang) with ≥ 100 synthetic versionless PURL
inputs per ecosystem — as an exhaustive corner-case sweep the targeted
per-ecosystem unit tests miss. m197 shipped as three PRs (epoch fixes,
6-ecosystem versionless extension, and this deferred fuzz-test slice);
this milestone is that deferred slice landing as a standalone spec-driven
delivery. Closes #566.

The fuzz test IS the deliverable: a new test file exercising the `Purl`
type's parse → re-serialize round-trip across 1100+ synthetic inputs. It
catches per-ecosystem corner cases the targeted unit tests miss — URL-
encoded segments, max-length names, scoped-name grammars, ecosystem-
specific normalization quirks — and prevents future non-determinism
regressions in the `Purl` newtype from shipping silently.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Fuzz suite covers all 11 ecosystems, catches serialization drift (Priority: P1)

A mikebom maintainer refactors the `Purl` newtype's serialization code
path — perhaps to add a new ecosystem, tighten a validation rule, or
optimize a hot path. Before the change would ship silently as a
consumer-facing wire-shape regression, the fuzz test surfaces it: at
least one synthetic input across the 1100 total invocations produces a
byte-drift between the input string and the parsed-then-reserialized
output. The maintainer sees a diagnostic naming the ecosystem, the input
name shape, the observed vs expected values.

**Why this priority**: This IS the whole feature. The m197 spec's SC-003
called for ≥ 1100 total fuzz invocations across all 11 ecosystems; that
delivery is the entirety of this milestone.

**Independent Test**: `cargo test -p mikebom-common versionless_purl_fuzz`
completes with zero failures and prints diagnostic per-ecosystem
invocation counts showing ≥ 100 each.

**Acceptance Scenarios**:

1. **Given** the fuzz-test suite on a clean checkout (`main` post-m197
   PR-B),
   **When** `cargo test -p mikebom-common versionless_purl_fuzz` runs,
   **Then** every ecosystem's synthetic inputs round-trip byte-identically
   AND the test emits an INFO-level diagnostic showing ≥ 100 invocations
   per ecosystem for a total of ≥ 1100.

2. **Given** an intentional regression in the `Purl::new` /
   `Purl::as_str` code path (e.g., a maintainer accidentally strips
   URL-encoded `%2F` on serialization),
   **When** the fuzz suite runs post-regression,
   **Then** at least one synthetic input across the 11 ecosystems
   surfaces a byte-drift failure with an actionable diagnostic naming
   (a) which ecosystem, (b) which name shape variant, (c) observed vs
   expected string values.

3. **Given** the fuzz suite integrated into the standard `cargo test
   --workspace` invocation,
   **When** `./scripts/pre-pr.sh` runs,
   **Then** the fuzz test contributes ≤ 5 seconds of wall-clock to the
   pre-PR gate (matches m195/m196/m197 SC-004 threshold pattern).

---

### Edge Cases

- **Empty name segment**: the fuzz catalog includes empty-string names
  where the ecosystem's grammar allows/disallows. Skip synthesis (invalid
  PURL) when the ecosystem grammar rejects — do NOT panic.
- **Max-length name per ecosystem**: npm 214-char, cargo 200-char, maven
  200-char, etc. The catalog exercises the boundary + one over the
  boundary. The over-boundary case should fail to parse (Purl::new
  returns `Err`); the fuzz test tolerates this by skipping those inputs.
- **Unicode names**: most ecosystems reject; catalog includes at least
  one unicode input per ecosystem to verify the rejection is graceful
  (not a panic).
- **Nested scope segments**: composer `vendor/pkg`, npm `@scope/name`,
  maven `com.example:artifact` — each tested for both single-segment and
  multi-segment shapes.
- **Percent-encoded characters in names**: names like `foo bar` (space
  encoding as `%20`) or `foo+bar` (`+` encoding as `%2B`). Verify the
  Purl newtype's canonicalization preserves encoding.
- **Digit-prefix names**: some ecosystems reject; the catalog exercises
  the class per ecosystem to confirm rejection is deterministic.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: A test file MUST exist at `mikebom-common/tests/versionless_purl_fuzz.rs`
  (or an equivalent path chosen by the plan phase) implementing a fuzz-
  style round-trip test.
- **FR-002**: The fuzz suite MUST exercise ≥ 100 synthetic versionless
  PURL inputs per ecosystem across all 11 ecosystems mikebom emits
  (npm, cargo, maven, gem, pip, composer, dart, cocoapods, scala,
  haskell, erlang) — total ≥ 1100 invocations.
- **FR-003**: For each synthetic input, the suite MUST assert (a) the
  input parses successfully via `Purl::new()` OR is deliberately
  rejected by ecosystem grammar (test tolerates rejection); (b) parsed
  → re-serialized round-trip is byte-identical to the input;
  (c) `.ecosystem()` and `.name()` accessors return the expected values.
- **FR-004**: On any assertion failure, the suite MUST emit a diagnostic
  naming (a) the ecosystem, (b) the name shape variant that failed,
  (c) the observed value vs expected value.
- **FR-005**: The suite MUST use a hand-rolled catalog-driven generator
  — no new Cargo dependencies. Property-based testing crates
  (`proptest`, `quickcheck`) are explicitly rejected per m197 research
  §R3.
- **FR-006**: The suite MUST run as part of the default `cargo test
  --workspace` invocation (no opt-in gate). Wall-clock contribution to
  the standard test suite MUST be ≤ 5 seconds per the pre-PR gate
  budget.
- **FR-007**: The `Purl` newtype at `mikebom-common/src/types/purl.rs`
  MUST NOT change in this milestone. If the fuzz suite surfaces a
  genuine `Purl` bug, fix it in a follow-up milestone; document the
  finding in this PR body but keep the fuzz-test milestone scoped.

### Key Entities

- **Fuzz Input Catalog**: A `const &[(&str, &[&str])]` (or equivalent
  static structure) mapping ecosystem type → array of name shape
  variants. Exact shape TBD in plan phase.
- **Fuzz Diagnostic**: Structured output emitted per-failure, naming
  ecosystem + shape + observed + expected. Format TBD in plan phase.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: `cargo test -p mikebom-common versionless_purl_fuzz`
  completes with zero failures on a clean m197-post-merge checkout.
- **SC-002**: The suite exercises ≥ 100 unique synthetic input strings
  per ecosystem — verified by an INFO-level per-ecosystem invocation-
  count diagnostic printed at test-run end.
- **SC-003**: A deliberately-introduced regression in the `Purl`
  newtype's serialization surfaces at least one failure in the fuzz
  suite (validation via maintainer-side smoke test: revert a Purl
  serialization line, run the suite, observe non-zero failures).
- **SC-004**: `./scripts/pre-pr.sh` wall-clock delta ≤ 5 seconds vs
  pre-m198 baseline (matches the SC-006 wall-clock threshold pattern
  from m195 / m196 / m197).
- **SC-005**: Follow-up issue #566 is closed by the m198 PR via
  `Closes #566` in the commit / PR body.

## Assumptions

- **`Purl` newtype is stable**: the fuzz test is a *check* of the
  `Purl` type's behavior, not a driver of `Purl` changes. If the
  fuzz surfaces a real `Purl` bug, the milestone bounds that finding
  as scope-out and files a follow-up rather than expanding to include
  a `Purl` fix.
- **Catalog is exhaustive-enough at 100 shapes per ecosystem**: 100
  variants covers ecosystem grammar corners without exceeding the 5s
  wall-clock budget. If a real-world corner case surfaces later
  that's NOT in the catalog, extending the catalog is a follow-up.
- **No new Cargo deps**: per m197 research §R3 audit. `proptest`
  would add a workspace dep for value the deterministic catalog
  covers.
- **Test runs in the default lane, not opt-in**: per FR-006. Unlike
  the m195/m196 public-corpus tests (which are opt-in due to network
  cost), the fuzz test is pure-computation and fast enough to run
  always.
- **The `Purl` newtype's `.as_str()` returns the canonical form**:
  the fuzz test's round-trip check assumes `Purl::new(s).as_str() == s`
  when `s` is already canonical. If `Purl` normalizes-on-construct
  in ways that deviate from the input's canonical form, the fuzz test's
  catalog inputs need to be pre-normalized to the canonical form for
  the round-trip assertion to make sense — the plan phase resolves
  this by inspecting `Purl` behavior first.
