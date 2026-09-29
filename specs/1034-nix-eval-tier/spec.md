# Feature Specification: Opt-in `nix eval` resolution tier

**Feature Branch**: `1034-nix-eval-tier`
**Created**: 2026-09-28
**Status**: Draft
**Issue**: **#971 part A**. Note the directory number is the next sequential
milestone number and is *not* an issue reference — issue #1034 is a different,
downstream piece of work (full-closure default) that this feature unblocks.
**Input**: An opt-in `--nix-eval` resolution tier. waybill resolves Nix-built
projects by fetching and parsing nixpkgs files. That path cannot see what only
evaluation knows. This adds a tier that invokes `nix` when the operator opts in,
in the shape the project already uses for the same trade — `--gradle-resolve`
(m235) and `--helm-render` (m203): file-parsing stays the default and the
fallback, and an absent, slow, or failing `nix` degrades to today's behaviour
with a reason code.

## Why this exists

waybill currently learns a Nix-built Haskell project's package versions by
fetching `hackage-packages.nix` and `configuration-ghc-*.nix` at the flake's
pinned nixpkgs revision and parsing them. That is a reconstruction of what Nix
*would* compute. It is not what Nix computes.

Milestone 1033 is the proof: two components shipped with wrong versions because
`configuration-common.nix` — which supersedes the generated set — was not being
read. The fix was to read one more file. The next such bug will be a different
file, or a construct no file-parser reproduces. Evaluation is the authority; the
files are an approximation of it.

Four things only evaluation knows:

| | why file-parsing can't see it |
|---|---|
| package-set overrides | spread across files that override each other in evaluation order |
| the compiler a flake actually selects | chosen by expressions, not declared |
| `meta.knownVulnerabilities` | attribute on the derivation, not in the version tables |
| the derivation closure | exists only after evaluation |

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Correct versions for a Nix-built project (Priority: P1)

An operator scanning a project whose dependencies come from nixpkgs opts into
evaluation, and the emitted SBOM carries the versions Nix actually resolves,
including where an override supersedes the generated package set.

**Why this priority**: This is the defect class the feature exists to close, and
it is the only story that delivers value alone. Without it the tier has no
reason to run.

**Independent Test**: Scan the corpus Haskell target pinned to nixpkgs revision
`cbb5cf358f50aa6acc9efd6113b7bcfbc352cd73` with the flag on, and compare each
resolved version against `nix eval` at the same revision and system. The
comparison harness already exists — the oracle shipped as #971 part B
(`19271a4c`).

**Acceptance Scenarios**:

1. **Given** a project whose flake pins a nixpkgs revision in which a package's
   version is overridden away from the generated set, **When** the operator
   scans with the tier enabled, **Then** the emitted version is the one `nix
   eval` reports, not the one the generated set declares.
2. **Given** the same project, **When** the operator scans with the tier
   disabled, **Then** the emitted document is byte-identical to what waybill
   produces today.
3. **Given** a component whose file-parsed and evaluated versions agree,
   **When** the tier runs, **Then** exactly one version is emitted and no
   divergence metadata appears.

---

### User Story 2 - The tier never breaks a scan (Priority: P1)

An operator enables the tier on a machine where `nix` is absent, or where
evaluation fails or exceeds its budget. The scan completes, produces the same
document the disabled tier would produce, and says in the document why
evaluation did not contribute.

**Why this priority**: Equal to US1. A resolution tier that can fail a scan is
not adoptable, and the flag would be enabled in CI where `nix` may be absent on
some runners and present on others. Independently valuable: it makes the flag
safe to set unconditionally.

**Independent Test**: Run the scan with the flag on and `nix` removed from
`PATH`. Assert exit status is success, and that the output differs from the
flag-off output only by the reason-code metadata.

**Acceptance Scenarios**:

1. **Given** `nix` is not on `PATH`, **When** the operator scans with the tier
   enabled, **Then** the scan succeeds, file-parsing results are emitted, and a
   reason code records that the tool was unavailable.
2. **Given** `nix` is present but evaluation exits non-zero, **When** the
   operator scans with the tier enabled, **Then** the scan succeeds and a
   reason code distinguishes evaluation failure from tool absence.
