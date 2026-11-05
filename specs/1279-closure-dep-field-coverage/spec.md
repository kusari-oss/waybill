# Feature Specification: `unreferenced` must mean "nothing depends on this"

**Feature Branch**: `1279-closure-dep-field-coverage`
**Created**: 2026-11-05
**Status**: Draft
**Issue**: #1346
**Input**: The nix closure role classifier reads 7 of roughly 20 nixpkgs
dependency attributes, so the `unreferenced` role conflates a derivation
nothing depends on with one whose dependency was declared through an attribute
the classifier does not read.

## Why this exists

`waybill:closure-role` has four values, and one of them currently answers two
different questions with the same word. `unreferenced` is emitted both when
nix genuinely records no dependency on a member, and when nix records one
through an attribute the classifier never reads.

Those are not the same claim. The first is a fact about the closure; the
second is a fact about waybill. A consumer cannot tell them apart, and neither
can an invariant — which is the same shape of defect as #1348, where a whole
attribute *carrier* went unread and the symptom was a large `unreferenced`
population that got written up as a finding about nix.

**Measured on the corpus closure, aarch64-darwin** (the system label matters —
see the note below the table), every dependency attribute any derivation
declares, against the seven the classifier consults:

| attribute | declared on | store paths | consulted |
|---|---|---|---|
| `nativeBuildInputs` | 1,253 | 2,540 | yes — tooling |
| `propagatedBuildInputs` | 1,253 | 630 | yes — artifact |
| `buildInputs` | 1,253 | 520 | yes — artifact |
| **`defaultNativeBuildInputs`** | 19 | **311** | **no** |
| `depsBuildBuild` | 1,253 | 179 | yes — tooling |
| **`depsTargetTargetPropagated`** | 1,253 | **18** | **no** |
| **`defaultBuildInputs`** | 19 | **12** | **no** |
| **`propagatedNativeBuildInputs`** | 1,253 | **7** | **no** |
| **`depsTargetTarget`** | 1,253 | **4** | **no** |
| **`depsBuildTarget`** | 1,253 | **1** | **no** |
| `depsBuildBuildPropagated`, `depsBuildTargetPropagated`, `depsHostHostPropagated` | 1,253 | 0 | no — never populated here |
| `depsHostHost` | 1,253 | 0 | yes — artifact, but never populated here |

So the gap is **six populated attributes carrying 353 store-path
declarations**, and it is not evenly spread: `defaultNativeBuildInputs` alone
carries 311 of them, on 19 derivations.

**These are darwin figures, the populations are not system-stable, and the
*membership* differs as well as the counts.** `defaultBuildInputs` is
populated on darwin (12 declarations) and empty on linux;
`extraNativeBuildInputs` is the reverse (3 on linux, absent from the darwin
table above). A requirement enumerated from one system's table is therefore
incomplete on the other, which is what happened to FR-012's first draft. The
x86_64-linux closure of the same project — the one the corpus goldens pin, and
therefore the one any assertion is checked against — carries **230**
unconsulted declarations across **23** derivations, and the stdenv pair
accounts for a far smaller share of them: the population of members
`unreferenced` *only* because of an unclassified attribute is **3** on linux
against **16** on darwin, a five-fold difference. Success criteria below are
therefore stated against linux. The ratio `323 of 353` quoted in the
clarifications is a darwin fact and is labelled as such.

Two consequences, and only the first is the one #1346 names:

1. **`unreferenced` is not trustworthy as a reason.** Any member reachable
   only through one of those six reads as though nothing depends on it.
2. **`propagatedNativeBuildInputs` propagates transitively.** A member
   reachable only through it is unreferenced for exactly the reason #1348's
   members were — a declaration that exists and is not read — and propagation
   means the effect is not confined to the derivation that declares it.

## Clarifications

### Session 2026-11-05

- Q: How should nixpkgs' cross-compilation triples map onto the two-way artifact/tooling split? → A: **Classified as `artifact-input`, which asserts no scope.** They name dependencies of the *target* platform — what a compiler being built hands to things it later compiles — so they are neither in the artifact nor tooling that built it. `build-tooling` was the initial lean and was rejected: milestone 1277 asserts CycloneDX `scope: "excluded"` for tooling roles, a positive claim that the member is not in the artifact, and a cross-compiler genuinely ships its target libc inside its own output. That claim can therefore be false, which is the defect class of #1342 and #1348. `artifact-input` already carries the meaning "nix references this in, and waybill cannot tell whether it ships", and m1277 deliberately asserts no scope for it. Imprecise, but it makes no false claim. A fifth role value was rejected on blast radius: it would change the C182 closed enum, every consumer of it, invariant I10's population and C203's vocabulary, for 22 store paths on this closure.
- Q: Should stdenv's `defaultNativeBuildInputs` / `defaultBuildInputs` be classified at all? → A: **No — they stay unclassified, and the distinction is made explicit instead.** #1346 is a truthfulness defect rather than a coverage one: its ask is that `unreferenced` stop meaning two things. A member named only in an attribute waybill does not classify is marked as such, which satisfies that for all 323 declarations (aarch64-darwin; the linux share is smaller) without moving the census at all. That also keeps SC-004's attribution clean — every role change then traces to a newly-consulted attribute rather than to stdenv's machinery arriving en masse — and avoids a large new population of `scope: "excluded"` assertions landing in one milestone.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - `unreferenced` means one thing (Priority: P1)

