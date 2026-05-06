# Feature Specification: Operator-overridable root component name and version

**Feature Branch**: `077-root-component-override`
**Created**: 2026-05-06
**Status**: Draft
**Input**: User description: "Add `--root-name <NAME>` and `--root-version <VERSION>` CLI flags so operators can override the auto-derived `metadata.component.name` / `metadata.component.version` of an emitted SBOM. Today, source-tier scans of arbitrary directories produce names like `filesystem-scan@0.0.0` (basename of `--path` + hardcoded `0.0.0`) that don't reflect the operator-meaningful project identity. Derived fields (`bom-ref`, `purl`, `cpe`) MUST automatically follow the overridden name/version."

## Overview

Today, mikebom's emitted SBOMs carry a root component (`metadata.component` in CDX, equivalent main-module package in SPDX 2.3, root element in SPDX 3) whose `name` and `version` are derived from the scan target. For source-tier scans (`mikebom sbom scan --path`), the name comes from `path.file_name()` (the basename of the supplied directory) — falling back to the literal string `"filesystem-scan"` when no parsable basename exists — and the version is hardcoded `"0.0.0"` when no manifest-driven main-module derivation is available. For image-tier scans, the name comes from the image reference. For build-tier scans (`mikebom trace run`), it comes from the wrapped command's working directory.

These defaults work for self-contained projects where the basename happens to match the operator-meaningful identity, or where a manifest-driven main-module milestone (Cargo, npm, pip, gem, Maven, Go) populates name+version from the project manifest. They break for the case the user surfaced: an operator running mikebom on an arbitrary directory or generic asset where the operator already knows the canonical project identity but the directory layout doesn't encode it. The current emitted SBOM contains:

```json
{
  "bom-ref": "filesystem-scan@0.0.0",
  "name": "filesystem-scan",
  "version": "0.0.0",
  "purl": "pkg:generic/filesystem-scan@0.0.0",
  "cpe": "cpe:2.3:a:mikebom:filesystem-scan:0.0.0:*:*:*:*:*:*:*"
}
```

The operator wants:

```json
{
  "bom-ref": "widget-svc@1.2.3",
  "name": "widget-svc",
  "version": "1.2.3",
  "purl": "pkg:generic/widget-svc@1.2.3",
  "cpe": "cpe:2.3:a:mikebom:widget-svc:1.2.3:*:*:*:*:*:*:*"
}
```

This milestone adds two new CLI flags that close the gap:

- `--root-name <NAME>` overrides the auto-derived `metadata.component.name`.
- `--root-version <VERSION>` overrides the auto-derived `metadata.component.version`.

Derived fields (`bom-ref`, `purl`, `cpe`) automatically follow the overridden name/version through the existing per-format derivation pipeline. Both flags are independent — operators can override one without the other. When neither is passed, behavior is byte-identical to alpha.17.

The scope is deliberately narrow: only the **root component**'s name/version. Vendor portion of the CPE (currently hardcoded `mikebom`), `metadata.component.type`, and per-component name/version overrides are out of scope. They can be addressed in follow-up milestones if operator demand emerges.

## Clarifications

### Session 2026-05-06

- Q: How strict should `--root-name` validation be? → A: Permissive — accept any non-empty UTF-8 except whitespace, control characters, `?`, and `#` (the URL-syntax-breaking subset). URL-encode the rest at PURL emission time. Allows npm-scoped names like `@acme/widget-svc`.
- Q: When `--root-name`/`--root-version` overrides a manifest-driven main-module derivation (Cargo `[package]`, npm `package.json`, etc.), what happens to the manifest-derived component? → A: Dropped entirely. The override is a clean replacement: the root's name/version/bom-ref/purl/cpe all derive from operator input; the manifest-derived `foo-internal@0.5.1`-style component is NOT preserved anywhere in the emitted SBOM. The "demote to `components[]` as a library entry" alternative is deferred to a follow-up GitHub issue for exploration based on operator demand.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Override root name and version on a generic source-tier scan (Priority: P1)

An operator runs `mikebom sbom scan --path /opt/builds/my-asset --root-name widget-svc --root-version 1.2.3 --output out.cdx.json` against an arbitrary directory whose basename doesn't reflect the operator-meaningful identity. The emitted SBOM's root component carries `name=widget-svc`, `version=1.2.3`, with `bom-ref`, `purl`, and `cpe` derived from those values automatically.