3. **Given** evaluation exceeds its time budget, **When** the budget expires,
   **Then** the scan succeeds and a reason code distinguishes timeout from the
   other two.
4. **Given** a flake that exposes no usable package output, **When** the tier
   runs, **Then** the scan succeeds and a reason code records that no
   evaluable attribute was found.

---

### User Story 3 - Evaluation is confined and cannot build (Priority: P1)

An operator enables the tier against a checkout they do not fully trust.
Evaluation does not build anything, does not run a builder, and is confined to
nixpkgs at the revision the project pins.

**Why this priority**: Also P1, because it is a property the other two stories
would otherwise silently remove. waybill is a read-only parser today.

**Scope correction (2026-09-29, from implementation).** An earlier draft of this
story claimed the tier evaluates expressions the scanned repository *authors*.
It does not. The implementation reads the pinned revision out of `flake.lock`
and evaluates `github:NixOS/nixpkgs/<rev>` attributes; the project's own
`flake.nix` is never evaluated. This was found by mutation-testing the story's
own acceptance test: removing the import-from-derivation refusal did not make
the test fail, because the repository-authored expression was never reached.

That narrows the threat but does not remove it, and the controls stay:

1. **nixpkgs is still code waybill neither authors nor audits.** Whether any
   `haskellPackages.<pkg>.version` attribute triggers import-from-derivation
   has not been measured; refusing it is cheap and does not depend on the
   answer.
2. **The repository controls the revision string**, which is interpolated into
   a Nix expression. Measured: a crafted `rev` in `flake.lock` does flow
   through to the expression builder. It is not exploitable today only because
   the package-set fetch for the same revision runs first and fails on a
   non-revision — defence by accident. The revision is now validated as 40 hex
   characters before use, which makes it defence by design.
3. **Evaluating project flakes is required by the work this unblocks** —
   research §R8's attribute-path strategy, and issues #1034 and #1040. The
   controls are what make that step additive rather than a new risk.

**Independent Test**: Scan a fixture whose own flake would build on evaluation
and assert nothing is built; separately, drive the pre-flight with a `nix` whose
`config show` omits the setting and assert the tier refuses to evaluate.

**Acceptance Scenarios**:

1. **Given** a `nix` that does not honour `allow-import-from-derivation`,
   **When** the tier runs, **Then** it does not evaluate and degrades with a
   reason distinguishing this from an absent or unusable tool.
2. **Given** any project, **When** the tier runs, **Then** evaluation runs in
   Nix's pure mode, in which the host environment is not readable — measured:
   `builtins.getEnv "HOME"` returns `""`.
3. **Given** a `flake.lock` whose `rev` is not a 40-character hex object id,
   **When** the tier runs, **Then** the revision is refused rather than
   interpolated into a Nix expression.
4. **Given** the operator has not passed the flag, **When** a scan runs,
   **Then** no `nix` process is started.

---

### User Story 4 - The result says which machine it describes (Priority: P2)

An operator can name the platform to evaluate for, and the document records
which platform the evaluated results describe.

**Why this priority**: P2 because US1 is useful on one machine without it, but
without it the SBOM silently means something different depending on where it
ran. Measured: `hinotify` is `0.4.2` on `x86_64-linux` and `0.1.8` on
`aarch64-darwin` at the same nixpkgs revision.

**Independent Test**: Evaluate the same project twice naming two platforms, and
assert the documents differ in the expected component versions and each records
the platform it was evaluated for.

**Acceptance Scenarios**:

1. **Given** no platform is named, **When** the tier runs, **Then** it evaluates
   for the host's platform and records that platform in the document.
2. **Given** a platform is named that the flake does not support, **When** the
   tier runs, **Then** the scan succeeds and degrades with a reason code.

---

### User Story 5 - A disagreement stays visible (Priority: P2)

When evaluation and file-parsing disagree about a version, the document carries
both: the evaluated value as the component's version, and the file-parsed value
as metadata.

**Why this priority**: P2 — US1 is complete without it — but it is the property
that makes the *next* #1033 findable instead of silent. The 1033 defect was
discovered precisely because two sources could be compared.

**Independent Test**: Scan a project with a known override, and assert the
component carries the evaluated version and metadata naming the superseded
file-parsed version.

