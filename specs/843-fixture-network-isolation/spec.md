# Feature Specification: Test fixtures must not depend on network reachability

**Feature Branch**: `843-fixture-network-isolation`
**Created**: 2026-09-12
**Status**: Draft
**Input**: Issue #843 — "Go test fixtures trigger real network resolution, making scan timings unstable"

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A contributor measuring performance gets a number they can trust (Priority: P1)

A contributor changes something that might affect scan speed and measures the effect by scanning this repository. Today the non-enrichment portion of that scan costs anywhere between half a second and twenty-three seconds depending on nothing they control, because fixture modules that can never resolve are looked up over the network anyway.

After this change, the same scan costs the same amount twice in a row, and a difference they measure is a difference they caused.

**Why this priority**: This is the reported defect, and it is not hypothetical harm. During milestone 839 three separate speed-up figures were derived from measurements resting on this floor and all three were wrong. Unstable measurement does not merely slow work down — it produces confident, incorrect conclusions, which is worse than producing none.

**Independent Test**: Scan this repository twice with enrichment disabled and compare the wall times. They must agree closely, and neither may depend on whether the machine has a working internet connection.

**Acceptance Scenarios**:

1. **Given** this repository and a working network, **When** a contributor scans it twice with enrichment disabled, **Then** the two wall times differ by a small margin rather than by an order of magnitude.
2. **Given** this repository and **no** network at all, **When** a contributor scans it with enrichment disabled, **Then** the scan completes in about the same time as it does with a network, and produces the same components.
3. **Given** a scan of this repository, **When** the contributor inspects what network requests it made, **Then** none of them are attempts to resolve a fixture module.

---

### User Story 2 - Fixtures that exist to test unresolvable modules keep doing so (Priority: P1)

Some fixtures are deliberately unresolvable: they exist to exercise what waybill does when a module cannot be found — the fallback ladder, the `unresolved` markers, the degraded-coverage annotations. Those behaviours are real and are covered by tests today.

After this change, those tests still test the same thing. Making every fixture resolve would delete that coverage silently, trading one measurement problem for a correctness blind spot.

**Why this priority**: Equal to Story 1, because the cheapest way to satisfy Story 1 is to make every fixture resolvable, and that would quietly remove the only coverage waybill has of its own failure paths. A fix that passes its own acceptance test by deleting someone else's is not a fix.

**Independent Test**: Identify every test that asserts on unresolved, fallback, or degraded-resolution behaviour. Each must still fail if the behaviour it covers regresses.

**Acceptance Scenarios**:

1. **Given** a test asserting that an unresolvable module produces a fallback marker, **When** this change lands, **Then** that test still passes and still fails when the fallback is broken.
2. **Given** a fixture whose unresolvability is incidental scaffolding rather than the subject of a test, **When** this change lands, **Then** it may become resolvable, and the tests that use it still pass.

---

### User Story 3 - A contributor scanning the repo locally does not wait on the internet (Priority: P2)

A contributor scans this repository during ordinary development — checking an SBOM, reproducing a bug, trying a flag. That should not spend seconds on lookups that cannot succeed, and should not behave differently on a train.

**Why this priority**: Lower than the others because it is friction rather than incorrectness, and contributors can work around it by passing a flag. It is included because the friction is paid by everyone, constantly, and the fix for Stories 1 and 2 delivers it for free.

**Independent Test**: Scan this repository on a machine with no network and confirm no operation waits on a timeout.

---

### Edge Cases