**Why this priority**: This is the headline use case the user surfaced. Source-tier scans of generic / non-manifest-driven directories are the most common path that produces operator-unfriendly default identities. Without this flag, operators must post-process emitted SBOMs with `mikebom sbom enrich` (JSON Patch) — workable but awkward, and easy to miss derived fields like `purl` and `cpe`. P1 because it directly closes a UX gap operators hit immediately on first use.

**Independent Test**: Run `mikebom sbom scan --path /tmp/scratch --root-name widget-svc --root-version 1.2.3 --output out.cdx.json` against an empty tempdir. Verify the emitted CDX `metadata.component` has the operator-supplied name/version and the derived `bom-ref`/`purl`/`cpe` reflect them.

**Acceptance Scenarios**:

1. **Given** an arbitrary directory `/opt/builds/abc123-snapshot` (a non-meaningful basename), **When** the operator runs `mikebom sbom scan --path /opt/builds/abc123-snapshot --root-name widget-svc --root-version 1.2.3 --output out.cdx.json`, **Then** the emitted SBOM's `metadata.component.name` is `widget-svc`, `metadata.component.version` is `1.2.3`, `metadata.component.bom-ref` is `widget-svc@1.2.3`, `metadata.component.purl` is `pkg:generic/widget-svc@1.2.3`, and `metadata.component.cpe` is `cpe:2.3:a:mikebom:widget-svc:1.2.3:*:*:*:*:*:*:*`.
2. **Given** the same scan invocation with only `--root-name widget-svc` (no `--root-version`), **When** the SBOM is emitted, **Then** the name override applies but the version stays at the auto-derived default (`0.0.0` for arbitrary directories). `purl`/`bom-ref` use `widget-svc@0.0.0`.
3. **Given** the same scan invocation with only `--root-version 1.2.3` (no `--root-name`), **When** the SBOM is emitted, **Then** the version override applies but the name stays at the auto-derived basename. `purl`/`bom-ref` use `<basename>@1.2.3`.
4. **Given** an operator runs `mikebom sbom scan --path .` with neither flag, **When** the SBOM is emitted, **Then** the output is byte-identical to alpha.17 (no regression on default-flag invocations).

---

### User Story 2 — Override on image-tier and build-tier scans (Priority: P2)

The same flags work on `mikebom sbom scan --image` and `mikebom trace run` to override the image-derived or build-derived root component identity.

**Why this priority**: Lower-priority because image-tier scans typically derive a sensible name from the image reference (`acme/widget-svc:v1` → `acme/widget-svc`), and build-tier scans derive from manifest-driven main-module milestones for the supported ecosystems. The override flag is an escape hatch for operators with non-standard workflows or who want consistency across tiers (the same `--root-name widget-svc --root-version 1.2.3` applied to all three tier scans produces three SBOMs that share the same root identity, which simplifies cross-tier consumption). P2 because the absence of this on image/build tiers is acceptable for MVP — operators can post-process via `mikebom sbom enrich` if needed for non-source tiers.

**Independent Test**: Run `mikebom sbom scan --image alpine:3.19 --root-name widget-svc-base --root-version 1.0.0 --output out.cdx.json`. Verify the emitted SBOM's root component has the override values, not the alpine-derived defaults.

**Acceptance Scenarios**:

1. **Given** an operator runs `mikebom sbom scan --image alpine:3.19 --root-name widget-svc-base --root-version 1.0.0`, **When** the SBOM is emitted, **Then** the root component name and version reflect the override. The image's auto-detected `image:` identifier (milestone 073) is unaffected — it's a separate slot in `externalReferences[]`.
2. **Given** an operator runs `mikebom trace run --root-name widget-svc --root-version 1.2.3 -- ./build.sh`, **When** the build SBOM is emitted, **Then** the root component name and version reflect the override. Auto-detected `repo:`/`git:`/`subject:` identifiers (milestones 073/074/076) are unaffected.

---

### User Story 3 — Override interacts cleanly with manifest-driven main-module milestones (Priority: P2)

When mikebom scans a project with a recognized manifest (Cargo.toml, package.json, etc.), the existing main-module milestones (064, 066, 068, 069, 070) populate name+version from the manifest. The new `--root-name`/`--root-version` flags MUST override these manifest-derived values when both are present (explicit operator input wins over derived input).

