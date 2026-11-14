# Feature Specification: `fragment` requires evidence of a live-bootstrap function

**Feature Branch**: `1503-fragment-positive-evidence`
**Created**: 2026-11-14
**Status**: Draft
**Input**: User description: "#1511 — C206 `fragment` is the classifier's fall-through, not a positive finding: it takes every pname-less member with no content hash, no `patches` listing, no `env.text` and no version token. Measured on the pinned moat closure via `waybill debug closure-dump`: 215 of 240 fragments are built by live-bootstrap's `kaem` (strncpy, libc.a, mes), and 25 are stdenv-built (all carry a `stdenv` attribute; bash builder): setup-hook.sh, add-flags.sh, utils.bash, python-setup-hook.sh, expand-response-params, nuke-refs, remove-references-to, uname, mirrors-list, bootstrap-stage0-glibc-iconv-minimal-bootstrap. On nix-closure-openai and -slack-web (an older bootstrap with no kaem) all 67/66 fragments are hooks, stdenv stages, bootstrap-tools, utilities and `stdenv-linux` itself; walking through `stdenv-linux` fans each consumer out to the whole toolchain (~38 edges per patch vs 2–4 on moat, specs/1492-patch-member-source/measurements/golden-review.md). Make `fragment` require positive evidence of a live-bootstrap function, decide the role of the members that lose it, and measure the edge delta on all five closure goldens with the s1367 accounting. The openai and slack-web closures are not captured; capture them in CI (as for moat, public-corpus run 43535531153), never evaluate them locally. Do not add a sixth C206 value without saying so. Keep the docs' claim "fragment is now said only of live-bootstrap functions" true."

## Background

C206 `waybill:closure-member-role` says what a closure member that declares no `pname` **is**. Each member gets one of five frozen values:
- `source`: something a package is built from;
- `package`: documented as "a version-bearing bootstrap package";
- `check`: a version check or test program;
- `scaffolding`: a build script (`writeText` output);
- `fragment`: documented as "a live-bootstrap C function", such as `strncpy`.

The dependency-graph projection **stops** at `source`, `package` and `check` members and **walks through** `scaffolding` and `fragment` ones. Consumers are told to filter on the role.

The classifier's evidence order is: content hash → `source`; listed in a `patches` attribute → `source` (spec 1492); `env.text` → `scaffolding`; version-shaped name → `source`, `check` or `package` (spec 1380); **else `fragment`**. Nothing in that last rung asks whether the member is a live-bootstrap function. It takes everything the earlier rungs did not. Since spec 1492 the C206 catalog row says "`fragment` is now said only of live-bootstrap functions". The goldens show that claim is false (#1511).

