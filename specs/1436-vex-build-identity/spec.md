# Feature Specification: Nix closure VEX that scanners honour, without overclaiming

**Feature Branch**: `1436-vex-build-identity`
**Created**: 2026-11-12
**Status**: Draft
**Input**: User description (agreed 2026-11-10 as "option A"; restated 2026-11-12): "1436 Nix closure VEX that scanners honour, without overclaiming. Closes #1436 and #1421 (fuller option). Measured 2026-11-10 (grype 0.120.0, Trivy 0.71.1, six targets; probes in measurements/): grype ignores the document subject for SBOM input, so a VEX statement suppresses a finding only when its product is the component's own PURL; today's m1035 shape (product = the build, component as subcomponent) suppresses 0 of 102 patched findings. Grype and Trivy break ties between matching statements in opposite directions, so an affected+not_affected pair on one PURL works on at most one of them. Decision (option A): give each patched pkg:generic closure component a build-identity PURL qualifier (candidate: its Nix output hash) and emit one not_affected per (component, CVE) on that qualified PURL, keeping the evidence grade in impact_statement; the version-level claim stays in pedigree.patches[].resolves. Measured on one CVE: 6/6 suppressed, 0/6 leakage onto an unqualified unzip 6.0, no other finding count changed. This ends m1035's "two statements, never one" rule and moves goldens. Open questions: the qualifier key and value, whether registry PURLs (pypi, hackage) stay unqualified, how CycloneDX-embedded vulnerabilities[] and SPDX VEX follow, and whether to report the SBOM-subject limitation to grype upstream (needs approval). Constraints: zero new Cargo deps, no new network, success judged by grype suppressing all patched findings across the six targets with zero leakage onto unpatched components."

Figures are from `measurements/README.md` (2026-11-10) unless marked otherwise.

## Background

nixpkgs fixes many CVEs by **backporting** a patch into an older release: `unzip 6.0` in nixpkgs is not the `unzip 6.0` an advisory describes. waybill reads those patches from the closure (m1035). It records which CVEs each patch resolves in `pedigree.patches[].resolves`, with an evidence grade, and emits VEX: an OpenVEX sidecar, and CycloneDX `vulnerabilities[]` (m1068).

Today that VEX is shaped so that no scanner can use it:

- **Two statements per CVE (m1035).**
  - `affected`, on the component's PURL: the version as published is vulnerable.
  - `not_affected`, whose product is the *document subject* (the build), with the component as a subcomponent: this build is not affected.
