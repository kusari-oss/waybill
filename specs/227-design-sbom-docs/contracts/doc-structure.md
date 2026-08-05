# Contract: docs/ecosystems.md new-section structure

**Feature**: 227-design-sbom-docs
**Phase**: 1

The waybill project's only public "interface" affected by this milestone is the shape of `docs/ecosystems.md` as read by operators, downstream SBOM consumers, and future contributors. This contract fixes that shape so writing-phase deviations produce visible violations rather than silent scope drift.

## Section placement in `docs/ecosystems.md`

Anchor: `## SBOM tiers: source, design, binary`

Placement: immediately after the existing `## Coverage matrix` section and before the first per-ecosystem section (`## apk` today per file inspection at planning time). This puts conceptual framing before per-ecosystem detail without disturbing the existing matrix.

## Table of contents (section-internal)

```text
## SBOM tiers: source, design, binary

### 1. Concept — what are source, design, binary tiers
### 2. Per-ecosystem design-tier fallback matrix
### 3. Detection recipes (jq for CycloneDX + SPDX)
### 4. The waybill:unresolved-reason annotation
### 5. When design-tier is enough vs when it isn't
### 6. Design-tier and the graph-completeness annotation
### 7. Upgrading design-tier to source-tier
### 8. Contributor guidance (implementing design-tier in a new reader)
```

Approximate line budget per subsection (per research §F, total ≤ 500 lines):

| Subsection | Est. lines | Content type |
|---|---|---|
| 1 Concept | 50 | Prose + one 5-column table |
| 2 Per-ecosystem matrix | 60 | One 5-column table with ~20 rows |
| 3 Detection recipes | 70 | 6 fenced code blocks + prose |
| 4 unresolved-reason annotation | 30 | Prose + 2 code samples |
| 5 When design-tier suffices | 60 | Two bulleted lists + prose |
| 6 Graph completeness interaction | 30 | Prose + 1 jq snippet |
| 7 Upgrading design-tier | 50 | Per-ecosystem bullet list |
| 8 Contributor guidance | 40 | Prose + 2–3 file-path citations |
| **Total** | **~390** | Comfortably under 500-line ceiling with headroom |

## Per-subsection content contract

### §1 Concept

Content MUST include:

- One-paragraph plain-language definition of "tier" as a per-component provenance-strength signal.
- 5-column table: `Tier | Trigger | PURL shape | sbom_tier value | Recipe link`
- Rows exactly for `source`, `design`, `binary` (plus `file` referenced-only-by-link since it's covered in `docs/reference/component-tiers.md`).
- Explicit statement that a single scan output CAN contain multiple tiers side-by-side.

Content MUST NOT:

- Discuss any specific ecosystem in this subsection (that's §2's job).
- Invent tier labels beyond source/design/binary/file (spec Assumptions §2).

### §2 Per-ecosystem matrix

Content MUST include:

- 5-column table: `Ecosystem | Fallback? | Trigger | PURL shape | unresolved-reason emitted?`
- One row per ecosystem currently supported by waybill (~20 rows per research §A).
- Ecosystem cells MUST be Markdown-linked to the corresponding per-ecosystem section anchor further down in `ecosystems.md`.
- Ecosystems with NO design-tier fallback (OS package readers `apk`, `dpkg`, `rpm`, `pacman`, `brew`, `opkg` etc.) MUST have an explicit "no — always source-tier" row rather than being omitted.
- Rows sorted alphabetically by ecosystem name.

Content MUST NOT:

- Duplicate per-ecosystem reader implementation details that already live in the per-ecosystem sections. This matrix is a summary; depth lives elsewhere.
- Include unverified rows. Every row's factual claim comes from research §A source-code citations.

### §3 Detection recipes

Content MUST include:

- Six `jq` recipes minimum: source-only filter (CDX + SPDX 2.3), design-only filter (CDX + SPDX 2.3), unresolved-reason extraction (CDX), per-tier count (both formats).
- Every recipe wrapped in a fenced code block with `bash` highlighter.
- Every recipe verified against a real waybill-emitted SBOM at doc-authoring time per research §C. Verification evidence (a comment stub inside the doc noting "verified against `<path>` on `<date>`", or a link to the fixture SBOM used) MUST accompany each recipe.

Content MUST NOT:

- Include recipes for output formats not currently emitted by waybill (SPDX 3 is currently opt-in labeled experimental per constitution Principle V; SPDX 3 recipes are OPTIONAL — include only if verification against a real SPDX 3 SBOM succeeds).
- Include recipes that require jq version > 1.6 (compatibility floor per spec Assumptions §4).

### §4 waybill:unresolved-reason annotation

Content MUST include:

- Where the annotation appears in each output format (CDX `properties[]`, SPDX 2.3 `annotations[].comment` inside the milestone-071 envelope, SPDX 3 similar structure).
- Concrete value shape as emitted by NuGet today (verified per research §B: "no Version= on <PackageReference>, no CPM entry in Directory.Packages.props, no packages.lock.json entry").
- Explicit "cross-reader consistency gap" note — the annotation is emitted by NuGet only today; other design-tier readers do NOT emit it.
- A pointer to the follow-up GitHub issue filed post-merge that tracks universalizing the annotation.

Content MUST NOT:

- Claim the annotation is universal — that would violate spec FR-010.
- Silently omit the gap — the doc's transparency value depends on flagging it.

### §5 When design-tier suffices

Content MUST include:

- Two bulleted lists side-by-side: "design-tier is enough for X" and "design-tier is NOT enough for Y".
- Explicit call-out of the silent-miss failure mode in vuln scanners (exact-version CVE match on versionless PURL → no match, not a false positive — a false negative masquerading as "clean").
- One paragraph on the interpretive frame: "declared inventory" vs "resolved inventory" — what each supports.

Content MUST NOT:

- Recommend against using design-tier SBOMs — the goal is calibrated use, not avoidance.
- Name specific competing SBOM tools per m150 Q1 Option D precedent (aligned via spec Assumptions §5).

### §6 Graph-completeness interaction

Content MUST include:

- Statement that design-tier components imply partial-coverage classification from m158's perspective.
- Named distinction between the two orphan classes (design-tier fallback vs unreachable-from-root).
- One jq recipe extracting the m158 completeness annotation and reason-code list.

Content MUST NOT:

- Duplicate m158's docs (`docs/reference/graph-completeness.md` if it exists — verified during writing; otherwise a source citation).

### §7 Upgrading design-tier to source-tier

Content MUST include:

- Per-ecosystem bullet list: for each ecosystem that has a design-tier fallback (~15 readers), one bullet naming the specific action (generate `Cargo.lock`, run `bundle install`, use `--warm-go-cache`, etc.) that upgrades design → source.
- Documentation of `--supplement-cdx` as the operator-supplied override mechanism (m119).

Content MUST NOT:

- Recommend actions that require capabilities waybill doesn't have (e.g., "run `dotnet restore`" — that's a proposed follow-up per research §G, not shipped behavior).

### §8 Contributor guidance

Content MUST include:

- 4-field convention: versionless PURL + `sbom_tier: "design"` + `waybill:unresolved-reason` annotation + explicit trigger condition.
- 2–3 precedent-reader file paths with line-number ranges (verified against source at writing time).
- Explicit anti-pattern: "don't invent your own `@unresolved` sentinel; that's what the NuGet reader did before #653 and it produced invalid PURLs downstream tools dropped silently."

Content MUST NOT:

- Prescribe implementation details that don't currently match any existing reader (spec FR-010).

## Cross-reference invariant

For each of the ~30 existing per-ecosystem sections in `ecosystems.md`:

- If the section ALREADY discusses design-tier behavior in-line (verified during writing), add ONE line at the end of that discussion linking to the new tier section (`See [SBOM tiers](#sbom-tiers-source-design-binary) for the general framing.`).
- If the section does NOT discuss design-tier behavior, add a new 2–3-line "Design-tier fallback" subsection at the appropriate placement (typically before the section's closing paragraph). The subsection MUST state (a) whether the ecosystem has a design-tier fallback, (b) what the trigger is, and (c) a link to the new tier section.

**Coverage verification**: after writing, grep for the new tier section's anchor in `ecosystems.md`. Hit count MUST equal (per-ecosystem-section count) + 1 (the anchor definition itself). Any lower means a section was silently skipped; verification is manual per SC-003.

## Success predicates (from spec, mapped to concrete verifiable checks)

- **SC-001** (operator predicts tier): pick 5 random ecosystems, hand a description of each to a first-time reader; count correct predictions from the doc. Target 5/5.
- **SC-002** (consumer locates recipe): time a first-time reader building a jq filter using only the doc. Target < 60s + working filter.
- **SC-003** (all per-ecosystem sections cross-linked): grep-based count as described above. Target 100%.
- **SC-004** (contributor produces correct code path from doc): write pseudocode from §8; compare to an existing reader. Target 4/4 fields match.
- **SC-005** (≤ 500 lines new section): `wc -l` on the new section's line range. Target ≤ 500.
- **SC-006** (5 randomly-selected claims verified against source): pick 5 reader-specific claims from the writing; grep the source for each. Target 5/5 match.