**Acceptance Scenarios**:

1. **Given** evaluation and file-parsing produce different versions for a
   component, **When** the tier runs, **Then** the component's version is the
   evaluated one and the file-parsed one is retained as metadata.
2. **Given** a scan with divergences, **When** it completes, **Then** a
   document-level count of divergences is emitted.

---

### Edge Cases

- **`nix` absent from `PATH`** — degrade, reason code, scan succeeds (US2).
- **`nix` present but unusable** (daemon not running, store not writable) —
  must be distinguishable from absence, because the operator's remedy differs.
- **Flake exposes no usable package output** — measured: haskell-language-server
  exposes no `default` package, only `docs` and devShells. No universal
  attribute path can be assumed.
- **Evaluation attempts import-from-derivation** — refuse, do not build (US3).
- **Evaluation does not terminate** — Nix evaluation is Turing-complete, so a
  bound is required. *The bound's default value is not yet measured; see
  FR-012 and Assumption A-7.*
- **The project has no flake** — the tier is inert, and its absence is not an
  error.
- **The pinned revision cannot be fetched** — degrade with a reason code
  distinguishable from evaluation failure.
- **Evaluation names a component file-parsing never saw, or omits one it did** —
  the document must not silently drop either.
- **A cold Nix store** — measured: nixpkgs source occupies 300–335 MB per
  revision in the store, against 16 MB per revision for today's file-parsing
  path (`hackage-packages.nix` 16 MB plus six `configuration-*.nix` files,
  ~150 KB). A first run on a cold store pays that fetch.

## Requirements *(mandatory)*

### Functional Requirements

**Opting in**

- **FR-001**: The tier MUST be off by default and MUST be enabled only by an
  explicit operator flag on the scan command.
- **FR-002**: With the tier off, waybill MUST start no `nix` process and MUST
  emit documents byte-identical to those it emits today.
- **FR-003**: The flag's help text MUST lead with the fact that enabling it
  **executes code**, and MUST carry the operating guidance: run it sandboxed, or
  only against a flake you trust. It MUST also state precisely what is evaluated
  — nixpkgs at the revision the project pins, **not** the project's own flake —
  so the warning is actionable rather than vague, and so it neither overstates
  the exposure nor omits that `nix` runs.
- **FR-003a**: Reference documentation MUST carry the same warning and guidance
  alongside the accuracy argument, so an operator deciding whether to enable the
  tier sees both halves of the trade in one place.

**Resolution**

- **FR-004**: When the tier runs successfully, evaluated versions MUST take
  precedence over file-parsed versions for the same component.
- **FR-005**: When an evaluated version supersedes a file-parsed one, the
  file-parsed value MUST be retained as component metadata.
- **FR-006**: The system MUST emit a document-level count of superseded
  components.
- **FR-007**: The system MUST NOT assume any particular attribute path exists in
  a flake; a flake exposing no evaluable package output is a degradation case,
  not an error.

**Safety**

- **FR-008**: Evaluation MUST run with import-from-derivation refused, and the
  refusal MUST be verified in effect before anything is evaluated. Requesting
  it is not evidence it applied: a `nix` that does not know the setting accepts
  the flag, warns, and exits 0 (research R3).
- **FR-008a**: The pinned revision MUST be validated as a 40-character
  lowercase hex object id before being interpolated into a Nix expression, and
  MUST degrade otherwise. The value reaches waybill from the scanned
  repository's `flake.lock`, and a Nix expression is code.
- **FR-009**: Evaluation MUST run in Nix's pure mode, so the host environment is
  not readable by the evaluated expressions.
- **FR-010**: Evaluation MUST NOT write outside the Nix store and waybill's own
  cache.

**Degradation**

- **FR-011**: Every failure mode MUST degrade to file-parsing and MUST NOT fail
  the scan.
- **FR-012**: Evaluation MUST be bounded in wall-clock time, and exceeding the
  bound MUST degrade. The bound covers acquiring the pinned revision as well as
  evaluating it, because `getFlake` fetches during evaluation and the two share
  one subprocess — so the default MUST accommodate a cold Nix store, and cannot
  be derived from evaluation timings alone. Until a cold-store cost is measured
  (task T-R5, which needs a clean runner), the default is provisional and MUST
  be labelled as such where it is defined.