**Why this priority**: Real-world case where a Cargo project's `[package].name = "foo-internal"` should be reported as `widget-svc` in the SBOM for downstream consumption. Operators have legitimate reasons to override manifest values (release-name vs internal-name divergence, multi-component repositories where one Cargo workspace member is the publishable artifact, etc.). P2 because the manifest-derived defaults work correctly for ~90% of operators; the override is a bridge for edge cases.

**Independent Test**: Run `mikebom sbom scan --path <a-cargo-fixture> --root-name override-name --root-version override-version --output out.cdx.json`. Verify the emitted SBOM's root component has the override values, NOT the Cargo.toml `[package].name`/`version` values.

**Acceptance Scenarios**:

1. **Given** a Cargo project with `[package].name = "foo-internal"` and `[package].version = "0.5.1"`, **When** the operator runs `mikebom sbom scan --path . --root-name widget-svc --root-version 1.2.3`, **Then** the emitted SBOM's `metadata.component.name` is `widget-svc` and `version` is `1.2.3` (override wins over manifest).
2. **Given** the same Cargo project, **When** the operator runs `mikebom sbom scan --path .` with neither flag, **Then** the emitted SBOM uses the manifest-derived `foo-internal` / `0.5.1` (no regression for the no-flag case).

---

### Edge Cases

- **Empty value** (`--root-name ""` or `--root-version ""`): rejected at CLI parse time with a clear error. Names and versions must be non-empty.
- **PURL-unsafe characters in name** (spaces, slashes, `?`, `#`, etc.): rejected at parse time with a clear error pointing at the PURL spec's allowed character set. Operators wanting unusual names can URL-encode the value themselves before passing.
- **Both flags passed identical to current auto-derived defaults**: the override applies anyway (no special-case "no-op" detection); behavior is byte-identical to the no-flag case for the same inputs.
- **One flag passed, the other absent**: the supplied flag overrides; the absent one falls through to the auto-derived default. Per US1 acceptance scenarios 2 and 3.
- **Override on a scan that produces no root component at all** (theoretical — currently every scan produces one): non-applicable; mikebom's emit pipeline always produces a root component.
- **Override interacts with `--component-id` (milestone 076)**: orthogonal — `--component-id` matches against `components[].purl`, not against the root component's `purl`. The override changes only `metadata.component.purl`, which is not a `--component-id` selector target by design (per FR-007 of milestone 076, `--component-id` rejects built-in scheme names; the root-component `purl` is in `components[]` semantics, but mikebom's pipeline keeps the root component out of the matching pool).
- **Override drops the manifest-derived main module** (Cargo / npm / pip / gem / Maven / Go projects): per the 2026-05-06 clarification, the operator's override is a clean replacement. The manifest's main-module identity (e.g., `pkg:cargo/foo-internal@0.5.1`) is NOT preserved in the emitted SBOM. Operators who want the manifest-derived component preserved as a regular library entry alongside the operator-supplied root identity should track the open GitHub issue for the demote-to-library follow-up feature.
- **Override interacts with cross-tier identifiers**: orthogonal — `--root-name`/`--root-version` change the root component's display identity; `repo:`/`git:`/`image:`/`subject:` identifiers (in `externalReferences[]`) and per-component identifiers (`components[].properties[]`) ride separately.
- **Override interacts with `--bind-to-source`** (milestone 072): orthogonal — the binding annotation references another SBOM's content hash, not the root component identity.
- **Operator passes special PURL-namespaced name** (e.g., `--root-name "@scope/pkg"` for npm-style scoped names): allowed if the characters are PURL-safe per the above rule. The PURL emitter URL-encodes namespace separators per the PURL spec.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Add a new CLI flag `--root-name <NAME>` to `mikebom sbom scan` and `mikebom trace run`. When set, the value MUST override the auto-derived `metadata.component.name` in the emitted SBOM (CDX), the equivalent main-module `Package.name` in SPDX 2.3, and the root element name in SPDX 3.
- **FR-002**: Add a new CLI flag `--root-version <VERSION>` to the same subcommands. When set, the value MUST override the auto-derived version in CDX `metadata.component.version`, SPDX 2.3 main-module `Package.versionInfo`, and SPDX 3 root element version.
- **FR-003**: Both flags MUST be independent — passing one without the other is supported. The unspecified field falls through to the auto-derived default.
- **FR-004**: Derived fields (`bom-ref`, `purl`, `cpe`, and any equivalent SPDX 2.3 / SPDX 3 derived identifiers like SPDXID) MUST automatically follow the overridden name/version through the existing per-format derivation pipeline. Specifically: `bom-ref` becomes `<NAME>@<VERSION>`, `purl` becomes `pkg:generic/<NAME>@<VERSION>` (when no manifest-driven PURL is in play), `cpe` becomes `cpe:2.3:a:mikebom:<NAME>:<VERSION>:*:*:*:*:*:*:*`.
- **FR-005**: Empty values (`--root-name ""` or `--root-version ""`) MUST be rejected at CLI parse time with a clear, actionable error message.
- **FR-006**: `--root-name` values MUST be rejected at CLI parse time when they contain ANY of: ASCII whitespace (`\s`), ASCII control characters (`\x00`–`\x1F`, `\x7F`), `?`, or `#`. All other UTF-8 characters are accepted (per the 2026-05-06 clarification — permissive). The PURL emission pipeline MUST URL-encode the value into the `name` segment per RFC 3986; specifically, characters like `@`, `/`, and other URL-reserved characters are encoded automatically rather than rejected. `--root-version` follows the same rule (rejects whitespace / control / `?` / `#`; URL-encodes the rest at emission).
- **FR-007**: Override MUST apply consistently across all three SBOM formats (CDX, SPDX 2.3, SPDX 3) when emitted simultaneously. Operators emitting all three formats from one scan get root components with byte-identical name+version across the trio.
- **FR-008**: Override MUST take precedence over manifest-driven main-module derivation (Cargo, npm, pip, gem, Maven, Go from milestones 064–070) when both are present. Explicit operator input wins. Per the 2026-05-06 clarification, the override is a **clean replacement**: the manifest-derived main-module's identity (its `pkg:cargo/...` / `pkg:npm/...` / etc. PURL) is NOT preserved in the emitted SBOM — neither at `metadata.component` nor as a demoted entry in `components[]`. (The "demote to library" alternative is tracked as a separate follow-up GitHub issue.)
- **FR-009**: Cross-format byte-identity goldens for fixtures with neither flag set MUST stay byte-identical to alpha.17 (no regression). The existing fixture set has no operator-supplied `--root-name` / `--root-version`, so the regen is empirically a no-op.
- **FR-010**: Override MUST be deterministic — same flag values + same scan target → byte-identical emitted SBOMs across re-runs.
- **FR-011**: Override is orthogonal to all milestone-072–076 identifier and binding flags. `--root-name` / `--root-version` change the root component's display identity; identifiers (`--repo`, `--git-ref`, `--image-id`, `--attestation`, `--id`, `--subject-hash`), per-component identifiers (`--component-id`), and cross-tier binding (`--bind-to-source`) ride independently and are unaffected by the root-component override.

