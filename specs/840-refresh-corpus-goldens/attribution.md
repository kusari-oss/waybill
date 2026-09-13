# T010–T012 — attribution of the 33 corpus golden diffs

Artifact for FR-006 / FR-007. Source: regen run 34666830925
(`corpus-goldens-regen`, 11 targets × 3 formats), compared against the
goldens committed at `25bfbce4` (2026-07-21, the m214 rename).

All 33 diffs were produced with `xtask corpus-diff --old <committed>
--new <regen> --format <f>` and read. Nothing below is inferred from a
commit title alone; each claim names the evidence that produced it.

## Staleness check performed first

Four merges landed between the regen and this attribution (m841, m842,
m843, m850). None can move these goldens:

- m841 / m842 are deps.dev enrichment fixes. The harness scans with
  `--offline` (`corpus_harness_195/harness.rs:184`), so no deps.dev
  call occurs.
- m843 is documentation only.
- m850 adds `--no-go-proxy-fetch` and also sets the env var when
  `--offline` is passed — but step 3 was *already* gated by
  `if !ctx.offline` (`graph_resolver.rs:688`), so the offline path is
  unchanged.

The regen artifact is therefore still representative.

## Categories — benign, explained

| # | cause | where | evidence |
|---|-------|-------|----------|
| 1 | tool version `alpha.62` → `0.7.0` | all 11 targets | `metadata.tools.components[0].version`, `annotations[].annotator`, SPDX-3 `createdBy`/`createdUsing`. Three pants targets differ by **nothing else** in cdx. |
| 2 | document `$.name` `"repo"` → `"<target> <shortsha>"` | 9 targets, spdx-2.3 | old name was the corpus cache leaf directory (`.../<pin>/repo`); new is target + pin. Naming improvement. |
| 3 | content-addressed id cascade | 427× django, 314× jvm, … | SPDX ids derive from content, so categories 1/4/5/6 cascade into `spdxId`, `subject`, `statement`, `from`, `to[]`, `documentDescribes[0]`, `relatedSpdxElement`, `packages[0].SPDXID`. |
| 4 | uv.lock reader emits hashes (m674, #754) | 83× python-flask | `components[].hashes` / `packages[].checksums` added. |
| 5 | source-provenance refs (m776, #797) | 302× pants-js, 51× ripgrep | `externalReferences` / `externalRefs`. |
| 6 | PEP 735 dev-dep groups (#784) | 1× python-flask | root `dependsOn` 12 → 29; 19× added `scope`; every new entry `lifecycle-scope=optional`. |

## FR-007 findings — NOT benign drift

### A. image-postgres16 — file-tier coverage swung. **Blocks refresh.**

`library` components are **unchanged at 144**. Every one of the +213 is
file-tier, and the composition inverted:

| prefix | old | new |
|--------|-----|-----|
| `bin/` | 326 | **0** |
| `sbin/` | 82 | **0** |
| `lib/` | **0** | 616 |
| `var/` | 123 | 123 |

Losing every executable from a container image inventory is a coverage
regression, not accumulated drift. The mechanism is the file-tier
`dedupe_index` (`file_tier/walker.rs`), which skips files already
claimed by a package reader — coverage moved in *both* directions, which
points at path matching between dpkg's file lists and the walker's
observed paths. There is no global inventory cap (`FILE_PATHS_CAP` is
per-entry, for one hash appearing at many paths).

Per T013 this target must **not** be regenerated: doing so encodes the
fault as expected output.

### B. SPDX 2.3 emits self-referential relationships

`spdxElementId == relatedSpdxElement`:

| target | old | new |
|--------|-----|-----|
| rust-ripgrep | 17 | 17 |
| maven-guice | 4 | **7** |

Pre-existing, and growing on maven-guice. In the old ripgrep golden one
of these was typed `OPTIONAL_DEPENDENCY_OF` — the document root declared
an optional dependency of itself. The new golden homogenises the type to
`DEPENDS_ON` but the **count is unchanged**, so the defect persists; only
its label changed. Not introduced by this drift, so it does not block the
refresh, but it should not be blessed silently either.

### C. maven does not resolve `<dependencyManagement>` versions

`pkg:maven/junit/junit@unknown` (new) and
`pkg:maven/com.google.code.findbugs/jsr305@unknown` both carry
`waybill:unresolved-reason = "no <version> in pom.xml; no
dependency-reduced-pom.xml or effective-pom fallback"`.

The guice root `pom.xml` declares **both** in `<dependencyManagement>`
with concrete versions — junit `4.13.2`, jsr305 `3.0.1` — which the child
poms inherit. So the version is present in the same file tree, and the
reason string is misleading. 16 such components in maven-guice.
Pre-existing (15 in the old golden); does not block.

### D. jsr305 source attribution moved — benign, verified stable

`extensions/throwingproviders/pom.xml` → `extensions/testlib/pom.xml`.
Both poms declare jsr305 version-less, so both are legitimate winners of
an arbitrary pick. Verified **deterministic**: three consecutive local
scans of the pinned guice checkout produced byte-identical attribution,
all selecting `testlib`, matching the regen. The golden will not
oscillate. Freeing that pom let it appear as file-tier, making all six
extension poms file-tier where previously five were — the new state is
the consistent one.

### E. Review-tool gap — `xtask corpus-diff` cannot align SPDX-3 `@graph`

Element-wise grouping happens only when `@graph` length is unchanged:

| aligned | `@graph` old → new |
|---------|--------------------|
| yes | django 481→481, jvm 359→359, python 126→126, javascript 14→14 |
| **no** | go-cobra 98→105, image-postgres16 5036→**6315**, maven-guice 447→479, npm-express 368→412, pants-golang 65→69, python-flask 979→1086, ripgrep 501→502 |

The 7 length-changed targets each report one opaque `changed $.@graph`
line. SPDX-3 review is therefore **blind for 7 of 11 targets**, including
the +1279-element image-postgres16 change. FR-007 cannot be satisfied for
those targets until the normaliser aligns on a stable semantic key rather
than requiring equal lengths — sorting by the content-addressed `spdxId`
cannot work, because that is precisely what changes.

## Disposition

- Categories 1–6 and finding D: explained; those targets may proceed.
- Finding A: image-postgres16 blocked pending T013 decision.
- Findings B, C: pre-existing defects; track separately, do not block.
- Finding E: blocks SPDX-3 sign-off for 7 targets; tool fix needed first.
