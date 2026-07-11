# Specification Quality Checklist: Maven + Gradle optional-dependency classification

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-07-11
**Feature**: [spec.md](../spec.md)

## Content Quality

- [X] No implementation details (languages, frameworks, APIs) — spec references `maven.rs:689`, `maven.rs:578`, `gradle/lockfile.rs:38`, and `parity/extractors/cdx.rs:866` as file-path anchors so a planner can find the affected code, but user-story text stays at the operator level (SBOM filter-parity for Java-ecosystem `<optional>` / `compileOnly` declarations). Constitution Alignment cites internal types (`LifecycleScope::Optional`, `is_non_runtime()`) as design signals, not prescriptions.
- [X] Focused on user value and business needs — two P1 user stories each pin a distinct pico filter-parity gap for a distinct Java ecosystem (Maven US1, Gradle US2). Every FR traces to a user-visible false-positive Runtime edge that misleads SBOM consumers today.
- [X] Written for non-technical stakeholders — reader can follow "Maven `<optional>true</optional>` deps and Gradle `compileOnly` deps aren't required at runtime, but mikebom emits them as Runtime today" without opening code.
- [X] All mandatory sections completed — User Scenarios (2 stories + 10 edge cases), Requirements (14 FRs), Success Criteria (9 SCs), Assumptions, Constitution Alignment, Deferred to Future Milestones all present.

## Requirement Completeness

- [X] No [NEEDS CLARIFICATION] markers remain — every design decision documented in Assumptions with cross-references to m179+m180+m181+m183 precedent (KEEP-BOTH polarity, is_non_runtime() filter, C122 catalog value-set growth, dev/build-wins-over-optional).
- [X] Requirements are testable and unambiguous — FR-001 through FR-014 all specify observable classifier decisions + emission behavior; FR-005/FR-006 pin exact precedence semantics; FR-011 pins the byte-identity regression guard; FR-013/SC-009 pin zero-new-dep.
- [X] Success criteria are measurable — SC-001/002 are set-equality gates (pico filter-parity), SC-003 pins net-decrement-zero, SC-004/005 pin byte-identity on non-Java fixtures, SC-006 is the m228 basic-mode preservation gate, SC-008 is the C122 parity annotation gate, SC-009 is the `cargo tree` line-count invariant.
- [X] Success criteria are technology-agnostic — reference operator-visible behaviors (edge counts, filter-parity set-equality, byte-identity across formats) rather than specific parsing internals.
- [X] All acceptance scenarios are defined — US1 has 4 scenarios (happy path, byte-identity annotation, scope preservation, test-wins-over-optional), US2 has 5 (happy path, byte-identity annotation, compile+runtime NOT optional, runtime-only NOT optional, buildscript-wins-over-optional); every scenario uses GIVEN/WHEN/THEN.
- [X] Edge cases are identified — 10 cases covering `<dependencyManagement>` non-classification, provided-scope precedence, inherited-optional-via-parent deferral, Gradle empty=... preservation, annotation-processor treatment, test-scope Gradle configs, dual-format coexistence, `--spdx2-relationship-compat=basic`, `--include-dev=false`, DSL parsing deferral.
- [X] Scope is clearly bounded — Deferred section explicitly lists inherited-optional resolution, DSL parsing, annotation-processor dedicated value, sbt/Mill/Ant-Ivy, and multi-module reactor cross-classification as OUT of m184 scope.
- [X] Dependencies and assumptions identified — 7 assumptions covering `<optional>` parse rule, Gradle suffix-based compile-only detection, independent per-format classification, root-project scope, docstring-unchanged invariant, expected golden regen shape, DSL-parsing deferral, inherited-optional deferral.

## Feature Readiness