**Measured** on the pinned x86_64-linux `nix-closure-moat` closure (1,749 derivations; captured in public-corpus run 43535531153; closure sha256 `898fcdd3…44ab`). Roles were read with `waybill debug closure-dump` built from `main` at ac2bd28 (after #1510). Derivation attributes come from the captured closure document.
- **240 `fragment` members.**
- **215** have the live-bootstrap shell `kaem` as their builder and **no** `stdenv` attribute (`strncpy`, `libc.a`, `mes`, …).
- **25** are built by bash through stdenv, and **every one** carries a `stdenv` attribute:
  - **10 hook and helper scripts:** `setup-hook.sh` ×3, `python-setup-hook.sh` ×3, `add-flags.sh` ×2 and `utils.bash` ×2;
  - **13 programs that declare a main program** (`NIX_MAIN_PROGRAM`): `expand-response-params` ×6, `nuke-refs` ×5, `remove-references-to` and `uname`;
  - **1 data file:** `mirrors-list`;
  - **1 bootstrap stage:** `bootstrap-stage0-glibc-iconv-minimal-bootstrap`.

**Observed from the committed goldens.** These closures were not captured, so their derivation attributes have not been read.
- **`nix-closure-openai` (67 `fragment`) and `nix-closure-slack-web` (66)** pin an older bootstrap with no `kaem`. By name, none of their fragments is a live-bootstrap function. They are setup hooks, the `bootstrap-stage*-stdenv-linux` stages, `bootstrap-tools`, utilities, and `stdenv-linux` ×2 itself. Walking through `stdenv-linux` reaches the whole toolchain. Each patch made `source` by spec 1492 gained about 38 edges on these two closures, against 2–4 on moat (`specs/1492-patch-member-source/measurements/golden-review.md`).
- **`nix-closure-shellcheck` (241) and `nix-closure-slacklinker` (250)** show by name the same 25-member pattern as moat. slacklinker adds `cargo-deps-vendor`, `fetch-cargo-vendor-util`, `pythoncheck.sh` ×4, `builder.pl` and `replace-workspace-values`.

## Clarifications

### Session 2026-11-14

- Q: Which role do members that lose `fragment` get? → A: A split by evidence. Hook and helper scripts → `scaffolding`; programs, data, bootstrap stages and `stdenv-linux` → `package` (FR-004).
- Q: On openai and slack-web, 37 and 36 pname-less fragments are `makeSetupHook` outputs (setup-hook packages, whose `buildCommand` writes `nix-support/setup-hook`). On moat the same-named hooks declare a `pname` and are package endpoints. Which role? → A: `package`, on that positive evidence, whatever the name. Only single-file scripts are `scaffolding` (FR-004).
- Q: Does the script rung include `.pl` (slacklinker's `builder.pl`)? → A: Yes. The extensions are `.sh`, `.bash` and `.pl`, which are all the script extensions observed on the five closures.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - `fragment` means a live-bootstrap function (Priority: P1)

A reviewer filters a Nix closure SBOM for `fragment` to audit the live-bootstrap chain. Today on moat, 25 of the 240 are setup hooks and build utilities. On openai and slack-web, all of them are. After this change every `fragment` member is one the live-bootstrap toolchain built, and a closure without that toolchain has none.

**Why this priority**: The role is a statement of fact in the document, and the catalog now asserts a definition the output contradicts. A consumer told "these are C functions, not dependencies" drops `stdenv-linux` and every setup hook from view.

**Independent Test**: Scan a closure that holds a `kaem`-built pname-less member and a stdenv-built pname-less member with no other evidence. The first is `fragment` and the second is not.

**Acceptance Scenarios**:

1. **Given** a pname-less member with no earlier evidence that the live-bootstrap toolchain built, **When** the closure is scanned, **Then** its C206 value is `fragment` in all three formats.
2. **Given** a pname-less member with no earlier evidence that stdenv built, **When** the closure is scanned, **Then** its C206 value is not `fragment`.
3. **Given** the pinned moat closure, **When** it is scanned, **Then** exactly the 25 measured members leave `fragment`, the 215 `kaem`-built members keep it, and no other member's C206 value changes.
4. **Given** the openai and slack-web closures, **When** they are scanned, **Then** no member carries `fragment` unless its captured derivation shows live-bootstrap evidence.

---

### User Story 2 - The members that lose `fragment` get a role their evidence supports (Priority: P1)

A reviewer reading `stdenv-linux`, `nuke-refs` or `setup-hook.sh` sees a role that describes what it is, and the graph treats it accordingly.

**Why this priority**: Moving a member out of `fragment` without an evidenced destination swaps one false label for another. The destination also decides whether the projection stops there, which is what fixes the `stdenv-linux` fan-out.

**Independent Test**: For each kind of member that leaves `fragment` (hook script, declared program, data file, stdenv stage), a fixture member of that kind gets the role FR-004 assigns. A member of the same kind with earlier evidence keeps that earlier role.

**Acceptance Scenarios**:

1. **Given** the 10 hook and helper scripts that leave `fragment` on the pinned moat closure, **When** it is scanned, **Then** each is `scaffolding`.
2. **Given** the other 15 members that leave `fragment` on moat (13 programs, `mirrors-list` and the bootstrap stage), **When** it is scanned, **Then** each is `package`.
3. **Given** `stdenv-linux` on the openai closure, **When** it is scanned, **Then** it is `package`, and the projection stops there.
4. **Given** a stdenv-built member with neither live-bootstrap nor script evidence, **When** it is scanned, **Then** it is `package`.

---

### User Story 3 - The edge change is measured and reviewed on every closure target (Priority: P1)

A reviewer can see, for each of the five closure targets, how many dependency edges the reclassification added and removed and why.

**Why this priority**: The role decides whether the projection walks through a member. Any role change into or out of an endpoint role moves edges, and on openai and slack-web it may move thousands. Spec 1492 established that such a change is measured before it ships, not discovered in a golden diff.

**Independent Test**: The #1367 edge accounting run before and after on each captured closure attributes every changed edge to a reclassified member.

**Acceptance Scenarios**:

1. **Given** each of the five closure targets, captured in CI, **When** the edge accounting is run before and after the change, **Then** every added or removed edge is attributed to a reclassified member.
2. **Given** the regenerated goldens, **When** they are compared with the previous ones, **Then** every changed component and edge is explained by the measurement.

---

### Edge Cases

- **A closure with no live-bootstrap toolchain** (openai, slack-web). It has zero `fragment` members. That is a correct result, not a degradation.
- **A `kaem`-built member with a version token.** It is already `package` (for example `mes-libc-mini-0.27.1`). The version rung precedes the `fragment` rung and is unaffected.
- **A `kaem`-built member with `env.text`.** It is already `scaffolding` and is unaffected. Live-bootstrap build scripts such as `strncpy-builder` are of this kind.
- **A stdenv-built member listed in `patches`, or with a content hash.** It is already `source` and is unaffected.
- **Several derivations of one name.** For example `expand-response-params` ×6, built once per bootstrap stage. Each is its own member with its own identity, as today, and they share a role because they share evidence.
- **A member that neither the live-bootstrap toolchain nor stdenv built.** If one exists (for example a raw `derivation` call in an older bootstrap such as `bootstrap-tools`), FR-004 applies to it as to any other: `scaffolding` with script evidence, otherwise `package`. It never gets `fragment` by default.
- **A script with no recorded script evidence.** It is `package`, the FR-004 default. The measurement lists every such member so the review can see whether the script evidence is missing any.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The classifier MUST give C206 `fragment` only to a member for which the closure records positive evidence that the live-bootstrap toolchain built it. The plan chooses the evidence from the measured candidates (Assumptions). Absence of other evidence MUST NOT be enough.
- **FR-002**: The earlier rungs MUST be unchanged, in order and in effect: content hash, `patches` listing, `env.text`, version-shaped name. Only members that reach the last rung can change role.
- **FR-003**: The classifier MUST stay total: every member gets exactly one role. The value set stays the five frozen values. A sixth value MUST NOT be added unless this spec is amended to say so.
- **FR-004**: A member that reaches the last rung without live-bootstrap evidence MUST get a role by evidence:
  - **`scaffolding`** when the closure records positive evidence that its output is a hook or helper script that a build sources or runs, as `setup-hook.sh`, `add-flags.sh`, `utils.bash` and `python-setup-hook.sh` are. The plan chooses the evidence after reading all five captures, and records why.
  - **`package`** for a setup-hook package (`makeSetupHook` output, evidenced by `buildCommand` writing `nix-support/setup-hook`) whatever its name, consistent with the same hooks on closures where they declare a `pname`.
  - **`package`** otherwise: programs (`nuke-refs`, `expand-response-params`), data (`mirrors-list`), bootstrap stages and `stdenv-linux`. `package` is the default because it is the conservative graph choice: the walk stops there, so no consumer is fanned out through a member whose nature is unestablished.
  - **Both definitions change.** The C206 catalog row MUST describe each role truthfully. "A version-bearing bootstrap package" is not true of `nuke-refs`, and "`writeText` output" is not true of `add-flags.sh`.
- **FR-005**: The projection MUST NOT change. The edge change MUST follow from the roles alone, as in spec 1492.
- **FR-006**: The closures of all five closure targets MUST be captured in CI. Moat is already captured. The other four (openai, slack-web, shellcheck, slacklinker) are captured by the same public-corpus workflow mechanism. None of them may be evaluated locally. Each capture MUST record its run and sha256.
- **FR-007**: On each captured closure, the #1367 edge accounting and `waybill debug closure-dump` MUST be run before and after the change. Every member that changes role and every edge added or removed MUST be listed or counted and attributed. The results and their review MUST be committed under this feature's `measurements/`.
- **FR-008**: The three formats MUST agree on the changed C206 values and edges. The C206 and C184 parity rows stay as they are.
- **FR-009**: The C184 `waybill:nix-closure` counts MUST describe the new documents: `member-roles`, `edge-ineligible-components` and `edges`.
- **FR-010**: The five closure goldens MUST be regenerated. Each diff MUST be reviewed component by component and edge by edge against the FR-007 measurement. A change no reclassified member explains is a defect, not churn. Non-closure goldens MUST be byte-identical.
- **FR-011**: The C206 catalog row and `docs/reference/nix-evaluation.md` MUST state the new last rung and its evidence. The sentence "`fragment` is now said only of live-bootstrap functions" MUST be true of all five regenerated goldens.

### Key Entities

- **Closure member**: a derivation in the closure that declares no `pname`, emitted as a component since milestone 1282. It has one C206 role and an identity. This feature changes the role of some members and never changes identity.
- **Live-bootstrap evidence**: what the captured closure records about a member that shows the live-bootstrap toolchain built it, as opposed to stdenv.
- **Projected edge**: a dependency edge between two edge-endpoint members, derived from `inputs.drvs` by walking through non-endpoint members.
- **Closure capture**: a closure document produced in CI for one corpus target, with its run ID and sha256, used for measurement only.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On the pinned moat closure, exactly 25 members leave `fragment` and they are the 25 listed in Background: 10 become `scaffolding` and 15 become `package`. 215 keep `fragment`, and no other member changes role.
- **SC-002**: On all five closure goldens, 0 members carrying `fragment` lack live-bootstrap evidence in the captured closure.
- **SC-003**: On each of the five captured closures, every edge that differs before and after has a reclassified member as an endpoint, or replaces a walk through one. 0 differing edges are unexplained.
- **SC-004**: On openai and slack-web, the number of edges each spec-1492 patch contributes is recorded before and after. The measurement says whether the `stdenv-linux` fan-out remains.
- **SC-005**: A fixture control fails if the evidence check is removed: with the check disabled, the stdenv-built fixture member is `fragment` again.
- **SC-006**: The 49 corpus fixture files outside the five closure targets, and the three closure targets' `recall-truth.json`, are byte-identical before and after. Only the 15 closure SBOM files may change.

## Assumptions

- **Evidence candidates (measured on moat; the plan chooses).** Two signals separate the 240 identically: builder is `kaem` (215 true, 25 false), and the derivation carries no `stdenv` attribute (215 true, 25 false). They may diverge on the other closures. The plan picks after the four captures are read, and records why.
- **The other closures are inferred until captured.** The openai and slack-web populations above are read from member names in the goldens, not from derivation attributes. Their counts in SC-002 to SC-004 are established by FR-006, not assumed.
- **No new annotation.** No catalog row or annotation is added. The change is to the values of C206 and to counts in C184.
- **Identity is unchanged.** Role does not feed a member's identity (content hash or recipe hash), so no component's `bom-ref` or PURL changes.
- **Scope of the measurement.** Only x86_64-linux is measured, because that is what the goldens pin.
- **Base.** This branch starts from `main` after #1510 (ac2bd28), so the spec-1492 rung is in place.
