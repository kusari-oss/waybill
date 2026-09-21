# Feature Specification: Repo Observation Report

**Feature Branch**: `924-repo-observation-report`
**Created**: 2026-09-21
**Status**: Draft
**Input**: Issue #932 — "Repo observation report: a versioned, machine-readable account of what waybill understood, ignored, and could not determine"

## Context

waybill has 32 registered readers and **no coverage story**. When a scan under-reports, the failure is silent in both directions: the operator receives an SBOM with fewer components than expected and no indication which parts of their repository waybill did not recognise, and a maintainer handed a bug report has no structured way to learn what that repository actually looked like.

This feature adds a **repo observation report** — a stable, versioned, machine-readable document describing what waybill saw, what it claimed, what it ignored, and **what it could not determine**. It is produced locally and is shareable with maintainers at the operator's discretion.

### The organising principle

**The report records observations and typed uncertainty, not conclusions.**

"I could not determine what this is" is a valid and useful answer *when accompanied by what was observable*. A directory of 47 binary files and a directory of 47 UTF-8 text files are both unclassified, and they lead a reader to entirely different next steps. The report's job is to preserve that difference, not to resolve it.

This is a deliberate departure from how the rest of waybill behaves. Principle IX (Accuracy) requires the SBOM to assert nothing it cannot support; here the *uncertainty itself* is the payload.

### Why this is cheap

`dispatch_file` (`waybill-cli/src/scan_fs/walk_registry/dispatch.rs:30`) already iterates every registered reader and evaluates `reg.patterns.is_match(basename)` for every file on a single pass. Each reader declares a declarative match pattern set keyed by a reader identifier, and per-reader dispatch counts are already tracked.

**"Unclaimed" is the `else` branch of a loop that already runs.** The core signal requires no second traversal.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - An operator learns whether their repository is well supported (Priority: P1)

An operator runs waybill against their repository, gets fewer components than expected, and cannot tell whether that is correct or a gap in waybill. They produce an observation report and see, per directory, which readers claimed files and which directories produced nothing.

**Why this priority**: This is the whole problem. It delivers value to every operator whether or not they ever share the report, and it is the smallest slice that stands alone. Everything else in this feature enriches this census.

**Independent Test**: Run against a repository containing a mix of supported and unsupported project types; confirm the report names the claimed directories with their readers and lists the unclaimed ones, and that the totals reconcile with the files walked.

**Acceptance Scenarios**:

1. **Given** a repository where every project is a supported ecosystem, **When** the report is produced, **Then** every project directory is marked claimed with the reader(s) that claimed it, and no directory is reported as unrecognised.
2. **Given** a repository containing a project type waybill has no reader for, **When** the report is produced, **Then** that directory appears as not claimed by any reader, with its observable properties recorded.
3. **Given** any repository, **When** the report is produced, **Then** the count of files walked equals the sum of files claimed and files unclaimed, so the census is provably complete.
4. **Given** a repository where a reader matched files but produced no components, **When** the report is produced, **Then** that distinction is visible — matched-but-yielded-nothing is reported separately from never-matched.

---

### User Story 2 - Unrecognised directories are named as ecosystems where possible (Priority: P2)

An unclaimed directory reported only as "unknown" tells a maintainer nothing actionable. The report matches unclaimed marker files against a curated table of ecosystems waybill does not yet read, converting "unknown directory" into "this is a Deno project, and waybill has no reader for it".

**Why this priority**: This is what turns a census into a roadmap. It depends on US1's census existing, and it is the highest-value enrichment of it — it is the difference between a maintainer knowing *that* there is a gap and knowing *which* gap.

**Independent Test**: Place marker files for several ecosystems waybill does not support in separate directories; confirm each is named with its ecosystem and marked unsupported, and that a directory with no recognised marker is reported as unrecognised rather than mislabelled.

**Acceptance Scenarios**:

1. **Given** a directory containing a marker file for a known-but-unsupported ecosystem, **When** the report is produced, **Then** the directory is labelled with that ecosystem name and an explicit "no reader" status.
2. **Given** a directory whose marker file belongs to a supported ecosystem but which yielded no components, **When** the report is produced, **Then** it is distinguished from an unsupported ecosystem — the reader exists and did not produce output, which is a different problem.
3. **Given** a directory with no recognised marker of any kind, **When** the report is produced, **Then** it is reported as unrecognised with its observable properties, and **not** assigned an ecosystem inferred from source-file extensions alone.

