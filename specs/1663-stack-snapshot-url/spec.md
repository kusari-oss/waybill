# Feature Specification: Stack snapshots pinned by URL

**Feature Branch**: `1663-stack-snapshot-url`
**Created**: 2026-11-26
**Status**: Draft
**Input**: #1678. Refs #1678.

## Context

A Stack project pins its package set to a *snapshot*, usually by resolver name (`resolver: lts-18.8`). It can also pin one by URL. waybill emits each snapshot named in a `stack.yaml.lock` as a component.

**Measured** (#1678), in iand675/hs-opentelemetry at `25f5b2d65744af384e06f61040e5c037d7cb8db9` (the corpus target `hpack-hs-opentelemetry`, #1677). `instrumentation/yesod/stack.yaml.lock` has a snapshot whose `original` and `completed` both carry:

```
url: https://raw.githubusercontent.com/commercialhaskell/stackage-snapshots/master/lts/18/8.yaml
```

and no `resolver`. The scan emits `pkg:generic/unknown@93a10755…` for it, with:
- the name `unknown`;
- `waybill:source-files` empty;
- no edge from any project, and no orphan reason.

The corpus invariant layer reports it as I7 (unreachable and unexplained), and the target's declarations file records that as a known failure.

**Today, read from the code:**
- A snapshot is named from `original.resolver` alone, with `unknown` as the fallback.
- A resolver-named `lts-22.0` becomes `pkg:generic/stackage-lts-22.0@<sha256>`.
- No snapshot component records the lock file it came from.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A URL-pinned snapshot is named (Priority: P1)

A consumer reading the SBOM of a Stack project that pins its snapshot by URL sees which snapshot it is, not `unknown`.

**Why this priority**: an unnamed component tells a consumer nothing, and `unknown` would collide across unrelated snapshots that share no identity.

**Independent Test**: a `waybill-fixture-*` project with four lock files: an `lts` URL, a `nightly` URL, a non-stackage URL, and a resolver name. Each gives the identity this spec defines.

**Acceptance Scenarios**:

1. **Given** a snapshot pinned by `…/stackage-snapshots/master/lts/18/8.yaml`, **When** scanned, **Then** it gets the same identity as one pinned by `resolver: lts-18.8` with the same hash.
2. **Given** a snapshot pinned by `…/stackage-snapshots/master/nightly/2024/5/7.yaml`, **When** scanned, **Then** it is identified as `nightly-2024-05-07`, the same as `resolver: nightly-2024-05-07`.
3. **Given** a snapshot pinned by any other URL, **When** scanned, **Then** its identity keeps the URL. It is never `unknown`.
4. **Given** hs-opentelemetry at the pinned commit, **When** scanned, **Then** no component is named `unknown`, and the snapshot is identified as `lts-18.8`.

---

### User Story 2 - A snapshot says where it came from and how it fits (Priority: P1)

A consumer can see which lock file named a snapshot, and either the snapshot is linked to its project or the document says why it isn't.

**Why this priority**: today the component has no source and no explanation, which is exactly what the corpus invariant I7 exists to catch.

**Independent Test**: in the fixture and on hs-opentelemetry, every snapshot component names its lock file, and is either reachable from the root or carries an orphan reason.

**Acceptance Scenarios**:

1. **Given** any snapshot component, **When** the document is read, **Then** its source files name the lock file that pinned it.
2. **Given** any snapshot component, **When** the document is read, **Then** it is either reachable from the document's root or carries an orphan reason, the same for URL-pinned and resolver-named snapshots.
3. **Given** the corpus, **When** the invariants run, **Then** `hpack-hs-opentelemetry` passes I7 and its I7 declaration is removed. No other declaration moves.

---

### Edge Cases

- **A stackage URL on another host or branch** (`…/stackage-snapshots/<ref>/lts/…`, a mirror). The plan decides whether the layout match depends on the host, and records it.
- **A URL with no hash recorded.** The version slot follows today's rule for a resolver-named snapshot with no hash.
- **Two lock files naming the same snapshot**, one by name and one by URL. They give the same identity, so the scan has one component that lists both lock files.
- **A malformed URL**, or one whose path matches the layout but has non-numeric parts, keeps the URL identity rather than being forced into a name.
- **The `completed` and `original` URLs differ.** The plan records which one names the snapshot.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-000** (added at plan time, maintainer decision; research R1): a `stack.yaml.lock` whose snapshot `original` is a plain string (`original: lts-24.35`, the form Stack writes for a resolver) MUST be read. Today it fails to deserialize and the whole file, packages included, is ignored. In the corpus, that's hs-opentelemetry's root lock and lootbox's.
- **FR-001**: A snapshot pinned by a URL in the stackage-snapshots layout MUST be named as its resolver would be:
  - `…/lts/<M>/<m>.yaml` → `lts-<M>.<m>`;
  - `…/nightly/<Y>/<M>/<D>.yaml` → `nightly-<Y>-<MM>-<DD>`, with zero-padding.
- **FR-002**: The same snapshot pinned by name and by URL MUST give the same component identity. The plan first measures what identity a resolver-named snapshot gets today, and the URL form adopts it.
- **FR-003**: A snapshot pinned by any other URL MUST get an identity that keeps the URL. The plan decides and records its form. No snapshot is ever named `unknown`.
- **FR-004**: Every snapshot component MUST list the lock file that named it in its source files. A snapshot named by several lock files lists them all.
- **FR-005**: Every snapshot component MUST be reachable from the document's root or carry an orphan reason. Which one is decided from how resolver-named snapshots are treated today, and URL-pinned snapshots MUST be treated the same.
- **FR-006**: Before the change, measure every snapshot component across the Haskell corpus targets on `main`:
  - `hpack-*`;
  - `haskell-*`;
  - `nix-closure-*` targets that use Stack.
  After the change, measure them again, and explain every difference.
- **FR-007**: A `waybill-fixture-*` positive control MUST cover an `lts` URL, a `nightly` URL, a non-stackage URL and a resolver name. Each naming form MUST have a test that a mutation of its rule fails.
- **FR-008**: Remove the I7 declaration for `hpack-hs-opentelemetry` once it passes. No other invariant declaration may change unless the measurement explains it.
- **FR-009**: Zero new Cargo dependencies. Corpus golden changes only where FR-006's measurement explains them. One gate, one PR.

### Key Entities

- **Snapshot component**: the identity derived from the resolver name or URL, the lock-file hash as its version, the lock files that named it, and its link to the project or its orphan reason.
- **Snapshot measurement**: target, snapshot identity, source files, and reachability or orphan reason, on `main` and on the branch.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Across the Haskell corpus targets, 0 snapshot components are named `unknown` (hs-opentelemetry has 1 today).
- **SC-002**: For each naming form in FR-007, a snapshot pinned by name and by URL gives one identity, shown by tests.
- **SC-003**: 100% of snapshot components name their lock file, and 100% are reachable or carry an orphan reason.
- **SC-004**: `hpack-hs-opentelemetry` passes I7 with its declaration removed. Unexplained corpus differences: 0.

## Assumptions

- The stackage-snapshots repository layout (`lts/<M>/<m>.yaml`, `nightly/<Y>/<M>/<D>.yaml`) is stable. The plan confirms it against the repository before relying on it.
- Whether nightly path parts are zero-padded in that repository is checked in the plan, not assumed.
- **Out of scope**: #1675, #1682, #1731.