### Key Entities

- **RootComponentOverride**: Two optional operator-supplied values (`name: Option<String>`, `version: Option<String>`) flowing from CLI flags through the scan pipeline to per-format emitters. When `Some(_)`, the value replaces the corresponding auto-derived field in `metadata.component` (CDX) / main-module Package (SPDX 2.3) / root element (SPDX 3). When `None`, the existing auto-derivation flow runs unchanged.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: An operator running `mikebom sbom scan --path /tmp/scratch --root-name widget-svc --root-version 1.2.3 --output out.cdx.json` against an empty tempdir produces an SBOM whose `metadata.component.name == "widget-svc"`, `version == "1.2.3"`, `bom-ref == "widget-svc@1.2.3"`, `purl == "pkg:generic/widget-svc@1.2.3"`, `cpe == "cpe:2.3:a:mikebom:widget-svc:1.2.3:*:*:*:*:*:*:*"`. Verified by integration test.
- **SC-002**: The same scan without flags produces an SBOM byte-identical to alpha.17 (no regression). Verified by golden equivalence on every existing source-tier fixture.
- **SC-003**: `--root-name widget-svc` alone (no `--root-version`) emits an SBOM with `name == "widget-svc"` and `version == "0.0.0"` (the existing auto-derived default for arbitrary directories). Verified by integration test.
- **SC-004**: `--root-version 1.2.3` alone (no `--root-name`) emits an SBOM with `name == <basename>` and `version == "1.2.3"`. Verified by integration test.
- **SC-005**: All three SBOM formats (CDX, SPDX 2.3, SPDX 3) emitted from one scan invocation with `--root-name widget-svc --root-version 1.2.3` carry byte-identical name+version in their respective root component fields. Verified by integration test asserting per-format.
- **SC-006**: A Cargo project with `[package].name = "foo-internal"` scanned with `--root-name widget-svc` produces an SBOM whose `metadata.component.name == "widget-svc"` AND whose `components[]` array contains zero entries with PURL `pkg:cargo/foo-internal@0.5.1` (override is a clean replacement; the manifest-derived main-module identity does not appear anywhere in the emitted SBOM). Verified by integration test asserting both the override-applied root and the absence of the manifest-derived entry.
- **SC-007**: `--root-name ""` and `--root-version ""` fail at CLI parse with a clear, non-zero exit and a human-readable error message. Verified by negative integration test.
- **SC-008**: `--root-name "my widget svc"` (whitespace) fails at CLI parse with a clear error message identifying the offending character and the rule it violates. Same outcome for control characters, `?`, and `#`. `--root-name "@acme/widget-svc"` (npm-scoped) succeeds and emits URL-encoded into the PURL (`pkg:generic/%40acme/widget-svc@1.2.3` or per the PURL emitter's canonical encoding). Both verified by integration tests.
- **SC-009**: Re-running an identical `mikebom sbom scan` invocation with the same `--root-name` / `--root-version` against the same scan target produces byte-identical SBOMs across the two runs. Verified by determinism integration test.
- **SC-010**: Existing milestone-073/074/075/076 byte-identity goldens stay byte-identical after milestone 077 ships, since no fixture passes the new flags. Verified by the existing parity-check golden suite continuing to pass unchanged.

## Assumptions

- The validator is permissive per the 2026-05-06 clarification: accepts any non-empty UTF-8 except whitespace, control characters, `?`, and `#`. The PURL emitter URL-encodes per RFC 3986 at emission time. This permits ecosystem-style names (`@acme/widget-svc`, `org.acme:widget-svc`) and most operator-supplied identifiers without forcing pre-encoding. Tightening (toward a stricter validator) is reserved for a future hardening milestone if downstream tools surface concrete compatibility issues — loosening from a strict rule would be a breaking change, so the safer default is permissive-with-room-to-tighten.
- The `--root-version` validator is permissive — anything non-empty + non-control-character. The PURL spec doesn't enforce a strict version syntax (semver, calver, etc. are all valid). Operators choose their own scheme.
- This milestone scopes only `--root-name` and `--root-version`. The CPE vendor portion (currently hardcoded `mikebom`) is NOT operator-overridable in this milestone. A future `--root-vendor` flag is reasonable if multi-vendor CPE namespacing demand emerges.
- This milestone scopes only the **root component** (`metadata.component`). Per-component name/version overrides for other components in `components[]` are out of scope (operators wanting that should use `mikebom sbom enrich` post-processing or wait for a future per-component-edit milestone).
- The flags are accepted on `mikebom sbom scan` (both `--path` and `--image` variants) and `mikebom trace run`. They are NOT accepted on enrichment / verification / parity subcommands (`mikebom sbom enrich`, `mikebom sbom verify`, `mikebom sbom verify-binding`, `mikebom sbom trace-binding`, `mikebom parity-check`) — those don't construct root components.
- When `--root-name` is set but the scan target is a manifest-driven project (Cargo, npm, etc.), the override is a **clean replacement** per the 2026-05-06 clarification: the root component's full identity (name, version, bom-ref, purl, cpe) derives from operator input, and the manifest-derived main-module identity is NOT preserved in the emitted SBOM (it doesn't appear in `metadata.component`, doesn't appear in `components[]` as a demoted library entry). Operators who want the manifest-derived information preserved alongside the override can use `mikebom sbom enrich` to add it back as a custom component, or wait for the future demote-to-library follow-up tracked in the project's GitHub issues.
- Operators concerned about override side-effects on downstream tooling (e.g., a vulnerability scanner that hard-codes name patterns) can preview the change by emitting once with the flag and once without and `diff`ing the outputs.
- Backward compatibility: existing CLI surface unchanged for operators not using the new flags. The flags are additive, default-absent. No breaking change to any milestone-073/074/075/076 contract.