- A fixture module is referenced from more than one place, and is made resolvable in one of them. Resolution must not become order-dependent, or a test passes or fails according to which fixture the walker reached first.
- A fixture is made self-contained and the local reference later breaks — a moved directory, a renamed module. This must fail loudly at test time rather than silently falling back to a network lookup that also fails, which would restore the original problem wearing a disguise.
- The repository is scanned from an extracted archive rather than a working tree (`git archive`, a release tarball, a container build context). Self-containment must survive that, since it is how the corpus and benchmark harnesses obtain the tree.
- A contributor adds a new Go fixture. The problem must not silently return; there must be something that notices.
- A fixture is deliberately unresolvable **and** performance-sensitive — for example one used by a benchmark. Its cost must be bounded without making it resolvable.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Scanning this repository MUST NOT issue network requests on behalf of test fixtures.
- **FR-002**: The wall time of a scan of this repository with enrichment disabled MUST NOT depend on network conditions.
- **FR-003**: Every fixture whose unresolvability is the subject of a test MUST remain unresolvable, and the tests covering it MUST continue to detect regressions in the behaviour they assert.
- **FR-004**: Fixtures whose unresolvability is incidental MUST be made self-contained — resolvable without leaving the repository.
- **FR-005**: The distinction between the two MUST be recorded per fixture, so a later contributor can tell which kind they are editing without re-deriving it.
- **FR-006**: Self-containment MUST survive extraction of the repository to another location, including `git archive` output and container build contexts.
- **FR-007**: A broken local reference in a self-contained fixture MUST fail visibly at test time, and MUST NOT degrade into a network lookup.
- **FR-008**: A newly added fixture that reintroduces network-dependent resolution MUST be detected rather than discovered later by someone measuring performance.
- **FR-009**: This change MUST NOT alter waybill's production behaviour for real, resolvable modules. The resolution ladder, the proxy path and the fallback markers are unchanged; only the fixtures and the harness around them move.
- **FR-010**: Fixtures that remain deliberately unresolvable MUST have their resolution cost bounded, so that "deliberately unresolvable" does not mean "unboundedly slow".

### Key Entities

- **Fixture module**: a module declared under the test-fixture tree. Either *incidental* (its identity is scaffolding; tests care about something else) or *deliberate* (its unresolvability is the thing under test).
- **Resolution attempt**: one lookup the toolchain performs for a module. The unit of cost this feature removes.
- **Scan floor**: the wall time of a scan with all enrichment disabled. The quantity Story 1 is about; it should be a property of the repository, not of the network.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Two consecutive scans of this repository with enrichment disabled differ in wall time by less than 20%, measured on a machine with a working network.
- **SC-002**: The same scan on a machine with **no** network completes within 20% of the networked time, and emits the same component count.
- **SC-003**: A scan of this repository issues **zero** network requests attributable to fixture-module resolution, down from 20 per scan.
- **SC-004**: Every test that currently asserts on unresolved, fallback or degraded-resolution behaviour still fails when that behaviour is deliberately broken.
- **SC-005**: A newly added fixture that resolves over the network is detected before it reaches the default branch.

## Assumptions

- **Measured, not assumed** (2026-09-12, this repository):
  - 21 of 27 Go fixtures declare module paths that cannot resolve, all under `example.com` or `github.com/waybill-fixture`.
  - A scan makes **20** failed resolution invocations, each reaching the network.
  - The scan floor with enrichment disabled moved between **5.13s and 23.53s** across runs; the same scan fully offline is **0.48s**.
- **Renaming the domain is not the fix, and the original issue said it was.** #843 proposed a reserved-for-testing domain as "probably cheapest". Measured, that changes a single resolution from 1.40s to 1.31s — about 6% — because the toolchain consults the module proxy before it ever contacts the module's own host. The proxy round-trip is the cost and it is independent of the module path. Recorded here because the recommendation is wrong and a reader of the issue would otherwise act on it.
- Two approaches do work, measured on the same probe: refusing the proxy entirely (**0.01s**), and declaring a local replacement so the module resolves inside the tree (**0.01s**, and the dependency graph resolves rather than failing). No fixture uses a local replacement today.
- These are different in kind, not just in mechanism: refusing the proxy keeps fixtures unresolvable and merely makes failure fast, preserving exactly what tests see today; a local replacement makes them resolve, which changes which waybill code path the fixture exercises. Which applies depends on whether a given fixture's unresolvability is the point — hence FR-003 and FR-004.
- The repository's own test suite is the only consumer of these fixtures. No downstream user depends on their module paths.
- `git archive` is the extraction path used by the benchmark and corpus harnesses, so FR-006 is a live constraint rather than a hypothetical one.