- [X] All functional requirements have clear acceptance criteria — FR-001/002 ⇔ US1 acceptance 1+2 + SC-001; FR-003 ⇔ US2 acceptance 1+2 + SC-002; FR-004 ⇔ SC-008 (distinct-value byte-identity); FR-005 ⇔ US1 acceptance 4 (Test/Build/Runtime precedence); FR-006 ⇔ US2 acceptance 5 (buildscript precedence); FR-007 ⇔ SC-006 (basic-mode preservation); FR-008 ⇔ Edge Cases `--include-dev=false`; FR-009/010 ⇔ SC-004/005 byte-identity guards; FR-011 ⇔ SC-004 regression pin; FR-012 ⇔ SC-007 test-continuity; FR-013 ⇔ SC-009 zero-new-dep; FR-014 ⇔ pre-committed docstring invariant.
- [X] User scenarios cover primary flows — two distinct filter-parity flows: Maven `<optional>true</optional>` (US1, biggest Java installed base — Spring Boot, Hibernate) + Gradle `compileOnly` (US2, growing Android/Kotlin greenfield installed base). Combined they cover ~100% of the Java lockfile/POM landscape mikebom currently reads.
- [X] Feature meets measurable outcomes defined in Success Criteria — SC-001/002 are the two individual gap-fix gates; SC-003 is the net-decrement-zero regression guard; SC-004/005 are byte-identity guards; SC-006 preserves m228 escape hatch; SC-008 is the C122 parity annotation gate; SC-009 is the zero-new-dep pin.
- [X] No implementation details leak into specification — spec names internal types (`LifecycleScope::Optional`, `is_non_runtime()`, `PomDependency`) and file-path anchors as design signals but frames them as planning-phase surfaces, not user-facing prescriptions.

## Notes

- **All 16 checklist items PASS as of 2026-07-11**. Ready for `/speckit-plan`.
- **/speckit-analyze remediations applied (2026-07-11)**: R1 (HIGH — fixed FR-005 wording that incorrectly listed `<scope>runtime</scope>` as a scope-wins scope over `<optional>true</optional>`; the Decision Matrix + Decision 2 correctly say only `<scope>test</scope>` + `<scope>provided</scope>` win. Also fixed FR-002's parenthetical to remove Runtime from the "more-specific scope" list). R2 (LOW — extended T022 with an FR-008 `--include-dev=false` sub-check mirroring the m183 T024 R1 pattern). R3 (LOW U2 FR-014 docstring-unchanged invariant) accepted as-is — very low risk since m184 tasks don't touch the docstring file and T020 pre-PR gate would surface any accidental edit. Task count unchanged: 26 tasks (24 pending + 2 deferred).
- Delivery cadence: 2 user stories (US1 + US2 as P1 for their respective Java-ecosystem installed bases). Fit a single-PR bundle per m180/m181/m183 precedent — the shared `LifecycleScope::Optional` infrastructure means only per-reader classifier deltas are needed.
- **US1 (Maven) is a NEW extraction path** — `PomDependency` struct at `maven.rs:578` gains an `optional: bool` field, and `parse_pom_xml` at line 689 needs a new XML element handler for `<optional>`. Larger surface than m183 US1 (which just added a helper); still isolated to maven.rs.
- **US2 (Gradle) is a shape-inference extension** — `read_gradle_lockfile` at `gradle/lockfile.rs:38` already parses the configs list into a string annotation. m184 adds a suffix-based check for the compile-only shape (compileClasspath present + runtimeClasspath absent) and sets `lifecycle_scope` at construction time. Smaller surface than US1.
- **Distinct derivation values `"maven-optional-element"` + `"gradle-compile-only"`** — the Java-family gets TWO values, unlike m180/m181/m183 which reused ONE per ecosystem family. Rationale: the mechanisms are semantically distinct (POM `<optional>` element = transitive-exposure control; Gradle `compileOnly` = classpath-composition). Values already pre-committed in the C122 docstring since m179; m184 makes them real.
- **Test-scope-wins-over-optional (US1 acceptance 4) + Build-scope-wins-over-optional (US2 acceptance 5)**: matches Maven's own semantic (a test-scope dep only lives in the test classpath regardless of `<optional>`; a provided-scope dep is compile-time provided by the container). Simplifies the classifier + preserves `--include-dev=false` filtering behavior + honors the one-derivation-per-component invariant m180 established.
- No clarifications needed. The m179+m180+m181+m183 precedent + Maven POM spec + Gradle configuration semantics resolve every "how should this behave?" question. Delivery follows the established m179+ cadence.