- **grype never consults the document subject for SBOM input.** A statement suppresses a finding only when its *product* is the scanned package's own PURL. A subcomponent is never consulted.
  - With the subject fixed (#1436 part 1, PR #1437), today's shape suppresses **0 of 102** patched findings across six targets (curl, openssh, httpie, gh, ripgrep, moat), counted with #1435's second unzip CPE in place.
- **A pair on one PURL works for at most one scanner.** grype and Trivy break ties between matching statements in opposite directions. With equal timestamps, grype honours the **last** matching statement in document order and Trivy the **first**. So any `affected` + `not_affected` pair that both match the scanned PURL is honoured by grype or by Trivy, never both.
- **CycloneDX contradicts itself (#1421).** `vulnerabilities[]` marks the patched component `exploitable` while the same document says it was patched. On main today, `nix-closure-moat` carries 13 `exploitable` and 13 `not_affected` entries. A consumer that reads `vulnerabilities[]` (Dependency-Track, Kusari's tools) sees patched components flagged exploitable.

**What does work, measured:**
- Give the patched component a PURL that names *this build* (a qualifier).
- Emit a single `not_affected` statement with that qualified PURL as its product.
- grype suppressed the finding on 6 of 6 targets. On 0 of 6 did it leak onto an unpatched, unqualified `unzip 6.0`.
- Adding the qualifier changed no other finding count.

This feature makes that the closure tier's VEX shape. It ends m1035's "two statements, never one" rule. The version-level claim ("the published version is affected") is not lost: it stays in `pedigree.patches[].resolves`, the native construct for it.

## Clarifications

### Session 2026-11-12

- Q: What status should the per-build statement carry? → A: `fixed` in OpenVEX, which is `resolved_with_pedigree` in CycloneDX (FR-002, FR-005). Research must confirm grype honours `fixed` on a qualified product. `not_affected` with a justification is the fallback.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - My scanner stops reporting CVEs nixpkgs already patched (Priority: P1)

A user scans a Nix flake with `--nix-closure` and passes waybill's VEX to grype (`grype sbom:app.cdx.json --vex app.openvex.json`). Every finding for a CVE that a backported patch in this build resolves disappears. No finding on any other component changes.

**Why this priority**: it is the point of the feature. Today waybill holds the evidence that clears these findings, and no scanner can use it. 102 patched findings were measured across six targets, and every one still shows.

**Independent Test**: scan each of the six measurement targets, run grype with and without the VEX, and compare finding sets. The removed findings must be exactly the patched (component, CVE) pairs.

**Acceptance Scenarios**:

1. **Given** a closure where `unzip 6.0` carries a backport resolving CVE-2014-8139, **When** grype scans the document with waybill's VEX, **Then** it does not report CVE-2014-8139 on that unzip.
2. **Given** the same VEX, **When** grype scans a document holding an *unpatched* `unzip 6.0` (another build, so a different qualifier or none), **Then** CVE-2014-8139 is still reported there.
3. **Given** any scan, **When** findings with and without the VEX are compared, **Then** the only findings that disappear are patched (component, CVE) pairs.

---

### User Story 2 - The document never says a patched component is exploitable (Priority: P1)

A user loads the CycloneDX document into a tool that reads `vulnerabilities[]`. For each backported CVE it sees one entry on the patched component, saying the build resolved it and citing the patch as the evidence. It does not see an `exploitable` entry on a component the same document says was patched.

**Why this priority**: today the document contradicts itself 13 times on moat alone. A self-contradicting document is worse than no VEX: a consumer has to choose which half to believe.

**Independent Test**: on every closure golden, count `vulnerabilities[]` entries whose `affects` names a patched component with state `exploitable`. There must be none. Each backported (component, CVE) pair must have exactly one entry.

**Acceptance Scenarios**:

1. **Given** a backported CVE on a component, **When** the CycloneDX document is read, **Then** there is exactly one `vulnerabilities[]` entry for that (component, CVE). Its `affects` names the component, and its state says the build resolved it.
2. **Given** the same CVE, **When** a reader asks whether the *published* version is affected, **Then** the answer is in that component's `pedigree.patches[].resolves`, as it is today.
3. **Given** the OpenVEX sidecar and the CycloneDX entries, **When** they are compared, **Then** they make the same claims about the same (component, CVE) pairs. They are one source in two carriers, as m1068 requires.

---

### User Story 3 - A consumer can tell what a qualified PURL means (Priority: P2)

A user sees a qualifier on a patched component's PURL and wants to know what it is, why it is there, and that it does not change which advisories match the component.

**Why this priority**: the qualifier changes a component's identity, which every consumer that matches on PURL can observe. It has to be explained, stable, and harmless to matching.

**Independent Test**: read the user guide and the catalog row. Then confirm that the qualifier is identical across repeated scans of the same build, and that scanners report the same findings on the qualified document as on the unqualified one when no VEX is supplied.

**Acceptance Scenarios**:

1. **Given** two scans of the same build, **When** their documents are compared, **Then** each patched component's qualifier is identical.
2. **Given** a qualified document and no VEX, **When** grype and osv-scanner scan it, **Then** their findings equal those on the unqualified document.
3. **Given** the docs, **When** a reader looks up the qualifier, **Then** they find what it identifies, why it exists (scanners match VEX on the product PURL), and that it applies only to components with backport evidence.

### Edge Cases

- **A component patched for one CVE but not another.** Only the resolved CVEs get a statement. Every other advisory still matches, because version matching ignores the qualifier (Acceptance US3-2).
- **The same name and version, built twice** (two outputs, or a bootstrap build). Each build has its own identity, so a statement for one never covers the other (Acceptance US1-2). This is the leakage the success criteria forbid.
- **A registry PURL that carries backports** (`pkg:hackage`: 3 of 66 patched components on moat; `pkg:pypi` on httpie). The same rule applies, provided research shows the qualifier changes no finding for the scanners that match those types (osv-scanner, grype). If it does, the plan must say so, and those components keep a shape that makes no false claim.
- **A filename-derived patch-to-CVE association** (m1035's lower evidence grade). The statement still carries the grade in its impact statement, as today. The grade is never dropped by the reshaping.
- **A component dropped from this document** (root override, `--tier`, `--split`). C193 counts the claims this document cannot carry, as today, and the sidecar still names them.
- **`--split`.** Each projected document carries the qualified PURLs of the components it holds. A statement and its product stay in the same document.
- **Consumers that match PURLs exactly.** A consumer that ignores qualifiers sees the same component as before. One that compares PURLs exactly sees a new identity for patched components only. The golden diff must show that nothing else changed.
- **Out of scope:** the Windows-only advisory class that #966 surfaced (HSEC-2024-0003 on `process`). That is a platform-applicability claim, not a backport, and needs its own evidence source.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Every closure component with backport evidence (at least one `pedigree.patches[].resolves` entry) MUST carry a PURL qualifier that identifies the exact build. It MUST be deterministic, meaning the same build always gives the same value. It MUST leave the PURL's type, namespace, name and version unchanged. Components without backport evidence MUST keep their PURL byte-identical.
- **FR-002**: For each (patched component, resolved CVE) pair, the OpenVEX sidecar MUST carry exactly one statement whose product is the component's qualified PURL, with no subcomponent. Its status MUST be `fixed`: the build's backported patch remediated the CVE (clarified 2026-11-12, option A).
  - On an unqualified product, grype honoured `fixed` on 6 of 6 targets (`b_fixed`). The research phase MUST measure it on a **qualified** product, which was measured only with `not_affected` (`q_lone`, 6/6).
  - If grype does not honour `fixed` there, the fallback is `not_affected` with the closest justification. The measurement and the reason MUST be recorded, and the spec amended.
- **FR-003**: No statement MAY name a patched component's PURL with any other status for the same CVE, whether qualified or unqualified. The version-level claim MUST NOT be emitted as a VEX statement. It stays in `pedigree.patches[].resolves`. This ends m1035's two-statement rule for closure backports.
- **FR-004**: Each statement MUST keep the evidence grade and the patch provenance (m1035) in its impact statement.
- **FR-005**: CycloneDX `vulnerabilities[]` MUST carry one entry per (patched component, CVE). `affects[].ref` MUST be the component's bom-ref.
  - The state MUST be `resolved_with_pedigree`: the native state for a resolution that the component's pedigree shows, and it points at `pedigree.patches`. Catalog row F1, which today says `resolved_with_pedigree` is never emitted, MUST be updated.
  - No entry may mark a patched component `exploitable` for a CVE its patches resolve (#1421).
- **FR-006**: The OpenVEX sidecar and CycloneDX `vulnerabilities[]` MUST make the same claims about the same pairs (m1068: one source, two carriers). SPDX 2.3 and SPDX 3 MUST continue to reference the same sidecar (catalog row F1), and the qualified PURLs MUST be identical across all three formats.
- **FR-007**: Adding the qualifier MUST NOT change the set of findings grype or osv-scanner report on the document when no VEX is supplied. This is measured per target.
- **FR-008**: VEX from sources other than closure backports (m1050's nixpkgs security declarations, and any other VEX) MUST be unchanged by this feature.
- **FR-009**: Before choosing the qualifier key and value, the plan MUST check the PURL specification and the existing catalog for an established key, applying Principle V. It MUST also measure that grype, osv-scanner and Trivy tolerate the chosen qualifier on every PURL type it is applied to. The choice and the evidence MUST be recorded.
- **FR-010**: The catalog (`docs/reference/sbom-format-mapping.md`, row F1 and any new row) and the user guide MUST describe the new shape, the qualifier, and where the version-level claim now lives.
- **FR-011**: The behaviour MUST be testable without network access or real builds, using the existing stub closures: a patched component; an unpatched build of the same name and version; a component patched for one of two CVEs; a filename-derived association; a projected (`--split`/`--tier`) document.
- **FR-012**: No new Cargo dependencies, and no new network access at scan time.

### Key Entities

- **Build identity**: the value that distinguishes this build of a component from any other build of the same name and version. It is carried as a PURL qualifier on patched components only.
- **Backport claim**: one (patched component, resolved CVE) pair with its evidence grade and patch provenance, derived from `pedigree.patches[].resolves`.
- **VEX statement**: the single statement per backport claim, carried in the OpenVEX sidecar and in CycloneDX `vulnerabilities[]`.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Across the six measurement targets, grype with waybill's VEX suppresses **every** patched finding: those whose (component, CVE) is in the document's `pedigree.patches[].resolves`. Today it suppresses 0. The count is re-measured at implementation time against the same targets. It was 102 on 2026-11-10 with #1435's unzip CPE, and 36 without.
- **SC-002**: Across the same targets, **zero** findings disappear that are not patched pairs. That includes zero leakage onto an unpatched build of a patched component's name and version.
- **SC-003**: On every committed closure golden, no `vulnerabilities[]` entry marks a patched component `exploitable` for a CVE its patches resolve. Today moat has 13.
- **SC-004**: Without VEX, grype and osv-scanner report the same findings on the qualified documents as on today's, on every closure golden and measurement target.
- **SC-005**: Goldens of scans without `--nix-closure`, and closure goldens with no backport evidence, are byte-identical.

## Assumptions

- **The measured scanners are the target.** grype 0.120.0 (OpenVEX consumer) and Trivy 0.71.1. Trivy reports nothing on `pkg:generic` Nix components, so it can neither honour nor be harmed by this VEX on them. osv-scanner 2.7.0 consumes no OpenVEX but must not lose findings to the qualifier. Other scanners' behaviour is not claimed.
- **The version-level claim belongs in pedigree.** `pedigree.patches[].resolves` already records which CVEs a patch resolves on the published version. A separate VEX `affected` statement adds no information, and it is the half that breaks one scanner or the other.
- **#1435 waits on this.** #1435 adds unzip's second CPE, which surfaces 11 more patched findings per target. Without this feature those are new unsuppressable noise. Its draft status is lifted when this lands.
- **Reporting grype's SBOM-subject behaviour upstream** is not part of this feature. It is a separate decision that needs the maintainer's approval.
- **Out of scope:** platform-applicability VEX (Edge Cases); VEX for non-closure scans; changing which CVEs a patch is associated with (m1035's grading is unchanged).
