# Contract — curation rubric

This is the canonical "interface" the milestone-151 doc edit exposes to maintainers: a 5-criterion / threshold-N=3 decision rubric. It is the contract because future maintainers (human or AI) will apply this rubric to new `mikebom:*` keys and need the rule to be specified unambiguously.

## Contract shape

```text
RUBRIC(signal: mikebom:KEY) -> {DEPTH | APPENDIX}
  let yes_count = 0
  for each criterion C in [
    C1: drives_consumer_policy_decision,
    C2: cross_ecosystem_reach OR ecosystem_essential,
    C3: audit_significant,
    C4: composes_with_another_signal,
    C5: wire_shape_requires_documentation_beyond_catalog_row
  ]:
    if signal satisfies C: yes_count += 1
  return DEPTH if yes_count >= 3 else APPENDIX
```

## Criterion definitions (mechanical YES/NO predicates)

### C1 — Drives a consumer policy decision

**YES if** a documented consumer workflow (CVE filtering, license auditing, build-provenance verification, completeness/audit assessment, supplement-conflict resolution) explicitly reads this signal to decide whether to alert, suppress, gate, or escalate.

**NO if** the signal is purely informational (e.g., forensics data like Mach-O load commands, internal pipeline evidence like dedup co-ownership) or is consumed only by mikebom's own internal pipeline.

**Audit method**: search the milestone-150 + milestone-151 doc for an explicit "use it to do X" or "as a filter for Y" or "to suppress Z" clause referencing the signal. If no such clause is plausibly authorable, the signal fails C1.

### C2 — Cross-ecosystem reach OR ecosystem-essential

**YES if** either:
- (a) the signal is emitted by ≥2 ecosystems / readers (CDX components originating from different `scan_fs/package_db/*.rs` readers), OR
- (b) the signal is emitted by exactly one ecosystem AND is essential to the default consumer workflow for that ecosystem (not just an opt-in / advanced-feature flag).

**Examples of (b) — ecosystem-essential single-emitter signals that satisfy C2**:
- `mikebom:not-linked` (Go-only; essential for Go CVE matching because Go's `runtime/debug.BuildInfo` is the only mechanism to prove linker DCE).
- `mikebom:peer-edge-targets` (npm-only; essential for npm SCA closure because npm peer-dep semantics are unique).
- `mikebom:depends-unresolved` + `…-rdepends-unresolved` (currently Yocto-only; essential for Yocto compliance audit because Yocto's recipe inheritance is the dominant transparency gap).

**NO if** the signal is emitted by exactly one ecosystem AND is an opt-in / advanced / niche feature within that ecosystem:
- `mikebom:kmp-source-set` (Kotlin Multiplatform — advanced feature; Android-only Kotlin projects don't use it).
- `mikebom:shade-relocation` (Maven shade plugin — opt-in build-time relocation, not most Maven projects).
- `mikebom:yocto-layer-version-missing` (Yocto-specific transparency niche — most Yocto consumers don't gate on layer-version metadata).

**Audit method**: grep emission sites in `mikebom-cli/src/scan_fs/` and `mikebom-cli/src/generate/`. Count distinct readers / cluster the emission sites by ecosystem. If exactly one ecosystem, apply the "essential vs niche" judgment using the example pairs above as precedent.

### C3 — Audit-significant

**YES if** the signal affects the consumer's trust in the SBOM itself: it answers "should I trust this component's identification?" or "did mikebom miss anything?" or "did the operator override scanner-derived facts?". Drives auditor / reviewer workflows.

**NO if** the signal is runtime-decision-oriented only (e.g., `mikebom:lifecycle-scope = "test"` filters CVE alerts but doesn't change the auditor's view of the SBOM's trustworthiness; it's a consumer-policy signal — C1 — but not an audit-trust signal — C3).

Note: a signal CAN satisfy both C1 and C3 (e.g., `mikebom:assertion-conflict` drives an auditor decision AND a runtime consumer decision). The criteria are not mutually exclusive.

### C4 — Composes with another signal

**YES if** the signal forms a meaningful tuple / trio with related signals that consumers query together. Concrete examples:
- Trust trio: `mikebom:source-type` + `mikebom:evidence-kind` + `mikebom:confidence`.
- Completeness pair: `mikebom:graph-completeness` + `mikebom:graph-completeness-reason`.
- Collision pair: `mikebom:duplicate-purl-divergent` + `mikebom:purl-collisions-detected`.
- Unresolved-deps pair: `mikebom:depends-unresolved` + `mikebom:rdepends-unresolved`.

**NO if** the signal is standalone — consumers query it on its own, not in combination with another `mikebom:*` key. Most appendix-only forensics signals fall here.

### C5 — Wire shape requires documentation beyond the catalog row

**YES if** the signal carries one or more of:
- (a) structured JSON-encoded data (object or array of records);
- (b) a closed enum value space (≥2 distinct named values) that benefits from explicit listing;
- (c) a two-state or three-state interpretation rule that affects consumer behavior (e.g., "absent means either X or Y; consumers MUST disambiguate via Z");
- (d) per-format placement variance that benefits from worked jq examples.

**NO if** the signal is a bare opaque hex string, a single boolean with no two-state interpretation rule, a single numeric with no scaling guidance, or an open-enum free-form string with no documented vocabulary.

## Application procedure (for future maintainers)

1. **Score** the new `mikebom:KEY` against each of C1–C5 (5 yes/no answers).
2. **Sum** the YES count.
3. **Verdict**:
   - YES count ≥ 3 → **DEPTH coverage**: add a new subsection in the appropriate §3 cluster section per the per-signal rendering invariant (data-model.md §1). Add to Appendix A with cross-reference. Add to Appendix B with originating milestone.
   - YES count < 3 → **APPENDIX coverage**: add an Appendix A entry (one-line description + catalog C-row link) only.

4. **Edge cases**:
   - If unsure on C2's "essential vs niche" judgment, look at the existing single-ecosystem precedent table (data-model.md §2 worked-example table) and reason by analogy.
   - If the rubric yields exactly N=3 on a marginal signal, prefer DEPTH coverage — the cost of a slightly-too-prominent depth section is lower than the cost of a consumer never discovering an actionable signal.
   - If applying the rubric to an existing depth-covered signal yields < 3 after a value-space or scope change, the signal should be reconsidered for demotion to appendix (out of scope for milestone 151, but worth a future milestone if it becomes relevant).

## Validation reference

This rubric is validated against:
- **18 depth-covered signals** (milestone 150's 12 + milestone 151's 6): see research.md §R1.1 worked-example table. All score ≥ 3.
- **7 representative appendix-only signals** (Mach-O, PE, ELF, Yocto, shade, dedup-internal): see research.md §R1.2 worked-example table. All score < 3.

Combined: 26/26 sampled signals correctly classified. SC-006 satisfied.

## Out of contract scope

This contract does NOT mandate:
- The internal pipeline behavior of mikebom (no Rust API contracts touched).
- The wire format of any specific annotation (FR-017 — no wire-format changes in this milestone).
- The membership of any specific cluster (placement decisions are per-signal authoring judgments within the existing 4-cluster organization).
- Auto-generation of the rubric scoring from the catalog (the rubric is applied by hand by future maintainers; automation is out of scope per spec Out of Scope).