---

### User Story 3 - Ambiguity is reported with its evidence rather than guessed (Priority: P3)

Some directories cannot be classified from observation alone. Rather than guessing, the report records the ambiguity, the competing interpretations, and the evidence that would let a human or an AI adjudicate.

**Why this priority**: It is what makes the report trustworthy — a report that guesses wrong is worse than one that declines. It builds on US1 and US2 and is the part most likely to need iteration, so it follows them.

**Independent Test**: Run against this repository. `waybill-cli/tests/` contains 89 lockfiles across several ecosystems — 27 `go.mod`, 24 `package.json`, 21 `Cargo.toml` — none of which are waybill's dependencies. Confirm the report marks this as indeterminate with the competing interpretations stated, rather than either treating them as dependencies or silently ignoring them.

**Acceptance Scenarios**:

1. **Given** a directory containing lockfiles from multiple different ecosystems, **When** the report is produced, **Then** it is recorded as indeterminate, listing the ecosystems present and the candidate interpretations (polyglot project / test fixtures / vendored examples).
2. **Given** a directory that cannot be classified at all, **When** the report is produced, **Then** the report still records file count, maximum depth, the proportion of contents that are binary versus text, and an extension histogram.
3. **Given** a directory whose contents are entirely binary, **When** the report is produced, **Then** that fact is explicit and distinguishable from a directory of the same size whose contents are entirely text.
4. **Given** an ambiguous directory, **When** a reader consults the report, **Then** no field asserts a classification the evidence does not support — ambiguity is a recorded state, never resolved by preference.

---

### User Story 4 - A report can be shared without leaking, and compared across submissions (Priority: P4)

Reports are meant to be sent to maintainers. Paths leak internal project names, customer names and employee names. The report is safe by default, and its schema is stable enough that recurring shapes are recognisable across reports from different sources.

**Why this priority**: It gates *external* use rather than the feature's own usefulness — US1–US3 deliver value locally without it. It is last because the redaction rules should be settled against a report whose contents are already known.

**Independent Test**: Produce a report for a repository containing deliberately sensitive-looking directory names; confirm the default output contains no absolute filesystem paths and no file contents, and that the document validates against its published schema.

**Acceptance Scenarios**:

1. **Given** any repository, **When** a report is produced with default settings, **Then** it contains no absolute filesystem paths and no content excerpted from any file.
2. **Given** a report, **When** it is validated against the published schema, **Then** validation succeeds and every field in the document is described by the schema.
3. **Given** two reports produced from the same unchanged repository, **When** they are compared, **Then** they are identical except for fields explicitly declared volatile — so a maintainer can diff submissions and see only real change.
4. **Given** a report produced by an older tool version, **When** it is read by a consumer expecting a newer schema, **Then** the schema version is present and unambiguous so the consumer can decide how to proceed.

---

### Edge Cases

- **An empty repository, or one containing only ignored directories.** The report must be produced successfully and say so, rather than failing or emitting an empty document that reads as an error.
- **A file claimed by more than one reader.** Multiple claims are legitimate; the census must not double-count files when reconciling totals, and all claiming readers must be named.
- **Symlink loops and unreadable directories.** These must be recorded as skipped-with-reason, not silently dropped — a directory waybill could not read is exactly the kind of gap this report exists to surface.
- **Very large repositories.** The report must stay bounded in size regardless of repository size; a per-file listing of a million-file repository is not useful to anyone and would defeat the sharing goal.
- **Vendored dependency trees** (`vendor/`, `node_modules/`, `third_party/`). These are frequently excluded from scanning by policy; the report must distinguish "excluded by policy" from "not recognised", because they demand opposite responses.
- **A directory that is both claimed and ambiguous** — e.g. a supported lockfile sitting beside three others from different ecosystems. The claim and the ambiguity must both be recorded.
- **Generated or build-output directories** containing copies of manifests. These will look like real projects; the report must record the ambiguity rather than assert either reading.