- **FR-013**: The system MUST emit a reason code that distinguishes, at minimum:
  `--offline` requested; tool absent; tool present but unusable; pinned
  revision unfetchable; no evaluable attribute; evaluation failed; evaluation
  refused (import-from-derivation); evaluation exceeded its bound.
- **FR-013a**: When `--offline` is set the tier MUST NOT run. Evaluation
  resolves the pinned revision through `getFlake`, which fetches when the Nix
  store lacks it — measured — and Nix's own `--offline` governs substituters,
  not flake inputs. A promise of "no outbound network calls" that holds only
  when a cache happens to be warm is not one.

**Platform**

- **FR-014**: The platform to evaluate for MUST be an explicit parameter,
  defaulting to the host's.
- **FR-015**: The document MUST record which platform the evaluated results
  describe.

**Emission**

- **FR-016**: The document MUST record whether the tier ran, and at which
  nixpkgs revision.
- **FR-017**: Every annotation this feature introduces MUST appear in all three
  emitted formats with a matching parity-catalog entry and extractor, per the
  existing project convention.

**Reachability (this feature only makes these possible; it does not do them)**

- **FR-018**: A single successful evaluation MUST yield, in one pass, the
  attributes that issues #1034 and #1040 will need — at minimum the derivation
  closure and each component's `meta` — even though this feature emits neither.
  Verified by a test that asserts those attributes are present in what
  evaluation returned, not by any emitted output.
