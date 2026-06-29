# Quickstart — milestone 151

Operator-facing walkthrough for VALIDATING the milestone-151 doc edit post-merge. Mirrors milestone 150's quickstart pattern (operator-cadence audits + mechanical grep checks + a single jq-recipe harness). Most of the doc's quality is fundamentally an operator-cadence read-through (per spec SC-001) — there's no automated test that can verify "is this added depth coverage useful?".

## Scenario 1 — SC-001 8-question read-through audit

After the doc ships, an operator (or external reviewer simulating a first-time mikebom-SBOM consumer) reads `docs/reference/reading-a-mikebom-sbom.md` end-to-end and must be able to answer all 8 questions WITHOUT consulting `sbom-format-mapping.md` or any other doc:

1. **"What does `mikebom:evidence-kind` mean and what values can it take?"**
   Expected: a closed enum — `direct-observation` (mikebom's eBPF / fs scan observed the evidence), `inference` (mikebom derived the evidence from a lockfile or manifest), `enrichment` (mikebom looked up the evidence from an external source like deps.dev). Found in §3.3 build provenance.

2. **"How do `mikebom:source-type`, `mikebom:evidence-kind`, and `mikebom:confidence` compose to support threshold-based vulnerability-scanner policies?"**
   Expected: source-type answers "where", evidence-kind answers "how", confidence answers "how strongly". Trust-trio composition documented as §3.3's opening paragraph + cross-references between the three signals' subsections.

3. **"What's the difference between `mikebom:linkage-kind = "dynamic"` and `mikebom:linkage-kind = "static"`?"**
   Expected: dynamic = the component is loaded at runtime via dynamic linking (e.g., a `.so`); static = the component is linked into the final binary at build time. Closed enum also includes `mixed` for binaries with both. Documented in §3.1 vulnerability scanning.

4. **"If a Go component has `mikebom:not-linked = true`, what does that mean for runtime CVE matching?"**
   Expected: the component was declared in `go.sum` but mikebom's binary-vs-source comparison proved the Go linker dead-code-eliminated it from the produced binary. CVE matching can suppress alerts for this component. Documented in §3.1.

5. **"If a Go component is missing the `mikebom:not-linked` annotation entirely, what are the two possible interpretations and how do I disambiguate them?"**
   Expected: absent means EITHER (a) the component is confirmed linked via `runtime/debug.BuildInfo` OR (b) no binary was present in the scan to compare against. Disambiguate by checking whether the scan emitted binary-tier components at all (`.components[] | select(.type == "library" and .properties[].name == "mikebom:component-tier" and .value == "binary")` returning anything = a binary was scanned). Documented in §3.1 with the two-state interpretation rule.

6. **"What's the difference between `mikebom:depends-unresolved` and a component being absent from the SBOM entirely?"**
   Expected: `mikebom:depends-unresolved` flags a component that mikebom KNOWS was declared as a dep but couldn't pin to a concrete component — a closure gap that's surfaced for the auditor. A component absent from the SBOM entirely is the case where mikebom doesn't know about the dep at all — silent gap. The first is transparent; the second is opaque. Documented in §3.4 transparency / completeness with reserved-key framing (currently emitted only by the Yocto recipe reader per milestone 128).

7. **"If a component has a `mikebom:assertion-conflict` annotation with `winner = "supplement"` and `justification = "developer-metadata-override"`, what should an auditor do with that signal?"**
   Expected: the operator's supplement file declared a value (e.g., a specific license or supplier) that contradicted what mikebom's scanner observed; the supplement won because the field is metadata (not bytes-derived). Auditor action: validate the supplement-declared value against external evidence (operator's policy, upstream registry metadata, license verbatim text). Documented in §3.4 with the structured-record shape spelled out.

8. **(Curation-criterion check:) "If I'm a future maintainer adding a new `mikebom:foo-bar` annotation, where in the doc do I find the rule for whether `mikebom:foo-bar` warrants depth coverage versus appendix-only?"**
   Expected: §2.1 "Curation rubric" — 5 yes/no criteria with threshold N=3; if 3 or more criteria evaluate YES, add as a new depth-coverage subsection; otherwise add to Appendix A only. The rubric is self-contained — answerable in under 5 minutes from §2.1 alone.

If all 8 answers come from the guide alone: ✅ SC-001 passes. If any answer requires reading `sbom-format-mapping.md` or another doc: the corresponding depth section or rubric subsection needs strengthening before merge.

## Scenario 2 — SC-002 + SC-009 + SC-010 appendix-coverage + hygiene audits

```bash
# 2a — SC-002: every mikebom: key in the catalog is in Appendix A:
grep -E "^\| C[0-9]+\b" docs/reference/sbom-format-mapping.md \
  | grep -oE "mikebom:[a-z0-9-]+" | sort -u > /tmp/catalog-keys.txt

grep -oE "mikebom:[a-z0-9-]+" docs/reference/reading-a-mikebom-sbom.md \
  | sort -u > /tmp/guide-keys.txt

diff /tmp/catalog-keys.txt /tmp/guide-keys.txt
# Expected: any catalog key NOT in the guide is a coverage gap. After milestone
# 151's US5 cleanup, a small number of internal-only keys (e.g., mikebom:component-role
# if confirmed internal at authoring time) may be in the catalog but NOT in the
# appendix — that's the intentional inversion per FR-010. Document each exception
# in the PR description.

# 2b — SC-009: every appendix entry corresponds to an actually-emitted key:
grep -rE "\"mikebom:[a-z0-9-]+\"" mikebom-cli/src/generate/ mikebom-cli/src/scan_fs/ \
  | grep -oE "mikebom:[a-z0-9-]+" | sort -u > /tmp/emitted-keys.txt

comm -23 /tmp/guide-keys.txt /tmp/emitted-keys.txt
# Expected: empty after US5 cleanup. Any key here is an appendix entry with no
# corresponding emission site — a removal candidate.

# 2c — SC-010: every "see §X" cross-reference resolves to an existing section:
grep -oE "§[0-9]+\.[0-9]+" docs/reference/reading-a-mikebom-sbom.md | sort -u > /tmp/refs.txt
grep -oE "^#+ [0-9]+\.[0-9]+ " docs/reference/reading-a-mikebom-sbom.md \
  | sed -E 's/^#+ //; s/ .*//' | awk '{print "§"$1}' | sort -u > /tmp/sections.txt
comm -23 /tmp/refs.txt /tmp/sections.txt
# Expected: empty. Any output is a broken cross-reference.
```

## Scenario 3 — SC-004 + SC-005 depth-covered signal counts

```bash
# 3a — SC-004: ≥18 depth-covered subsections (depth-4 headings inside §3):
awk '/^### 3\./,/^### 4 /' docs/reference/reading-a-mikebom-sbom.md \
  | grep -cE "^#### "
# Expected: ≥18 after this milestone (was 12 after milestone 150).
# Note: paired entries (e.g., "mikebom:graph-completeness + mikebom:graph-completeness-reason")
# count as ONE heading, so the count is ≥18 sections representing ≥21 unique catalog keys.

# 3b — SC-005: cluster balance:
for cluster in "3.1" "3.2" "3.3" "3.4"; do
  count=$(awk "/^### $cluster /,/^### [0-9]+\./" docs/reference/reading-a-mikebom-sbom.md \
          | grep -cE "^#### ")
  echo "Cluster $cluster: $count depth-covered signals"
done
# Expected after milestone 151:
#   Cluster 3.1: 6
#   Cluster 3.2: 3
#   Cluster 3.3: 5
#   Cluster 3.4: 5
# Floor of 3 per cluster (SC-005) satisfied; total 19 sections.
```

## Scenario 4 — SC-003 jq recipe verification

```bash
./specs/151-expand-consumer-guide/verify-recipes.sh
# Expected: "Verification summary: N passed, 0 failed" with N ≥ 6 (≥6 new recipes
# per FR-012). Exits 0 on success.
#
# Failure modes:
# - "fixtures dir not found": $MIKEBOM_FIXTURES_DIR not set or sibling fixture repo
#   not synced to a pinned SHA. Run the milestone-090 fixture-sync flow.
# - "JQ_ERROR" in a per-recipe line: the recipe is malformed; fix in the doc and
#   in the harness in sync.
# - "FAIL — recipe error or unexpected output" with `"present"` expectation: the
#   recipe runs but produces no output because the fixture doesn't carry the
#   signal. Acceptable when the recipe is "present"-expected; failure when
#   "nonempty"-expected.
```

## Scenario 5 — SC-006 curation-rubric application

```bash
# Apply the rubric to the 18 depth-covered signals + 7 appendix-only signals:
# (manual exercise — no automation; SC-006 is verified at authoring time by the
# spec author or a second reviewer reading research.md §R1.1 and §R1.2 tables.)
```

The two tables in research.md §R1.1 and §R1.2 are the canonical SC-006 verification artifacts. After merge, a second reviewer applying the rubric independently should produce matching scores. Any disagreement on a specific criterion's YES/NO answer for a specific signal is a rubric-spec gap that warrants a follow-up milestone clarification (NOT a milestone-151 blocker).

## Scenario 6 — SC-007 single-file deliverable

```bash
# Confirm the shipped diff touches only the single doc file:
git diff main --name-only -- docs/
# Expected output: docs/reference/reading-a-mikebom-sbom.md

git diff main --name-only -- mikebom-cli/ mikebom-common/ mikebom-ebpf/
# Expected output: (empty — no Rust source touched per FR-016 + FR-017)

git diff main --name-only -- .github/
# Expected output: (empty — no CI workflow touched)
```

## Scenario 7 — SC-008 pre-PR gate

```bash
./scripts/pre-pr.sh
# Expected: green except the documented pre-existing sbomqs_parity env failure.
# This milestone is docs-only; clippy + test outcomes match pre-151 main verbatim.
```

## Post-merge — operator-cadence external review

Per spec Assumption 3, the doc's quality is assessed via an operator-cadence read-through (the 8-question SC-001 audit above), not via automated tests. After merge:

1. The maintainer or an external reviewer sits down with a real mikebom-emitted SBOM (any format, any ecosystem) AND the updated doc.
2. They formulate a question relevant to their workflow that touches one of the 6 newly-depth-covered signals ("how do I filter to confirmed-linked Go components only?").
3. They search the updated doc for the answer.
4. They report success / failure in a follow-up issue if the doc didn't help.

This feedback loop drives future milestone updates to the doc — single-file deliverable means edits are surgical.

## Known deferrals (spec Out of Scope)

- No new `mikebom:*` annotation keys.
- No wire-format changes to existing annotations.
- No catalog changes beyond inline clarifications surfaced by depth-coverage authoring.
- No promotion of additional appendix-only signals beyond the 6 listed.
- No competitor-tool comparisons.
- No auto-generated appendix.
- No JSON Schema artifact for `mikebom-annotation/v1`.
- No translations (English-only).
- No interactive consumer tooling.
- No CI gating for `verify-recipes.sh`.
- No Yocto-`mikebom:*` depth coverage.
- No Mach-O / PE / ELF binary-forensics annotation depth coverage.