## Requirements *(mandatory)*

### Functional Requirements

#### The census (US1)

- **FR-001**: The report MUST record, for every directory walked, which readers claimed files within it and how many files each claimed.
- **FR-002**: The report MUST record files walked but claimed by no reader, aggregated per directory.
- **FR-003**: The census MUST reconcile: files walked MUST equal files claimed plus files unclaimed plus files skipped, with skips broken down by reason. A report whose totals do not reconcile is invalid.
- **FR-004**: The report MUST distinguish a reader that matched files but produced no components from a reader that never matched — these are different failures with different fixes.
- **FR-005**: The report MUST record which directories were skipped by exclusion policy, separately from those not recognised.
- **FR-006**: The report MUST record the count of components waybill produced per directory, so coverage can be read against output.

#### Naming the unknown (US2)

- **FR-007**: The report MUST match unclaimed marker files against a curated table of ecosystems waybill does not yet support, and name the ecosystem where a marker matches.
- **FR-008**: Ecosystem identification MUST be marker-file-driven. Source-file extensions MUST NOT alone be used to assign an ecosystem, because a directory's marker may sit above the source files it governs. Extension histograms are recorded as observation (FR-011) and MAY inform a stated-as-weak signal, never a classification.
- **FR-009**: The unsupported-ecosystem table MUST be data, editable without code changes, so it can be extended as reports reveal new gaps.
- **FR-010**: The report MUST NOT widen or reuse the file-tier source-shape allowlist that governs SBOM emission; ecosystem naming is a separate concern and changing that allowlist would change emitted SBOM content.

#### Typed uncertainty (US3)

- **FR-011**: For any directory not confidently classified, the report MUST record: file count, maximum depth, an extension histogram, and the proportion of files that are binary versus text.
- **FR-012**: The report MUST represent ambiguity as a first-class state carrying the competing interpretations, not as a missing or null classification.
- **FR-013**: Where a directory contains marker files from multiple ecosystems, the report MUST record every ecosystem observed and MUST NOT select one as authoritative.
- **FR-014**: The report MUST NOT assert a classification the recorded evidence does not support. Where the evidence is genuinely ambiguous, the ambiguity is the answer.
- **FR-015**: Each ambiguity record MUST carry the evidence that produced it, so a reader can evaluate the call independently rather than trusting it.

#### Shape, stability and safety (US4)

- **FR-016**: The report MUST be a machine-readable document with a published schema, and MUST carry an explicit schema version.
- **FR-017**: The schema MUST be marked unstable/alpha, and consumers MUST be expected to tolerate the addition of new fields without breaking.
- **FR-018**: The report MUST be readable by both a person and a language model without bespoke tooling — named fields and explicit enumerated values, not positional or compact encodings.
- **FR-019**: The default report MUST contain no absolute filesystem paths and no content excerpted from any scanned file.
- **FR-020**: The report MUST be deterministic: two runs against an unchanged repository MUST produce identical documents except for fields explicitly declared volatile, and those fields MUST be enumerated in the schema.
- **FR-021**: The report MUST be bounded in size independently of repository size, degrading to aggregates rather than growing a per-file listing without limit.
- **FR-022**: The report MUST be producible without network access.
- **FR-023**: The report MUST NOT be transmitted anywhere automatically. Sharing is an operator action taken after reading it.
- **FR-024**: Producing the report MUST NOT change emitted SBOM content in any format.

#### Principle V — standards-native audit

- **FR-025**: **Audit result, recorded per Principle V.** The target SBOM formats were audited for an existing native construct carrying this semantic: **none exists.** CycloneDX and SPDX describe the *components* a tool concluded exist; neither models a tool's traversal of a source tree, its per-reader match decisions, or its own uncertainty. SARIF was also considered as the nearest general tool-output standard and rejected: it models findings against source locations under rules, whereas this report is an inventory and census carrying typed uncertainty, and expressing "this directory contains 47 binary files and no recognisable marker" as a rule violation would distort both the data and the standard. This feature therefore defines a new document rather than extending an existing one, and introduces **no** new `waybill:*` property, annotation, or relationship type into any emitted SBOM (FR-024).

