# Contracts: closure members that are known Hackage packages

**Milestone**: 1275. These are the observable commitments. Each names the
requirement it serves and how it is checked.

---

**C1 — the CLI surface is unchanged.** No new flag, no changed default.
`waybill sbom scan --help` is byte-identical to the parent commit. (FR-010)

**C2 — a confirmed closure member carries a Hackage PURL.** A closure-appended
component whose default attribute's version equals its own, and whose
derivation was built by a GHC, is emitted as `pkg:hackage/<name>@<version>`.
Measured population: 59 of 276. (FR-001, SC-001)

**C3 — it carries the revision's license.** By the same rules the existing
resolution uses, including preserving a non-canonicalisable declaration as a
`LicenseRef-` rather than dropping it. (FR-002, SC-002)

**C4 — it carries the revision's content hash**, hex, converted from Nix
base32, or no hash at all — never a malformed value. (FR-003)

**C5 — a name collision is never given a Hackage identity.** No component in
the document carries a `pkg:hackage` PURL whose version is absent from the
pinned set for that name. The measured 26 — `bash 5.3p9`, `curl 8.20.0`,
`ghc 9.4.8`, `brotli 1.2.0` among them — stay `pkg:generic`. (FR-004, SC-003)

**C6 — identity rests on two independent signals.** The exact pair *and* the
derivation having been built by a GHC. A component where the two disagree is
not reclassified. Measured agreement, over two populations and stated with
them because the figures differ: **60 of 60** exact-pair rows among the 276
closure-**appended** members (the sizing probe, reading the emitted document),
and **105 of 105** among the 323 members the tier **emits** (the T009 probe,
running this feature's own code — the difference is the 47 that merge into
design-tier components). The GHC signal rejects all 26 collisions on its own
in both. (FR-005, FR-006)

**C7 — the agreement is measured, not asserted.** The committed probe reports
the two signals' agreement, and a disagreement on any component is a finding
to explain before the pass is trusted. (FR-006)

**C8 — every unreclassified member says why.** One of: the name is absent from
the set; the name matches but no version does; the signals disagree; no
version was recorded so no lookup was possible; the set was unavailable.
Measured today: 0 of 276 carry any reason.

The row is **also** emitted on `confirmed` members, above FR-007's floor. That
is deliberate: it makes the same row answer FR-013 — a closure-derived
`pkg:hackage` component carries `confirmed`, a manifest-derived one carries
nothing — which is a cleaner tier signal than inferring it from
`waybill:closure-role`, a property about how nix *referenced* a member. It is
**not** emitted on the components FR-012c synthesises for absent boot-library
targets: those are not members the pass examined, and a seventh value would
make a row that declares a closed set not have one. (FR-007, FR-013, SC-004)

**C9 — no two components share one identity.** Reclassification does not
change `(name, version)`, which is what the existing merge keys on, so it
cannot create a duplicate. Checked rather than assumed. (FR-008, SC-005)

**C10 — an unavailable package set degrades, it does not fail.** Every member
stays `pkg:generic`, the scan still succeeds, and the document records that
the pass did not run. (FR-009)

**C11 — a confirmed member carries declared out-edges**, derived from the
dependencies the pinned set declares, with every target a component present in
the document. Measured: 278 declared, 265 resolving by name. (FR-012, SC-005a)

**C12 — no edge is derived from a store reference.** Store references continue
to drive a component's *role* and nothing else, as milestone 1066 decided. An
operator reading `dependsOn` reads declared dependencies, in closure-derived
and manifest-derived components alike. (FR-012a, SC-005b)

**C13 — an unreclassified member gains no edges.** Edges follow the
declaration; a member the set does not identify has no declaration to follow.
(FR-012b)

**C14 — no edge dangles.** Where the set declares a dependency the closure
does not contain, the target is emitted as a versionless compiler-supplied
component. Measured: 13 of 278, all three distinct targets being GHC boot
libraries. Neither a dangling reference nor a dropped edge. (FR-012c)

**C15 — the PURL rewrite carries its edges in the same step.** No relationship
refers to a PURL this pass replaced. #980 measured that the alternative is
silent in all three formats. (FR-012, FR-013)

**C16 — a closure-derived Hackage component is distinguishable from a
manifest-derived one.** A consumer can tell which tier produced the identity.
(FR-013)

**C17 — the closure itself is unchanged.** Same derivation count, same role
classification, same membership. This feature changes how a member is named
and described, never which members exist. (FR-011, SC-006)

**C18 — every other ecosystem is byte-identical.** (SC-007)

**C19 — existing wire values keep their meaning.** A consumer reading today's
`pkg:hackage` components reads the new document's the same way. (FR-015)

**C20 — every requirement has a test that fails when the behaviour is
removed**, mutation-tested. FR-004's forbidden name-only match has its own
negative test, because an over-matching implementation produces a confidently
wrong identity — worse than the opaque one it replaces. (FR-014, SC-008)

**C21 — every committed probe re-runs and reproduces its finding.** (SC-009)