- **FR-019**: Consequently, adding closure emission (#1034) or vulnerability
  metadata (#1040) MUST NOT require a second evaluation pass. Motivating
  measurement for #1040: nixpkgs `unzip` carries 13 CVE-named patches against a
  version string unchanged since 2009, so version-based vulnerability matching
  is wrong for nixpkgs in both directions.

### Key Entities

- **Evaluation result**: what `nix` reported for one project at one revision and
  one platform — a set of component identities with versions, plus the platform
  and revision the set is valid for.
- **Resolution divergence**: one component for which evaluation and file-parsing
  disagreed — the evaluated value, the file-parsed value, and which won.
- **Degradation reason**: why the tier did not contribute, drawn from the
  enumerated set in FR-013.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On the corpus Haskell target pinned to nixpkgs revision
  `cbb5cf358f50aa6acc9efd6113b7bcfbc352cd73`, every component the tier resolves
  carries the version `nix eval` reports for it — zero `Disagree` verdicts from
  the existing oracle (`19271a4c`).
- **SC-002**: With the flag off, all committed corpus goldens are unchanged.
- **SC-003**: With the flag on and `nix` absent from `PATH`, the scan exits
  successfully, and its output differs from the flag-off output only in the
  degradation-reason metadata.
- **SC-004**: A flake that attempts import-from-derivation produces no built
  derivation, and the scan completes by degrading.
- **SC-005**: Evaluating the same project for two platforms produces documents
  that differ in component versions and each name their platform — demonstrated
  on the already-measured case (`hinotify` 0.4.2 / 0.1.8).
- **SC-006**: Each of the eight reason codes in FR-013 is reachable by a test
  that provokes it.
- **SC-007**: The added wall-clock cost of a warm-store evaluation stays within
  a bound expressed as a ratio against the flag-off scan time of a named corpus
  target. *The baseline and the ratio are established by a planning-phase
  measurement, not quoted here.*
- **SC-008**: For a project with a known override, the emitted component carries
  the evaluated version, carries the superseded file-parsed version as metadata,
  and the document carries a divergence count equal to the number of such
  components.
- **SC-009**: A test asserts that one evaluation pass returned both the
  derivation closure and per-component `meta`, demonstrating that #1034 and
  #1040 need no second pass.

## Assumptions

- **A-1**: Flake-based projects only. Non-flake Nix entry points (`default.nix`,
  `shell.nix`, niv, npins) are out of scope for this tier; their absence
  produces no error. **Override if you want them in v1.**
- **A-2**: The flag is the only way to enable the tier — no environment
  variable. This deliberately departs from `--helm-render`'s dual mechanism,
  because issue #1042 has just recorded that the CLI's flag surface has two
  mechanisms for several things and no rule for when each applies. Adding a
  second one here would widen the problem being investigated. **Override if you
  want parity with `--helm-render` instead.**
- **A-3**: Evaluation supersedes file-parsing on disagreement, per the operator
  decision of 2026-09-28: in Nix, evaluation is what is real.
- **A-4**: The file-parsing path is unchanged by this feature. It remains the
  default, and it remains the fallback.
- **A-5**: Operators who enable the tier accept that the resulting document
  describes their platform's resolution, not a platform-independent view of the
  project.
- **A-6**: The safety findings in US3 were measured on Determinate Nix 3.20.0
  (nix 2.34.6) on `aarch64-darwin`. The minimum `nix` version that honours
  `allow-import-from-derivation` has **not** been established and must be probed
  before any version floor enters the plan.
- **A-7**: Whether `nix` imposes any default bound on evaluation time or memory
  has **not** been established. FR-012 requires a bound; the planning phase must
  probe before choosing its default.

## Out of Scope

- Making evaluation the default. Still excluded, but on narrower grounds than
  the first draft gave. The original argument — that evaluation runs
  repository-authored expressions — is not true of what was built (see US3's
  scope correction). What remains is enough: the default would acquire an
  external runtime dependency on `nix`, make output host-dependent
  (`hinotify` differs by platform at one revision), require 300–335 MB of
  nixpkgs in the store against 16 MB of file fetches, and evaluate nixpkgs code
  waybill does not audit. Revisit if project-flake evaluation lands, which
  would restore the original, stronger objection.
- Emitting the derivation closure (issue #1034).
- Emitting vulnerability or VEX data (issue #1040).
- Changing, replacing, or deprecating the file-parsing path.
- Non-Haskell Nix ecosystems, beyond what falls out of the tier's general shape.

## Research constraint *(binds the planning phase)*

Two prior documented decisions about Nix in this project were wrong, and both
cost real defects:

- m926's research §R1 cited Constitution Principle I to avoid invoking `nix`
  entirely. The constitution was later amended (v3.0.0) and the objection did
  not survive; meanwhile the file-parsing path shipped #1033.
- m143's §R7 declared `cabal.project` a presence-only signal. It is not, and
  #1032 fixed the consequence.

Both failures share a shape: a claim about external behaviour was written into
an artifact without being probed. The planning phase for this feature MUST cite
observations already taken, and MUST NOT assert new claims about how `nix`
behaves without a probe committed alongside the spec.

**Observations available to cite** (all taken 2026-09-28 unless noted):

| observation | value |
|---|---|
| evaluation cost, 423 package names at a pinned revision | 0.5 s warm, 7.7 s cold |
| platform dependence | `hinotify` 0.4.2 on `x86_64-linux`, 0.1.8 on `aarch64-darwin`, same revision |
| flake without a `default` package output | haskell-language-server — `docs` and devShells only |
| HLS build closure composition | 1,796 derivations / 1,319 distinct names → 646 file artefacts, 54 build scaffolding, 619 candidate components |
| import-from-derivation during pure eval | builds and runs a repo-supplied `/bin/sh` command; refused by `allow-import-from-derivation false` |
| pure-eval environment access | `builtins.getEnv "HOME"` → `""` |
| store cost per nixpkgs revision | 300–335 MB, vs 16 MB for today's file fetches |
| patches vs version strings | nixpkgs `unzip`: 13 CVE-named patches, version unchanged since 2009 |

**A claim in this spec was itself wrong, and is worth recording.** The first
draft asserted that the tier evaluates repository-authored expressions. That
came from a correct measurement of `nix eval` on a flake, generalised to an
implementation that does not do it. It survived clarify, plan, tasks and
analyze, and was caught only by mutation-testing the acceptance test the claim
justified. The lesson is narrower than "measure": a measurement of what a
*tool* does is not a measurement of what *our use of it* does.

**Explicitly not yet measured** — must be probed before entering the plan:
the minimum `nix` version honouring `allow-import-from-derivation` (A-6); any
default bound `nix` places on evaluation time or memory (A-7); the wall-clock
and byte cost of a genuinely cold store on a machine that has never fetched the
pinned revision.