An operator reads `waybill:closure-role = "unreferenced"` and takes it to mean
nix records no dependency on this member. Today that is sometimes true and
sometimes an artifact of which attributes waybill reads, with no way to tell
which.

**Why this priority**: it is the defect #1346 describes, and it is a
truthfulness problem rather than a coverage one. A role that answers two
questions with one word is worse than a role that admits it does not know,
because the first cannot be audited.

**Independent Test**: for every member of the corpus closure carrying
`unreferenced`, confirm that no derivation names it in *any* dependency
attribute. Any member that fails that check is currently mislabelled.

**Acceptance Scenarios**:

1. **Given** a member no derivation names in any dependency attribute,
   **When** the closure is classified, **Then** it reads `unreferenced`.
2. **Given** a member named only in an attribute classification does not
   consult, **When** the closure is classified, **Then** it does **not** read
   `unreferenced` — it carries either a role derived from that attribute or an
   explicit marker that waybill saw a reference it could not classify.
3. **Given** either case, **When** the document is emitted, **Then** a
   consumer can distinguish them without consulting waybill's source.

---

### User Story 2 - a member reachable through a propagating attribute is reachable (Priority: P2)

An operator expects transitive build tooling to be reported as tooling.
`propagatedNativeBuildInputs` propagates to consumers, so a member reachable
only through it is reachable through a chain nix records and waybill drops.

**Why this priority**: it is the one unread attribute whose effect is not
local, so its population is understated by more than its own declaration
count. P2 rather than P1 because the measured population is small (7 store
paths) and the P1 story already stops the mislabelling.

**Independent Test**: a member named only in `propagatedNativeBuildInputs`
carries a tooling role, and so does a member reachable only through a chain of
them.

**Acceptance Scenarios**:

1. **Given** a derivation naming a member in `propagatedNativeBuildInputs`,
   **When** the closure is classified, **Then** that member is build tooling.
2. **Given** a member reachable only through two such hops, **When** the
   closure is classified, **Then** it is build tooling and not `unreferenced`.

---

### User Story 3 - the disclosure narrows to what is still unread (Priority: P2)

A maintainer reads C204 `waybill:nix-closure-read` to see which dependency
attributes waybill still does not consult. After this milestone that list must
shrink to exactly what remains unclassified, so the field keeps meaning
"declared and not consulted".

**Why this priority**: C204 exists because of #1348 and its value is measured
by this milestone's own baseline. If the list does not shrink, either nothing
was classified or the disclosure is lying — and both are worth catching. P2
because it is an observable consequence of US1 and US2 rather than independent
behaviour.

**Independent Test**: the `fields-unconsulted` list on the corpus target
contains no attribute this milestone classifies, and still contains any that
it deliberately leaves alone.

**Acceptance Scenarios**:

1. **Given** an attribute this milestone classifies, **When** the document is
   emitted, **Then** it does not appear in `fields-unconsulted`.
2. **Given** an attribute this milestone deliberately leaves unclassified,
   **When** the document is emitted, **Then** it still appears there, so the
   remaining gap stays visible.
3. **Given** a closure where nothing is left unconsulted, **When** the
   document is emitted, **Then** the disclosure is absent entirely.

---

### User Story 4 - an attribute nixpkgs adds later is loud (Priority: P3)

A maintainer bumps nixpkgs, which introduces a dependency attribute nobody has
seen. The run should say so rather than quietly classifying its members as
unreferenced.

**Why this priority**: this is the mechanism that would have caught #1346 and
#1348 years earlier, and C204 already provides most of it. P3 because the
first three stories are correct without it.

**Acceptance Scenarios**:

1. **Given** a closure declaring a dependency attribute the classifier does
   not know, **When** it is read, **Then** the attribute is named in the
   disclosure rather than ignored.
2. **Given** a closure declaring only known attributes, **When** it is read,
   **Then** no disclosure is emitted.

---

### Edge Cases