### Key Entities

- **Observation Report**: The root document. Carries schema version, the tool version that produced it, the volatile-field declaration, and repository-level totals.
- **Directory Observation**: One record per directory walked. Carries its repository-relative location, classification, per-reader claims, unclaimed file count, component count, and — where unclassified — the observable properties of FR-011.
- **Classification**: The typed verdict on a directory. Distinguishes at minimum: claimed by reader(s); recognised as an unsupported ecosystem; excluded by policy; indeterminate (with interpretations); unrecognised.
- **Ambiguity Record**: A competing-interpretations statement attached to a directory, carrying the candidate readings and the evidence for each.
- **Ecosystem Marker Entry**: A curated mapping from marker filename to ecosystem name and waybill support status. Data, not code (FR-009).
- **Reader Coverage Entry**: Per-reader totals — files matched, components produced — supporting the FR-004 distinction.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Run against this repository, the report classifies `waybill-cli/tests/` as indeterminate and names the competing interpretations, rather than treating its 89 lockfiles as dependencies or omitting them.
- **SC-002**: A maintainer given only a report for an unfamiliar repository can name every ecosystem present and state which of them waybill supports, without access to the repository. Verified by having a reader who has not seen the repository answer both questions from the report alone.
- **SC-003**: The census reconciles exactly on every test repository: files walked equals claimed plus unclaimed plus skipped, with zero unexplained residual.
- **SC-004**: The default report contains zero absolute filesystem paths and zero bytes excerpted from any scanned file, verified mechanically.
- **SC-005**: Two consecutive runs against an unchanged repository produce byte-identical reports after the declared volatile fields are masked.
- **SC-006**: Every field emitted validates against the published schema, and the schema describes every field emitted — neither document drifts ahead of the other.
- **SC-007**: The report is produced with no outbound network requests, verified with network access denied.
- **SC-008**: Report size stays bounded: the report for a repository an order of magnitude larger than another is not an order of magnitude larger.
- **SC-009**: Producing a report leaves emitted SBOM content byte-identical to a run that does not produce one, across all supported formats.
- **SC-010**: For a repository containing marker files from at least three ecosystems waybill does not support, all three are named with an explicit no-reader status.

## Assumptions

- **The reporting pass reuses the existing single traversal.** The claimed/unclaimed decision is already computed per file during reader dispatch; this feature surfaces it rather than re-walking. A second full traversal would be a material cost and is assumed unnecessary.
- **Reports are produced on demand, not as a side effect of every scan.** An operator investigating a gap asks for a report; a routine scan does not pay for one. This keeps FR-024 trivially true and avoids taxing the common path.
- **The unsupported-ecosystem table starts small and grows from evidence.** It is seeded with a modest set of well-known markers and extended as incoming reports reveal what is actually encountered. Completeness at v1 is not a goal.
- **"Binary versus text" is a heuristic, and is reported as such.** A standard content-sniffing approach is assumed sufficient; the report describes what was observed, not a guarantee about file semantics.
- **Repository-relative paths are retained by default; absolute paths never are.** Absolute paths leak home directories and usernames with no compensating value. Repository-relative paths carry the structural information that makes the report useful. See the clarification below on whether relative paths need redaction as well.
- **The schema is expected to change.** It ships marked alpha (FR-017), and consumers are expected to tolerate additive change. Stability is a goal for the *shape* of recurring records, not a compatibility guarantee.
- **This repository is the primary test fixture.** Its `waybill-cli/tests/` tree is a genuine, non-synthetic instance of the hardest case in the feature (SC-001), which makes it a better fixture than anything constructed for the purpose.

## Outstanding Clarification

- **FR-019 / default redaction of repository-relative paths**: [NEEDS CLARIFICATION: The default strips absolute paths and file contents. Should repository-relative directory paths (e.g. `services/billing/`) also be redacted by default — replaced with stable per-segment hashes that preserve structure and correlation but not names — or retained by default with redaction offered as an opt-in mode? Retaining them makes reports substantially more useful to a maintainer; redacting them makes reports safe to send without review. This choice materially changes the artifact's value and its adoption, and no default is obviously correct.]
