# Feature Specification: Nightly SBOM Quality Regression Corpus

**Feature Branch**: `770-sbom-quality-corpus`
**Created**: 2026-09-03
**Status**: Draft
**Input**: User description (issue #770): "A test job that runs occasionally (weekly, and on-demand to start; likely nightly) which fetches a list of git repositories and runs waybill against each. For each repo record execution wall time, quality as measured by `sbomqs`, number of components, and whether the relationships are flat or not. Numerical benchmarks support a per-repo acceptable range; values inside the range pass, values outside are flagged."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Measure SBOM quality across a real-world corpus (Priority: P1)

A waybill maintainer wants to know what waybill actually produces when pointed at real
open-source projects, rather than at the synthetic fixtures used by the perf suite. They
run the corpus command and receive one record per repository containing four measurements:
how long the scan took, how good the resulting SBOM is, how many components it found
(split by whether they are packages or bare files), and whether the dependency
relationships form a real graph or a flat list hanging off the root.

**Why this priority**: Nothing else in the feature works without the measurements. On its
own this already delivers value — it is the first time waybill's real-world output is
characterised in one place, and it is the data a maintainer needs in order to author the
acceptable ranges that User Story 2 depends on.

**Independent Test**: Run the command against the committed corpus with no ranges defined
at all. It must produce a complete report for every reachable repository and exit zero.
Delivers a characterisation of waybill's current real-world behaviour.

**Acceptance Scenarios**:

1. **Given** a corpus entry naming a public repository and a pinned commit, **When** the
   corpus command runs, **Then** the report contains that repository's wall time, quality
   score, package-tier count, file-tier count, and flatness measurements.
2. **Given** a repository whose dependency graph has depth greater than one, **When** it is
   measured, **Then** it is recorded as not flat, and the recorded maximum depth matches the
   longest path from the root component.
3. **Given** a repository whose components all hang directly off the root, **When** it is
   measured, **Then** it is recorded as flat regardless of what waybill's own
   graph-completeness annotation claims.
4. **Given** any measured repository, **When** the report is written, **Then** it also
   records waybill's own graph-completeness annotation as a separate field, so the
   independent measurement and waybill's self-report can be compared.

---

### User Story 2 - Fail when a measurement leaves its acceptable range (Priority: P2)

A maintainer has hand-authored an acceptable range for each measurement on each repository.
When a change to waybill pushes any measurement outside its range — fewer components than
expected, a lower quality score, a graph that went flat, a scan that got dramatically
slower — the job fails and names exactly which repository, which measurement, what was
expected, and what was observed.

**Why this priority**: This converts the measurements into a regression gate. It depends on
US1 producing measurements and on a maintainer having authored ranges from them, so it
cannot come first.

**Independent Test**: Author a range deliberately narrower than a known-good observed value,
run the corpus, and confirm the job fails and names that specific repository and
measurement. Widen the range and confirm it passes.

**Acceptance Scenarios**:

1. **Given** a repository whose expected component count is a range and whose observed count
   falls inside it, **When** the corpus runs, **Then** that measurement is reported as
   passing.
2. **Given** an observed value below the low bound or above the high bound, **When** the
   corpus runs, **Then** that measurement is flagged and the command exits non-zero.
3. **Given** several repositories with several out-of-range measurements, **When** the
   corpus runs, **Then** every violation is reported — not only the first — before the
   command exits.
4. **Given** a repository expected to be flat and observed to be flat, **When** the corpus
   runs, **Then** no violation is raised for flatness.
5. **Given** a repository expected not to be flat but observed as flat, **When** the corpus
   runs, **Then** a violation is raised naming the collapse.
6. **Given** a repository in the corpus that has no ranges authored yet, **When** the corpus
   runs, **Then** its measurements are reported but cannot cause failure.

---

### User Story 3 - Run automatically overnight and on demand (Priority: P3)

The corpus runs on a schedule without anyone asking, and any maintainer can trigger it by
hand against a branch. Its results are retained as a downloadable record so trends can be
inspected after the fact, and a failing run is visible as a failed job.

**Why this priority**: Automation multiplies the value of US1 and US2 but neither depends
on it — a maintainer can run the corpus locally and get the whole benefit. Scheduling is
the last slice.

**Independent Test**: Trigger the job manually against a branch and confirm it runs the full
corpus, publishes the report as a retrievable record, and reflects pass/fail in the job
outcome.

**Acceptance Scenarios**:

1. **Given** the scheduled trigger fires, **When** the job runs, **Then** the full corpus is
   measured and the report is retained.
2. **Given** a maintainer triggers the job manually and names a branch, **When** the job
   runs, **Then** the corpus is measured against that branch's build of waybill.
3. **Given** any measurement is out of range, **When** the job completes, **Then** the job
   is marked failed and the report is still retained for inspection.

---

### Edge Cases

- A repository is unreachable, or the pinned commit no longer exists (history rewrite, repo
  deleted or made private). The run must distinguish "could not measure" from "measured and
  out of range" and must not silently score the repository as zero.
- The quality-scoring tool is not installed. The run must say so loudly rather than treating
  a missing score as a passing score.
- waybill itself exits non-zero on a repository, or produces no output document.
- A scan exceeds its time budget and must be abandoned without stranding the rest of the run.
- A repository legitimately produces zero package-tier components (observed: a Ruby project
  with no committed lockfile). The measurement is valid and must be reportable as a range.
- A repository is permanently flat because it commits no lockfile. Its flatness expectation
  is "flat", and that must be expressible rather than being treated as a defect.
- An acceptable range is malformed — a low bound above its high bound, or a negative count.
  This is a configuration error and must be reported as such, distinctly from a measurement
  violation.
- Two repositories in the corpus are given the same name.
- The corpus is empty, or a filter selects no repositories.

## Requirements *(mandatory)*

### Functional Requirements

**Corpus definition**

- **FR-001**: The system MUST read its corpus from a committed configuration file, so that
  adding, removing, or re-pinning a repository is a reviewable data change requiring no code
  change.
- **FR-002**: Each corpus entry MUST carry a unique name, a clone location, a pinned commit
  identifier, and an optional set of acceptable ranges.
- **FR-003**: The corpus entry's pinned commit MUST be expressed such that it can later be
  replaced by a moving branch reference without restructuring the configuration.
- **FR-004**: The system MUST reject a corpus containing duplicate entry names.

**Acquisition**

- **FR-005**: The system MUST retrieve each repository at exactly its pinned commit,
  fetching only the content of that commit and not the repository's history.
- **FR-006**: The system MUST NOT retrieve nested sub-repositories; only the outer
  repository's own tracked content is measured.
- **FR-007**: When a repository cannot be retrieved, the system MUST record that entry as
  unmeasurable with the reason, continue with the remaining entries, and cause the run to
  fail.

**Measurement**

- **FR-008**: The system MUST scan each repository with network access disabled, so that a
  run's results depend only on repository content and waybill's behaviour.
- **FR-009**: The recorded wall time MUST cover only the scan itself, excluding retrieval,
  scoring, and analysis.
- **FR-010**: The system MUST record a quality score for each repository's emitted SBOM,
  obtained from the external quality-scoring tool named in the corpus configuration.
- **FR-011**: The system MUST record component counts split into package-tier components
  (those carrying a package identifier) and file-tier components (those without one), as
  separate measurements that can be ranged independently.
- **FR-012**: The system MUST determine whether relationships are flat by examining the
  emitted document's own relationship structure, independently of any self-assessment
  waybill emits. The determination MUST be derived from the number of relationships, the
  number of components having at least one outgoing relationship, and the greatest distance
  from the document's root component.
- **FR-013**: The system MUST additionally record waybill's own graph-completeness
  self-assessment as a distinct field, so divergence between the two can be observed.
- **FR-014**: The system MUST abandon any scan exceeding a configurable per-repository time
  budget, record it as unmeasurable, and continue.
- **FR-015**: The system MUST pin the version of the external quality-scoring tool it
  expects, and MUST report a mismatch rather than silently scoring against a different
  version.
- **FR-016**: When the external quality-scoring tool is unavailable, the system MUST report
  the run as failed rather than recording a missing score as acceptable.

**Evaluation**

- **FR-017**: For each measurement having an authored acceptable range, the system MUST
  determine whether the observed value falls within that range inclusive of both bounds.
- **FR-018**: The system MUST evaluate every measurement on every repository before exiting,
  reporting all violations rather than stopping at the first.
- **FR-019**: The system MUST exit with a failure status when any measurement is out of
  range, or any entry is unmeasurable.
- **FR-020**: A measurement with no authored range MUST be reported but MUST NOT be capable
  of causing failure.
- **FR-021**: The system MUST report a malformed range as a configuration error, distinctly
  from a measurement violation.
- **FR-022**: A flatness expectation MUST be expressible as either "expected flat" or
  "expected not flat", and a mismatch MUST be a violation.

**Reporting**

- **FR-023**: The system MUST emit a machine-readable report containing every measurement,
  its expected range where one exists, and its pass, fail, or unmeasured status.
- **FR-024**: The system MUST emit a human-readable summary naming, for each violation, the
  repository, the measurement, the expected range, and the observed value.
- **FR-025**: The report MUST record which waybill revision and which corpus revision
  produced it, so a result can be attributed later.
- **FR-026**: Report contents MUST be ordered deterministically so two runs over identical
  inputs differ only in genuinely varying measurements.

**Operation**

- **FR-027**: The system MUST support restricting a run to a subset of the corpus, so a
  maintainer can iterate on one repository without paying for all of them.
- **FR-028**: The system MUST run on a recurring schedule and on manual request.
- **FR-029**: A scheduled or manually triggered run MUST retain its report as a retrievable
  record, including when the run fails.
- **FR-030**: The measurement set MUST be structured so that scoring additional SBOM output
  formats later is an additive change, not a restructuring.

**Project constraints**

- **FR-031**: The feature MUST NOT add a crate to the workspace. It extends the existing
  task-runner crate, so Constitution Principle VI's three-crate rule and its
  amendment requirement are not engaged.
- **FR-032**: The feature MUST NOT add runtime dependencies to the shipped waybill binary.

### Key Entities

- **Corpus Target**: One repository under measurement. Has a unique name, a location, a
  pinned commit, and optionally a set of expectations. Represents the unit a maintainer
  adds, removes, or re-pins.
- **Expectation**: An acceptable range for one measurement on one target — a low and high
  bound for numeric measurements, or a required value for flatness. Hand-authored.
  Absence means "observe but do not gate".
- **Measurement**: One observed value for one target — wall time, quality score,
  package-tier count, file-tier count, relationship count, components with outgoing
  relationships, greatest depth, flatness, and waybill's own self-assessment.
- **Violation**: The pairing of a Measurement with the Expectation it failed, carrying
  enough context to name the repository, the measurement, the bound, and the observation.
- **Corpus Report**: The complete record of one run — every target, every measurement,
  every violation, plus the waybill and corpus revisions that produced it.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A maintainer can determine, from one run's output alone, which repositories
  regressed and in what respect, without reading logs or re-running anything.
- **SC-002**: A deliberately introduced regression that empties a dependency graph, drops
  component counts, or lowers quality scores causes the run to fail and names the affected
  repositories.
- **SC-003**: Ten consecutive runs against an unchanged waybill revision produce zero
  violations, demonstrating the gate does not fire on its own variance.
- **SC-004**: Adding a repository to the corpus requires editing only the configuration
  file.
- **SC-005**: A complete run finishes within the scheduled job's time budget, with the
  slowest single repository accounting for no more than 80% of total runtime.
- **SC-006**: Retrieving the entire corpus consumes under 5 GB of working disk.
- **SC-007**: The workspace gains no new crate and the shipped waybill binary gains no new
  dependency.
- **SC-008**: For every repository, the report allows a reader to compare waybill's
  self-assessed graph completeness against the independently measured structure.

## Assumptions

- **Offline measurement is a floor, not a ceiling.** Scanning with network access disabled
  makes runs reproducible and attributable, but it also means the measurements reflect what
  waybill can determine without its network resolution ladders. Improvements that only
  manifest online will not be visible to this gate. This is accepted deliberately in
  exchange for determinism; it is called out because Constitution Principle VIII
  (Completeness) invites the opposite reading.
- **Pinned commits measure waybill, not the world.** With pins, a changed measurement is
  attributable to a waybill change. The configuration is shaped so a later switch to moving
  references is a data change, but the consequence of that switch — measurements moving for
  reasons unrelated to waybill — is understood and not solved here.
- **Nested sub-repositories are intentionally absent.** At least one corpus repository keeps
  much of its third-party content in nested sub-repositories that a plain retrieval leaves
  empty. Its component counts are correspondingly lower than a developer's working copy
  would produce. This is deterministic and therefore rangeable; it is documented so the
  numbers are not misread.
- **Ranges are hand-authored from observed values.** No automatic capture-and-bless mode is
  provided. A maintainer runs the corpus, reads the observed values, and writes ranges
  deliberately. New repositories are therefore observe-only until someone authors ranges.
- **Wall time is measured on shared infrastructure.** Because only the scan is timed and the
  scan runs offline, network variance is excluded — but processor variance on shared
  build machines is not. Wall-time ranges must be authored wide enough to absorb it.
- **File-tier components are included, not filtered.** Several repositories yield large
  numbers of file-tier components — shell scripts and similar unattributed content. These
  are counted separately from package-tier components rather than being filtered out, so
  that a change in either moves its own measurement. Filtering was considered and rejected:
  the available tier filter does not remove file-tier content, and it deletes the
  manifest-declared components of repositories that commit no lockfile.
- **Repositories without committed lockfiles are legitimate corpus members.** Several
  produce a flat graph permanently. They are retained because they still exercise reader
  paths and still detect count and quality regressions; their flatness expectation is
  simply "flat".
- **The quality-scoring tool is an external dependency.** It is not vendored. Its absence
  fails the run rather than degrading it, and its version is pinned because its output
  shape changes between releases.
- **Container images are out of scope for this milestone.** The corpus is git repositories
  only, matching the original request. The existing public-corpus suite already covers an
  image target.

### Baseline observations

Measured against 18 candidate repositories at the pins under consideration, scanning
offline. These are the inputs from which ranges will be authored; they are recorded here
so the spec's assumptions can be checked against reality.

- Quality scores occupied a narrow band — roughly 5.7 to 7.7 on a ten-point scale, with no
  repository scoring above that band. Ranges will need to be correspondingly tight to carry
  meaning.
- Seven of eighteen repositories produced a flat graph. In three cases the cause was an
  absent committed lockfile in the upstream project, not waybill behaviour.
- Three repositories reported their graph as complete while measuring as structurally flat.
  This divergence is the direct justification for FR-012 and FR-013.
- Scan durations ranged from under a tenth of a second to just over one hundred seconds. A
  single repository accounted for roughly three quarters of total scan time.
- Retrieval of all eighteen repositories took approximately ninety seconds and 2.2 GB.
