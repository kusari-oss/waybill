# Feature Specification: Project membership for `package.yaml`

**Feature Branch**: `1662-hpack-project-membership`
**Created**: 2026-11-26
**Status**: Draft
**Input**: #1685. Refs #1685.

## Context

A `cabal.project` that declares `packages:` is the project's own statement of which packages belong to it. Since #1032, waybill drops `.cabal` files that a governing `cabal.project` doesn't declare. On haskell-language-server that removed 69 of 75 `.cabal` files, which were test fixtures and a CI helper. They had produced 33 coordinates that exist on no registry, and 4 real package names carrying a fixture's version.

The hpack reader (`package.yaml`) never applies that rule.

**Measured** (#1685), on cardanosolutions/ogmios at `435189aea2d9a047dc395a6f1cdef737d52f1cbe`:
- `server/cabal.project` lists `./` and eight `modules/*` directories. It doesn't list `modules/json-via-show`.
- The repository has no stack file, so `cabal.project` is the only build definition, and `json-via-show` isn't built.
- `server/modules/json-via-show/` holds both a `package.yaml` and its hpack-generated `json-via-show.cabal`.
- Before spec 1649, waybill emitted nothing for it: the manifest wasn't read, and #1032 dropped the `.cabal`.
- Since spec 1649, waybill emits `pkg:hackage/json-via-show@2.0.0`.

**How #1032's rule works today:**
- A `packages:` (or `optional-packages:`) entry names one of three things:
  - a directory, which claims the `.cabal` files directly inside it, not recursively;
  - a directory glob, which uses the same rule;
  - a `.cabal` path or glob.
- `.`, `./` or an empty entry names the project's own directory.
- Globs never cross `/`.
- Each `cabal.project` governs only the tree it sits in.
- A `cabal.project` with no `packages:` field governs nothing.
- When none of a `cabal.project`'s entries match, the tree is left alone, because that's more likely a layout the parser doesn't understand than a project that declares nothing.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - An undeclared hpack package isn't reported (Priority: P1)

A consumer scans a Haskell repository whose `cabal.project` leaves out a directory holding a `package.yaml`. The SBOM doesn't list that package, just as it wouldn't if the directory held only a `.cabal`.

**Why this priority**: it's a false component today, reachable on the corpus (ogmios). A false component is worse than a missing one: a consumer reads it as shipped.

**Independent Test**: a `waybill-fixture-*` project whose `cabal.project` lists one `package.yaml` directory and omits another. The listed package is emitted and the omitted one isn't.

**Acceptance Scenarios**:

1. **Given** ogmios at the pinned commit, **When** it's scanned, **Then** no `json-via-show` component is emitted, and every package `cabal.project` declares still is.
2. **Given** the positive-control fixture, **When** it's scanned, **Then** the declared `package.yaml` package is emitted and the undeclared one isn't.
3. **Given** a directory holding only a `package.yaml`, **When** a `cabal.project` entry names it in any form, **Then** the hpack and `.cabal` readers reach the same decision they would for a `.cabal` in that directory.

---

### User Story 2 - Nothing else changes (Priority: P1)

A consumer scanning an hpack project with no `cabal.project`, or one governed by a stack file, gets the same SBOM as before.

**Why this priority**: #1032's own rule is that a project with nothing to filter against must behave exactly as before. A membership filter that over-reaches loses real packages, which is the opposite defect.

**Independent Test**: the hpack corpus targets (#1677) and haskell-language-server, scanned before and after, with every difference in their Haskell components explained.

**Acceptance Scenarios**:

1. **Given** a `package.yaml` with no governing `cabal.project`, **When** it's scanned, **Then** its component is unchanged.
2. **Given** a `package.yaml` governed only by a stack file, **When** it's scanned, **Then** its component is unchanged.
3. **Given** a directory holding a `package.yaml` and its generated `.cabal`, **When** it's declared, **Then** at most one component is emitted for it, as today.
4. **Given** the measured targets, **When** before and after are compared, **Then** every added or removed Haskell component is listed with its reason, and none is unexplained.

---

### Edge Cases

- **An entry naming a `.cabal` that exists only as hpack's future output** (for example `modules/foo/foo.cabal` beside a lone `package.yaml`). The spec must record whether this matches. hpack names the generated file after the manifest's `name:` field, not the directory.
- **The "nothing matched, leave the tree alone" fallback.** Today it counts only `.cabal` files. Once `package.yaml` directories count as candidates, a project whose entries match only hpack packages must no longer fall back. A project whose entries match nothing must still fall back.
- **Nested `cabal.project` files.** Each governs its own tree, as for `.cabal`.
- **`optional-packages:`** declares membership, as for `.cabal`.
- **A `package.yaml` nested inside a declared package's test data** isn't claimed, because directory entries don't recurse.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: A `package.yaml` in a tree governed by a `cabal.project` that declares `packages:` MUST be emitted only if that `cabal.project` declares its directory. The decision MUST come from the same membership logic that #1032 applies to `.cabal` files, not a second implementation.
- **FR-002**: For each entry form, the spec's plan MUST record how a directory holding only a `package.yaml` is matched:
  - a directory;
  - a directory glob;
  - `.`, `./` or empty;
  - a `.cabal` path or glob, including one naming a `.cabal` that hpack would generate.
  The hpack and `.cabal` readers MUST reach the same decision for the same directory under every form.
- **FR-003**: A directory holding both a `package.yaml` and its generated `.cabal` MUST yield at most one component, as today.
- **FR-004**: A `package.yaml` with no governing `cabal.project`, or whose governing `cabal.project` declares no `packages:`, or which is governed only by a stack file, MUST be unaffected.
- **FR-004a** (added at implementation, maintainer decision): a package is a member if the governing `cabal.project` OR a `stack.yaml` in the same directory declares it, for both readers. Measured: hs-opentelemetry's `examples/otlp-demo` is in `stack.yaml` but commented out of `cabal.project`, and a `cabal.project`-only rule dropped it. A package built only by Nix is still not a member (see `measurements/membership.md`).
- **FR-005**: #1032's fallback MUST still hold: when none of a `cabal.project`'s entries match any candidate, `.cabal` or `package.yaml`, the tree is left alone.
- **FR-006**: Before the change is made, measure each target's Haskell component set on `main`:
  - ogmios at the pinned commit;
  - the hpack corpus targets (#1677);
  - haskell-language-server.
  After the change, measure them again, and explain every difference.
- **FR-007**: A `waybill-fixture-*` positive control MUST show a declared `package.yaml` emitted and an undeclared one dropped. Each entry form in FR-002 MUST have a test that a mutation of the matching rule makes fail.
- **FR-008**: Zero new Cargo dependencies. Corpus golden changes only where FR-006's measurement explains them. One gate, one PR.

### Key Entities

- **Package directory**: a directory holding a `.cabal`, a `package.yaml`, or both. It's the unit a `packages:` entry declares.
- **Governing `cabal.project`**: a `cabal.project` that declares `packages:`, governing the tree it sits in.
- **Membership measurement**: target, commit, waybill revision, and the Haskell component set before and after, with a reason for each difference.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: ogmios at the pinned commit emits 0 `json-via-show` components, down from 1, and keeps every component its `cabal.project` declares.
- **SC-002**: Across the measured targets, every difference in Haskell components before and after is listed with a reason. Unexplained differences: 0.
- **SC-003**: For every entry form in FR-002, a directory holding only a `package.yaml` and a directory holding only a `.cabal` get the same verdict, shown by tests.
- **SC-004**: Projects with no governing `cabal.project`, or governed only by a stack file, produce unchanged output.

## Assumptions

- hpack names its generated `.cabal` after the manifest's `name:` field. The plan confirms this against hpack's behaviour before relying on it.
- The corpus target list for #1677 is the one recorded there. If a target can't be fetched, the measurement says so rather than dropping it silently.
- **Out of scope**: #1675, #1678, #1682.
