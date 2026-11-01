# Feature Specification: pnpm v9 development scope

**Feature Branch**: `1270-pnpm-v9-dev-scope`
**Created**: 2026-10-29
**Status**: Local implementation, not submitted
**Input**: Issue #1297, pnpm v9 portion only.

## User Scenarios & Testing

### User Story 1: Exclude development tooling (P1)

An operator scanning a pnpm v9 project can distinguish development-only dependencies and exclude them, including their transitive closure, while retaining dependencies needed at runtime.

Independent test: scan a lockfile produced by pnpm 9.15.9, before and after `--exclude-scope dev`, in all three formats.

Acceptance scenarios:
1. Runtime string-width and development wrap-ansi share string-width and strip-ansi. Exclusion removes wrap-ansi, ansi-styles, color-convert and color-name, retaining the five runtime packages.
2. A development declaration also reachable from any importer's runtime dependencies remains runtime.
3. Runtime evidence from an importer remains authoritative when its child manifest is absent from the scanned tree.
4. Optional evidence remains non-development when a child manifest is absent, including shared paths without an optional package marker.
5. Different locked versions of the same package are classified independently.

### Edge Cases

Multiple importers, npm aliases, peer-context suffixes, optional dependencies, absent importer evidence, and v6/v7 per-entry flags.

## Requirements

### Functional Requirements

- FR-001: Resolve development declarations by locked package identity, not name alone.
- FR-002: Runtime reachability across all importers takes precedence over a development declaration. Optional roots must not be demoted to development.
- FR-003: Preserve optional/peer classification and existing v6/v7 behavior.
- FR-004: Leave only v9 transitives without runtime or optional evidence eligible for existing propagation, rather than asserting unsupported runtime evidence.
- FR-005: Missing importer evidence preserves legacy classification; infer no development seeds.
- FR-006: Existing three-format scope carriers and filtering must express the same package set. No new output fields or dependencies.

### Key Entities

Importer: a project and its resolved direct declarations. Package identity: canonical name plus resolved version under existing peer normalization. Runtime closure: identities reachable from runtime/optional importer roots. Development seed: direct development identity outside that closure.

## Success Criteria

- SC-001: The real generated fixture changes from nine to five dependencies when development scope is excluded, in each supported format.
- SC-002: Every runtime/shared dependency remains, including a direct dev declaration reached transitively at runtime.
- SC-003: Legacy flags, missing importers, aliases, peer contexts and multiple importers have explicit regression controls.
- SC-004: Mandatory pre-PR gate succeeds or the exact blocker is retained without claiming readiness.

## Assumptions and Scope

Lockfile-wide scope is the union across its importers, matching existing lockfile-wide emission. Existing name/version identity merges peer contexts; runtime reachability wins across merged contexts. Yarn and Hpack are excluded. Local work is authorized; maintainers have not approved direction and nothing is published.
