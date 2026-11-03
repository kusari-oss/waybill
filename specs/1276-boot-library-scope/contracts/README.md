# Contracts: a package the compiler ships is never scoped as absent

**Milestone**: 1276. The observable commitments, each naming the requirement
it serves and how it is checked.

---

**C1 — the CLI surface is unchanged.** No new flag, no changed default.
`waybill sbom scan --help` is byte-identical to the parent commit. (FR-013)

**C2 — a compiler-supplied component carries no non-runtime scope.** Measured
population: 10 of 78, in `haskell-security-advisories` (8) and
`nix-closure-moat` (2); 1 on `slack-web` (`pretty`). (FR-001, SC-001, SC-002)

**C3 — the exemption is immunity, not repair.** The propagation does not
assign the scope, rather than assigning and later removing it. Checked by the
pass's own summary: an exempt component appears in the immunity count and in
no assignment count. (FR-002)

**C4 — a component outside the exempt class keeps its scope.** 569 of the
corpus's 579 non-runtime scopes survive, and the 8 non-boot components below
`slack-web`'s disabled stanza stay non-runtime. This is the control for the
mirror-image defect. (FR-003, SC-003, SC-004)

**C5 — every exempt component records why, through C170.** No new property:
`waybill:haskell-version-unresolved-reason = compiler-supplied` already states
the fact the exemption rests on. The coincidence between the exempt class and
C170's population is **asserted, not assumed** — a component in the boot set
without C170 would be exempt without recording why. (FR-006, FR-006a, SC-005)

**C6 — the exempt set is the flake's named series, with the union only as a
fallback.** Already true by construction via `candidate_series`, so this is
asserted rather than built. Measured at `a799d3e3`: 41 names for `9.10.x`
against 50 for the union, so a correct implementation and an over-exempting
one differ by count alone. (FR-004, FR-004a, SC-006a)

**C7 — an unreadable compiler configuration exempts nothing.** The scan
succeeds, and no component is immune by default. The fail-closed direction for
this feature: the dangerous default is unearned immunity. (FR-005, Principle XI)

**C8 — I3 is amended, not declared around.** The invariant demanding a
non-runtime marker excludes the exempt class, from the same source of truth as
the propagation. Measured: without this, all 10 become **undeclared** I3
violations, which fails the lane. Adding them to
`invariant-declarations.toml` is forbidden — that file records defects
awaiting a fix, and these are components the feature deliberately corrected.
(FR-002a, R1)

**C9 — a new invariant asserts the complement.** No compiler-supplied
component carries a non-runtime scope, from any origin. With C8 the two
partition the class: every component satisfies exactly one precondition.
Measured direct-mark population: **0** — all 10 are `transitive` — so this
invariant is expected green from the start, which is why SC-007a requires it
be demonstrated to fail. (FR-002a, SC-007a)

**C10 — the document records the exemption count, including zero.** A
document-scope row, because its reader is comparing two documents and a log
line cannot serve that. Audited in research R4: no native carrier in CDX 1.6,
SPDX 2.3 or SPDX 3.0.1; KEEP-NO-NATIVE, parity-bridge, same shape as C184 and
C188. (FR-007, SC-005)

**C11 — all three formats agree about every component's scope.** A correction
visible in CycloneDX and absent from SPDX is the asymmetry the parity
discipline exists to catch. (FR-009, SC-006)

**C12 — milestone 1275's `t057` asserts the correction, and still exists.**
Its assertions invert; its premise checks — that the fixture's member really is
non-runtime and really carries the declared edge — remain, so it cannot pass
vacuously. (FR-010)

**C13 — existing wire values keep their meaning.** A consumer reading
`required`, `optional` or `excluded` reads the new document's the same way;
what changes is which components carry them. (FR-012)

**C14 — every requirement has a test that fails when the behaviour is
removed**, mutation-tested. FR-003's control has its own test, because the
plausible wrong implementation over-exempts and would make every component
below a disabled stanza look runtime. (FR-011, SC-007)

**C15 — every committed probe re-runs and reproduces its finding.** (SC-008)