- **An attribute declared but empty.** `depsHostHost` is declared on all
  1,253 derivations and holds zero store paths on this closure; three more are
  the same. An empty list must credit nothing and must not appear as a gap.
- **The same member named by both an artifact and a tooling attribute.** The
  existing union rule covers this and must keep covering it as the field set
  widens.
- **A member named only in an attribute left deliberately unclassified.** This
  is the case the whole milestone turns on, and after the clarifications it is
  concretely the stdenv pair: a member named only by
  `defaultNativeBuildInputs` or `defaultBuildInputs` must be distinguishable
  from a member nothing names, and must acquire no role.
- **An attribute whose value is not a list of store paths.** Resolution
  already tolerates any JSON shape; widening the field set must not change
  that.
- **The root derivation.** Nothing in the closure depends on the thing being
  built, so it stays `unreferenced` whatever the field set. A caller treating
  that role as "drop" would drop the artifact.
- **Cross-compilation on a non-cross closure.** The target-platform triples
  are populated here only lightly (23 store paths across three attributes); a
  genuinely cross closure would populate them heavily, and this milestone must
  not assume the light case.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: `unreferenced` MUST mean that no derivation in the closure names
  the member in any dependency attribute, read or unread.
- **FR-002**: A member named only in an attribute that classification does not
  consult MUST be distinguishable, in emitted output, from a member nothing
  names.
- **FR-003**: The classifier MUST consult `propagatedNativeBuildInputs` and
  treat members reachable through it, including transitively, as build
  tooling.
- **FR-004**: The set of attributes the classifier consults MUST be derivable
  from one declaration, so the disclosure of what is *not* consulted cannot
  drift from what is.
- **FR-005**: An attribute declared with an empty value MUST credit no member
  and MUST NOT be reported as an unconsulted gap.
- **FR-006**: The union rule for a member named by both artifact-side and
  tooling-side attributes MUST be unchanged.
- **FR-007**: An attribute the classifier does not recognise MUST be reported,
  never silently ignored.
- **FR-008**: The corpus goldens MUST be refreshed in this milestone through
  the existing workflow dispatch, never generated locally; and the refreshed
  goldens MUST then be verified by a **compare-mode** run whose own log is
  checked to confirm it compared rather than rewrote. A regen run cannot
  verify its own output, and a local run of the corpus target reports `ok` in
  0.00s having skipped — so neither establishes anything on its own.
- **FR-009**: The role census MUST be recorded before and after, per
  attribute, so the change is attributable to this milestone rather than to a
  nixpkgs or nix movement.
- **FR-010**: The classifier MUST consult `depsTargetTarget` and
  `depsTargetTargetPropagated` and treat members reachable through them as
  `artifact-input`, which asserts no scope. It MUST NOT classify them as
  tooling: that would assert the member is absent from the artifact, and a
  cross-compiler ships its target-platform dependencies inside its own
  output, so the assertion can be false.
- **FR-011**: The classifier MUST consult `depsBuildTarget` and treat members
  reachable through it as build tooling — it names what runs on the build
  machine to produce target-platform output, which is tooling by the same
  reading as `depsBuildBuild`.
- **FR-012**: The **stdenv-injected** attributes MUST remain unclassified:
  `defaultNativeBuildInputs`, `defaultBuildInputs` and
  `extraNativeBuildInputs`. A member named only by them MUST carry the FR-002
  marker, so the declarations they hold stop being reported as `unreferenced`
  without their members acquiring a role. The role census MUST NOT move on
  their account.
  **The third was missed when this requirement was first written.** It
  enumerated two, measured on the darwin closure, where
  `extraNativeBuildInputs` is unpopulated. On the x86_64-linux closure — the
  one the goldens pin and every criterion below is stated against — it carries
  3 declarations while `defaultBuildInputs` carries **none**. The two systems
  have different populated sets, so a requirement enumerated from one is
  incomplete on the other; all three are named so the set is complete on
  both.
- **FR-013**: The marker of FR-002 MUST name the attribute, or attributes,
  through which the member was referenced. "Referenced by something waybill
  does not classify" without saying what is a smaller version of the same
  defect: it tells a consumer that a gap exists and not where.

### Key Entities

- **Dependency attribute**: a named attribute of a derivation whose value is a
  list of store paths. nixpkgs names them by convention — `*Inputs`, and
  `deps<Host><Target>` with an optional `Propagated` suffix. Fourteen are
  declared on this closure; six are populated and unconsulted.
- **Closure role**: how nix references a member. Today four values, one of
  which is overloaded. Whether this milestone adds a fifth is the first open
  question.
- **Consulted set**: the attributes classification reads. Its complement is
  what the disclosure reports, so the two must come from one source.
