# Feature Specification: the subject of a Nix scan is the package it built, identified by the CPE nixpkgs declares

**Feature Branch**: `1377-nix-subject-identity`
**Created**: 2026-11-09
**Status**: Draft
**Issues**: #1398 — *A --nix-closure scan of a flake names its subject `filesystem-scan 0.0.0`, and the package it built carries no CPE*; #1394 — *The nix closure tier ignores the CPE nixpkgs declares in `meta.identifiers`*

## Context

This milestone is the first step of the goal set on 2026-11-09: a Nix SBOM that a vulnerability-management user's scanner does more with than sbomnix's. A spike that day measured **scanner outcomes** rather than component counts (`measurements/scanner-outcome-spike.md`). Two of its findings make this milestone.

**The subject is not the package.** A `--nix-closure` scan of a flake whose evaluated package has no manifest main-module reaches the last rung of the root-selection ladder, so its subject is a synthetic placeholder: `pkg:generic/<directory>@0.0.0`, named `filesystem-scan` under `--path .` (#1401), carrying `cpe:2.3:a:waybill:filesystem-scan:0.0.0`. The package the flake evaluates to appears only as an ordinary component, with no CPE.

**nixpkgs declares CPEs and waybill does not read them.** `meta.identifiers.cpe` is set for many packages: openssh, curl, openssl and zlib at the measured revision. Where nixpkgs has a product but no vendor (python313, httpie) there is no `cpe`.

| Measured on nixpkgs `a799d3e3`, aarch64-darwin, grype 0.120.0 | sbomnix 1.9.0 | waybill `main@72b455e2` |
| --- | --- | --- |
| subject of the openssh wrapper flake | `openssh` `10.3p1`, `openbsd:openssh:10.3p1` | `filesystem-scan` `0.0.0`, `waybill:filesystem-scan:0.0.0` |
| grype findings on `openssh 10.3p1` | 28 | **0** |
| nixpkgs-declared CPEs in openssh's buildtime closure | 16 distinct name+version pairs, 17 derivations (two `perl` 5.42.0), plus the subject | 14 emitted identically, gnugrep missing, bash encoded differently |

**The bash disagreement is decided by measurement, not preference** (`measurements/bash-cpe-encoding/`). The same bash 5.0 patch 11 component, encoded four ways:

| encoding | grype findings |
| --- | --- |
| waybill's curated form, version `5.0p11` | **0** — misses CVE-2019-18276 |
| nixpkgs' form, version `5.0` update `11` | 1 |
| NVD's form, version `5.0` update `patch11` | 1 |
| version `5.0` alone | 1 |

nixpkgs' declared CPE is not always NVD's convention. nixpkgs writes openssh as version `10.3p1`, where NVD uses update `p1`. On openssh 9.3 that form finds 40 vulnerabilities to NVD's 41; on 10.3 both find 28. In every disagreement or absence observed, though, nixpkgs' CPE was never worse than waybill's curated one: bash 1 vs 0, openssh 28 vs none.

**Precedent.** #1170 / #1177 made an explicitly named image the subject above any main-module heuristic, because naming `--image <ref>` names the subject. An evaluated Nix installable is the same case: the operator named what to build. The closure already records the attribute it evaluated and the full derivation graph, and milestone 1281 already evaluates closure members' `meta` for licenses.

**Already the right component:** for `moat`, the Cabal main-module and the closure's `moat-0.1` derivation are one component, `pkg:hackage/moat@0.1` (milestone 1275), and it is the subject today, chosen by the repo-root main-module rung and carrying the placeholder `cpe:2.3:a:waybill:moat:0.1`. Its identity must not change; why it was chosen, and its placeholder CPE, do.

## Clarifications

### Session 2026-11-09

- Q: SC-008 expects the `nix-closure-moat` corpus golden's subject to become `pkg:hackage/moat@0.1`, but the harness passes `--root-name` for every target, so the override wins. Rewrite SC-008? → A: Yes. The moat-shaped subject is proven by the `closure_subject` integration test and the override-free probe scan; the golden's subject stays the override.
- Q: The closure root is now always emitted, but the metadata-coverage population (C184 `licenses.wanted`, later `identifiers.wanted`) counts it only when it would be emitted anyway. Count the root? → A: Yes. The coverage population is every member the lookup was wanted for, the evaluated package included.

- Q: When a Nix subject has neither a nixpkgs-declared nor a curated CPE, should it carry no CPE or the `cpe:2.3:a:waybill:<name>:<version>` placeholder? → A: **No CPE.** Checked against CISA's 2026 Minimum Elements (final, co-sealed by CISA, NSA, BSI, ANSSI, NCSC-NL and others) and ENISA's December 2025 SBOM landscape analysis (`measurements/identifier-requirements.md`). Both require at least one component identifier and name CPE as one example among several, and the subject keeps its PURL. CISA defines an identifier as something that identifies the component or serves as a look-up key for a database, which a `waybill`-vendor CPE does not do.
- Q: Which CPE wins when nixpkgs and the curated table disagree? → A: nixpkgs-declared, settled by measurement rather than asked: waybill's curated bash encoding misses CVE-2019-18276 where nixpkgs' finds it (`measurements/bash-cpe-encoding/`).
- Q: Which component is the subject when a flake has both a manifest main-module and an evaluated package? → A: settled by evidence and precedent rather than asked: for moat they are already one component; where they differ, the evaluated package wins and the main-module is recorded as outranked, as #1170 does for an image.
- Q: When nixpkgs and the curated table disagree, what does the document keep? → A: The nixpkgs-declared CPE is the component's single CPE identifier in every format. The curated value it overrode is not emitted as a second identifier; it is kept in that CPE's provenance record, so the disagreement stays auditable and the three formats stay in parity.
- Q: Does the new rule re-label a subject that is already the right component (moat)? → A: Yes. The rule applies whenever a closure resolves. moat's subject identity (`pkg:hackage/moat@0.1`) is unchanged, but its recorded root-selection heuristic becomes the new rule and its `waybill:moat:0.1` placeholder CPE is dropped (FR-012). The `nix-closure-moat` corpus goldens change accordingly.
- Q: Where is the count of closure members the `meta` evaluation could not reach reported? → A: In the document, at document level: the existing closure summary gains that count. No per-component marker, and not only in the operator log.
- Q: When a closure member merges into a component another reader already emitted, whose CPE wins? → A: Rank by source: an operator `--supplement-cdx` assertion, then nixpkgs-declared, then curated, then none (FR-013). Two CPE sources are already on components when the closure merges: the curated table (in `read_all`) and the operator supplement. So "the existing reader wins" would let the curated CPE beat nixpkgs on merged members, contradicting FR-007's measured precedence.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A scanner attributes the built package's vulnerabilities to the package (Priority: P1)

A vulnerability-management user scans a Nix package with `--nix-closure` and feeds the document to a CPE-matching scanner. The package they built is the document's subject, under its own name and version, with the identity a vulnerability feed keys on. Vulnerabilities in that package reach the scanner's report.

**Why this priority**: today the scanned package is the one thing in the document a scanner cannot connect to any vulnerability: 0 of 28 findings on openssh.

**Independent Test**: scan the openssh wrapper flake (`measurements/wrapper-flakes/openssh`) and inspect the subject in each output format, then run grype over the CycloneDX document.

**Acceptance Scenarios**:

1. **Given** a flake whose evaluated package is openssh 10.3p1, **When** it is scanned with `--nix-closure`, **Then** the subject is `openssh` `10.3p1` in CycloneDX, SPDX 2.3 and SPDX 3, with the identity the closure tier assigns that derivation.
2. **Given** the same document, **When** grype scans it, **Then** the findings reported against openssh 10.3p1 are at least the 28 grype reports for the same package from sbomnix's document.
3. **Given** a flake whose evaluated package is also the manifest main-module (moat), **When** it is scanned, **Then** the subject is that one component, unchanged in name, version and PURL, no second component describes the same package, and the recorded root-selection heuristic is the evaluated-package rule.
4. **Given** an operator override (`--root-name` / `--root-purl`), **When** a `--nix-closure` scan runs, **Then** the override still decides the subject.

---

### User Story 2 - Closure components carry the CPE nixpkgs declares (Priority: P1)

The same user's scanner matches closure members, the subject included, by the CPE nixpkgs' maintainers declared for them wherever one exists. waybill's curated table supplies a CPE only where nixpkgs declares none.

**Why this priority**: nixpkgs' declared CPE is authoritative where set, and the one disagreement measured (bash) is a false negative in waybill's current form.

**Independent Test**: scan the openssh wrapper flake and compare every closure component's CPE with that derivation's `meta.identifiers.cpe`.

**Acceptance Scenarios**:

1. **Given** a closure member whose derivation declares `meta.identifiers.cpe`, **When** the closure is scanned, **Then** that component carries exactly the declared CPE.
2. **Given** bash, where nixpkgs declares version `5.3` update `9` and the curated table produces version `5.3p9`, **When** scanned, **Then** the component carries the nixpkgs form.
3. **Given** gnugrep, which nixpkgs identifies and waybill currently leaves without a CPE, **When** scanned, **Then** it carries `gnu:grep`.
4. **Given** a member nixpkgs does not identify with a vendor (python3, gh), **When** scanned, **Then** the curated table is consulted as it is today, and nothing is synthesized from `cpeParts` without a vendor.
5. **Given** any component that carries a CPE today, **When** scanned after this change, **Then** it still carries one.

---

### User Story 3 - A reader can tell where a component's CPE came from (Priority: P2)

A reader auditing a match, or a consumer weighting identities, can see per component whether its CPE was declared by nixpkgs or supplied by waybill's curated table.

**Why this priority**: two sources now feed one field, and they can disagree (bash). A consumer who cannot tell them apart cannot audit the choice.

**Independent Test**: for every CPE-bearing closure component in the openssh document, read its recorded source and check it against that derivation's `meta.identifiers`.

**Acceptance Scenarios**:

1. **Given** a component whose CPE came from nixpkgs, **When** a reader inspects it, **Then** the source is recorded as nixpkgs-declared, in every output format that can carry it.
2. **Given** a component whose CPE came from the curated table, **When** inspected, **Then** the source is recorded as curated.
3. **Given** bash, where the curated table proposed `gnu:bash:5.3p9` and nixpkgs declares `gnu:bash:5.3:9`, **When** inspected, **Then** the component carries one CPE, the nixpkgs-declared one, and its provenance records the curated value it overrode.

---

### Edge Cases

- **The evaluated package and a manifest main-module are different components.** The evaluated package is the subject, and the main-module is recorded as outranked, as #1170 does for an image.
- **The closure degrades** (no evaluable attribute, import-from-derivation refused #1283, budget exceeded). There is no evaluated package to name, so subject selection falls back to the existing ladder, byte-identical to today.
- **The attribute is a NixOS configuration** (`nixosConfigurations.<host>`). The subject is the system's top-level derivation, by the same rule.
- **`cpeParts` without a vendor.** Not a declared CPE. Nothing is synthesized from it.
- **A member the `meta` evaluation cannot reach.** Milestone 1281 reaches members by attribute, not all of them. Such a member keeps today's behaviour (the curated table), and the gap is counted in the document's closure summary, not hidden (FR-009).
- **The declared CPE's version differs from the derivation's version.** The declared value is used as declared; a mismatch is not silently corrected.
- **A non-Nix scan, or a Nix scan without `--nix-closure`.** Unaffected.
- **No CPE from either source** (gh: nixpkgs has a product but no vendor, and the curated table has no row; measured 2026-11-09). The subject carries its PURL and no CPE (FR-012).
- **A closure member merges into another reader's component.** If that component carries the curated CPE or none, the declared CPE replaces it. If it carries an operator-asserted or artifact-derived CPE, that CPE is kept and no nixpkgs provenance is attached (FR-013).
- **`--no-root-purl` with no CPE.** The subject would then carry no identifier at all, below CISA's "at least one". That is an explicit operator choice and today's behaviour, not something this milestone introduces.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: When a `--nix-closure` scan resolves a closure, the derivation the evaluated attribute produced MUST be the document's subject, above every main-module heuristic and below only an explicit operator override. The rule applies whenever a closure resolves, including when a main-module rung would have chosen the same component.
- **FR-002**: The subject MUST carry that derivation's name and version and the identity the closure tier assigns it, identically in CycloneDX, SPDX 2.3 and SPDX 3.
- **FR-003**: When the evaluated package and a manifest main-module are one component, the subject MUST be that component, with no duplicate. When they are different components, the outranked main-module MUST be recorded the same way #1170 records main-modules an image outranked.
- **FR-004**: Whenever the document records a root-selection heuristic, it MUST name this rule distinctly from the existing ones (`nix-evaluated-package`), so a reader can tell why the subject was chosen. The existing emission gate applies unchanged: the heuristic is recorded only when the selection outranked another candidate, as for every rung (research R3).
- **FR-005**: For each closure member, subject included, whose derivation declares `meta.identifiers.cpe`, the component MUST carry exactly that CPE.
- **FR-006**: Where nixpkgs declares no CPE, the curated table MUST apply as it does today. A CPE MUST NOT be synthesized from `cpeParts` that lack a vendor.
- **FR-007**: The conflict case of FR-005: where nixpkgs and the curated table both produce a CPE and they differ, the nixpkgs-declared CPE MUST win and MUST be the component's only CPE identifier, in every format. The curated CPE it overrode MUST NOT be emitted as a second identifier. Basis: `measurements/bash-cpe-encoding/`.
- **FR-008**: Each CPE-bearing closure component MUST record which source supplied its CPE and, where a nixpkgs-declared CPE overrode a different curated one, the curated value it overrode. *Principle V audit (research R5)*: all three formats have a native carrier for an identifier's provenance, so the source MUST be recorded there: CycloneDX `evidence.identity[]` (`field: "cpe"`), SPDX 2.3 `externalRefs[].comment`, SPDX 3 `ExternalIdentifier.comment`. No format has a field for the join basis or for an overridden identifier, so those two, and only those, MAY go in a `waybill:cpe-provenance` annotation documented with a justification clause.
- **FR-009**: The document's existing closure summary MUST state how many closure members the `meta` evaluation could not reach, so that a missing declared CPE reads as a coverage gap rather than an absence. Unreached members MUST NOT be individually marked. *Principle V audit (research R6)*: no format has a native construct for metadata-evaluation coverage (CycloneDX `compositions` concern component and dependency completeness), so the count extends the existing `waybill:nix-closure` annotation; no new field is introduced.
- **FR-010**: Reading declared CPEs MUST NOT add network access or evaluation beyond what `--nix-closure` already performs, and MUST stay behind that opt-in.
- **FR-011**: Output for scans that do not resolve a closure MUST be byte-identical to today's.
- **FR-012**: When a Nix subject has neither a nixpkgs-declared nor a curated CPE, it MUST carry no CPE. It MUST NOT carry a `waybill`-vendor placeholder. It keeps its PURL, which satisfies CISA's and ENISA's "at least one identifier" (`measurements/identifier-requirements.md`).
- **FR-013**: When a closure member merges into a component another reader already emitted, its nixpkgs-declared CPE MUST replace that component's CPE only when the existing CPE is exactly what the curated table produces for that component, or the component has none. Any other existing CPE, such as an operator `--supplement-cdx` assertion or one a reader derived from the artifact, MUST be kept, and the closure's CPE provenance MUST NOT be attached to it. The recorded provenance MUST always describe the CPE actually emitted.

### Key Entities

- **Evaluated package**: the derivation the scanned attribute produces. It is the root of the closure's derivation graph and becomes the subject.
- **Declared CPE**: `meta.identifiers.cpe` for a closure member, as nixpkgs states it. Absent when nixpkgs records no vendor.
- **Curated CPE**: what waybill's dictionary-verified table supplies today. The fallback.
- **CPE source**: per component, which of the two supplied its CPE, and, when nixpkgs overrode a different curated CPE, the curated value it overrode.

## Success Criteria *(mandatory)*

Every criterion is checked by re-running the committed spike probes (`measurements/gen-sboms.sh`, `scan.sh`, `analyze.py`, `precision.py`) on the same targets, revision and scanner version.

### Measurable Outcomes

- **SC-001**: On the openssh wrapper flake, the subject is `openssh` `10.3p1` in all three formats. Baseline: `filesystem-scan` `0.0.0`.
- **SC-002**: grype reports at least 28 findings on openssh 10.3p1 from waybill's document. Baseline: 0. Reference: sbomnix 28.
- **SC-003**: In openssh's buildtime closure, all 16 distinct name+version pairs with a nixpkgs-declared CPE (17 derivations: two `perl` 5.42.0) carry it. That is 14 unchanged, gnugrep gaining `gnu:grep`, and bash carrying the nixpkgs encoding, with 0 components losing a CPE they had.
- **SC-004**: A bash component encoded as the scan now emits it is matched by grype to CVE-2019-18276 on the version-5.0-patch-11 control (`measurements/bash-cpe-encoding/`), where waybill's current encoding matches nothing.
- **SC-005**: 100% of CPE-bearing closure components in the openssh document record their CPE source.
- **SC-006**: Across all six spike targets, no grype finding present today disappears except where a CPE's source changed. Distinct already-patched false positives (today: 6) do not increase.
- **SC-007**: Every committed golden produced without `--nix-closure` is byte-identical.
- **SC-008**: For `moat` scanned without an operator override (the probe in `measurements/gen-sboms.sh`), the subject is `pkg:hackage/moat@0.1`, its recorded heuristic is the evaluated-package rule, and it carries no `waybill`-vendor CPE. The `nix-closure-moat` corpus golden is a different case: the harness passes `--root-name <target>` for every target (`waybill-cli/tests/corpus_harness_195/harness.rs:248`), the override wins (FR-001), and that golden's subject stays `nix-closure-moat`. Its diff is limited to closure-member CPEs and their provenance and to C184; any other change is investigated, not accepted.
- **SC-009**: The openssh document's closure summary states how many closure members the `meta` evaluation could not reach, and that count equals an independent count taken by the probe (members of the same closure for which a separate evaluation returns no `meta`).

## Out of scope

- **`waybill`-vendor placeholder CPEs in general.** 76 committed golden files carry one: 24 synthetic goldens (8 per format), all three formats of 17 public-corpus targets, and one alias-binding fixture. A parity extractor compensates for its shape. Whether synthetic or main-module subjects carry one at all is the cross-ecosystem question of #1402. FR-012 decides only the Nix subject's case.
- **Normalising declared CPEs to NVD's per-vendor convention** (openssh `10.3p1` vs NVD `10.3:p1`, measured at 40 vs 41 findings on 9.3). nixpkgs' form is used as declared.
- **A curated CPE for python3** (#1395): nixpkgs cannot supply one.
- **`--path .` naming the placeholder `filesystem-scan`** (#1401).
- **The contents of built binaries** (#1396), **runtime-closure scope** (#1034), and **store-path or flakeref targets** (#431).

## Assumptions

- The closure's derivation graph identifies the evaluated attribute's derivation unambiguously. The graph has a single top derivation per evaluated attribute.
- The `meta` evaluation milestone 1281 performs can return `identifiers` alongside licenses within the same evaluation. *Verified 2026-11-09 (`measurements/meta-eval-cost/`): +0.02 s median on a 0.8 s call, license coverage unchanged, and 17 of 17 declared-CPE members reached by drvPath plus name+version.*
- grype is a representative CPE matcher for these purposes. Kusari's matcher was proxied by grype's CPE path in the spike.
- The measurements hold for one nixpkgs revision (`a799d3e3`) and one system (aarch64-darwin). Declared CPEs are revision-dependent; the rule is not.

## Evidence

- `measurements/scanner-outcome-spike.md`: the 2026-11-09 spike, its method and the three probe errors caught on the way.
- `measurements/bash-cpe-encoding/`: the four bash encodings and four openssh encodings with grype's result for each, plus NVD's dictionary convention (bash 5.0 and 4.4 encode patch levels as update `patchN`; openssh as update `pN`; bash 5.3 had no dictionary entries on 2026-11-09).
- `measurements/wrapper-flakes/`: the pinned targets.
- `measurements/issue-bodies/`: the six issues filed from the spike.
- `measurements/identifier-requirements.md`: the CISA 2026 Minimum Elements and ENISA December 2025 text on component identifiers, quoted with page numbers, behind FR-012.