- **Propagation**: the property that makes `propagatedNativeBuildInputs`
  different in kind from the others — its effect reaches consumers of the
  declaring derivation, so an unread propagating attribute understates
  reachability by more than its own count.

## Success Criteria *(mandatory)*

Figures are **measured** on the corpus closure, 2026-11-05, and are dated
rather than eternal; the probe is committed under `measurements/`.

- **SC-001**: No member carrying `unreferenced` is named by any derivation in
  any dependency attribute. **Measured today: violated** — six populated
  attributes are unread, carrying 230 store-path declarations on
  x86_64-linux (353 on aarch64-darwin).
- **SC-002**: A member named only in an unconsulted attribute is
  distinguishable from a member nothing names, by reading the document alone.
- **SC-003**: Every member reachable through `propagatedNativeBuildInputs`,
  directly or transitively, carries a tooling role.
- **SC-004**: The role census is recorded per attribute before and after, and
  the difference **accounts for every member whose role changes**: each such
  member is attributed to the specific newly-consulted attribute that reaches
  it. A member whose role changes for a reason not traceable to one of those
  four is a defect in the measurement, not a finding about nix.
- **SC-005**: C204's `fields-unconsulted` contains no attribute this milestone
  consults, and still contains every one it deliberately does not.
- **SC-006**: An empty attribute credits nothing and is reported as no gap.
- **SC-007**: Removing a newly-consulted attribute from the declaration makes
  a test fail. A widened field set that no test is sensitive to is not
  evidence of anything.
- **SC-008**: Every corpus target that does not scan a closure produces
  byte-identical goldens.
- **SC-009**: The consulted set and the disclosure's complement are proven to
  agree by a test, not by inspection.
- **SC-010**: No member reachable through `depsTargetTarget` or
  `depsTargetTargetPropagated` carries a native marker asserting it is absent
  from the artifact. **Measured population on the pinned x86_64-linux closure:
  five — `numactl`, `gmp-with-cxx`, `libffi`, `elfutils`, `ncurses`.** All five
  are asserted, not just the one that changes today: four already carry
  `artifact-input` or `both` with no scope, so a criterion that checked only
  the changing one would pass unchanged if the triples were later reclassified
  as tooling and all five gained an exclusion.
- **SC-011**: The role census moves by exactly the members newly reachable
  through the four attributes this milestone consults, and by no others.
  `defaultNativeBuildInputs` and `defaultBuildInputs` contribute **zero**
  census movement; a run in which they contribute any is a defect.
- **SC-012**: On the pinned corpus target (x86_64-linux), the census moves
  270 / 118 / 42 / 1,319 → **268 / 121 / 45 / 1,315** across
  artifact-input / build-tooling / both / unreferenced. Seven derivations
  change role: 3 `unreferenced`→`build-tooling`, 3 `artifact-input`→`both`,
  1 `unreferenced`→`artifact-input`. **`artifact-input` falls by two**, which
  is not a typo — three of its members gain a tooling reference and become
  `both` while only one arrives.
- **SC-013**: `numactl` loses its `scope: "excluded"`. It is the one component
  in the committed golden whose exclusion rests on a role that an unread
  attribute made wrong, and no component gains an exclusion. Named rather than
  counted, because a criterion that says "the false exclusions are retracted"
  is how milestone 1278 shipped a count of nine for a population of six.

## Assumptions

- **The role enum does not change.** The four C182 values stay as they are,
  and the cross-compilation triples are folded into `artifact-input` rather
  than earning a fifth. The cost of that decision is semantic imprecision —
  `artifact-input` now covers two distinguishable things — and it was accepted
  because the alternative asserts a scope that can be false. If a genuinely
  cross closure later makes the imprecision expensive, a fifth value is the
  follow-up, and it should be taken with its own milestone rather than
  smuggled in here.
- **The census does not move on stdenv's account.** `defaultNativeBuildInputs`
  and `defaultBuildInputs` hold 323 of the 353 unread declarations and are
  deliberately left unclassified, so this milestone's census movement is
  attributable to the four attributes it does consult. The 323 are addressed
  by being *disclosed*, not by being classified.
- **The measured figures are from one closure on one system.** The corpus
  target is Haskell and non-cross; a cross-compilation closure would populate
  the target triples far more heavily. Nothing here should assume the light
  case, which is why FR-003 is scoped to the propagating attribute rather
  than to "the six".
- **This milestone does not revisit the attribute carriers.** Reading
  `structuredAttrs` as well as `env` landed in #1348 and is unchanged.
- **`unreferenced`-without-a-patch members are still dropped from emission.**
  That rule is milestone 1035's and is out of scope; this milestone changes
  which members are in that role, not what happens to them.
- **Zero new dependencies.** Everything needed is already parsed.
