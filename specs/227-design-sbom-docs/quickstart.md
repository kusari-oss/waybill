# Quickstart: verifying the new tier section works

**Feature**: 227-design-sbom-docs
**Phase**: 1

This quickstart lets the writer (and future reviewers) run the SC-001 "operator predicts SBOM tier before running a scan" success criterion end-to-end against the finished doc. It's the acceptance test for User Story 1.

## Setup (once)

1. Check out the branch: `git checkout 227-design-sbom-docs`
2. Have a real waybill-emitted SBOM handy for verifying jq recipes. Options:
   - Use one of the audit artifacts at `specs/audit-nuget-realworld/artifacts/` (post-#658 merged).
   - Or generate fresh: `./target/release/waybill --offline sbom scan --path <some-project> --format cyclonedx-json --output /tmp/mixed.cdx.json --no-deep-hash`

## Walkthrough — the SC-001 prediction test

Pick 5 real-world project shapes at random from the following list (or use these 5 verbatim for a first pass):

1. A .NET repo with `.csproj` files + `Directory.Packages.props` + no `packages.lock.json`.
2. A Ruby app with `Gemfile` only (no `Gemfile.lock`).
3. A Rust workspace with `Cargo.toml` + `Cargo.lock`.
4. A Node.js project with `package.json` + `pnpm-lock.yaml`.
5. A Yocto BSP with a `.bb` recipe collection (no lockfile concept).

For each, WITHOUT running waybill, use only the finished `docs/ecosystems.md` to predict:

- Will waybill produce components for this project? (yes / no)
- If yes, at which tier? (source / design / mix)
- If design-tier: will PURLs be versionless? Will a `waybill:unresolved-reason` annotation be present?

Then run waybill against a real instance of each and compare. Target: 5/5 correct predictions.

Expected answers (based on research §A):

1. Design-tier (NuGet reader after #656/#657/#658 emits design-tier for unresolved refs). Versionless PURLs. `waybill:unresolved-reason` annotation present.
2. Design-tier (gem reader with `build_gem_purl` versionless when no Gemfile.lock). Versionless PURLs. `waybill:unresolved-reason` NOT emitted today (research §B gap).
3. Source-tier (Cargo has full lockfile → resolved graph). Versioned PURLs. No unresolved-reason.
4. Source-tier (pnpm lockfile → resolved). Versioned PURLs. No unresolved-reason.
5. Design-tier (Yocto recipe reader always emits design-tier per research §A). Depends on recipe metadata for PURL shape.

If any prediction misses, revise the doc — the miss reveals a doc gap.

## Walkthrough — the SC-002 recipe-verification test

1. Open the finished doc.
2. Locate the "Filter to design-tier only (CycloneDX)" recipe.
3. Copy-paste it and run against `/tmp/mixed.cdx.json` (or the audit fixture).
4. Confirm the output shape matches the recipe's documented expected-output.

Target: recipe located in < 60 seconds, produces documented output on first paste.

## Walkthrough — the SC-003 cross-reference test

```bash
grep -c "SBOM tiers" docs/ecosystems.md
grep -c "^## " docs/ecosystems.md
```

The first count should equal (per-ecosystem section count + 1 for the new tier section's own heading). If lower, some section was skipped in the cross-reference pass.

Alternative: grep for the specific anchor:

```bash
grep -c "#sbom-tiers-source-design-binary\|#sbom-tiers" docs/ecosystems.md
```

Should return one hit per per-ecosystem section that includes a link back to the new tier section — target 100% coverage of readers listed in the coverage matrix.

## Walkthrough — the SC-004 contributor test

1. Open §8 "Contributor guidance" in the finished doc.
2. Read the 4-field convention.
3. Open a precedent-reader file cited in §8 (e.g., `waybill-cli/src/scan_fs/package_db/gem.rs`).
4. Confirm that (a) the reader emits a versionless PURL when version is empty, (b) sets `sbom_tier: Some("design")`, (c) may or may not emit `waybill:unresolved-reason` (per research §B, only NuGet does today; the doc should say so).

Target: all 4 fields match.

## Walkthrough — the SC-006 verification-against-source test

Pick 5 reader-specific claims at random from the finished section. For each, grep the source to confirm.

Example:

- **Claim**: "The gem reader emits a versionless `pkg:gem/<name>` PURL when a Gemfile declaration has no matching Gemfile.lock entry."
- **Verify**: `grep -A 5 "build_gem_purl" waybill-cli/src/scan_fs/package_db/gem.rs`
- **Expected**: sees the `if version.is_empty()` branch at gem.rs:392 (per research §A).

Target: 5/5 claims match source.

## Post-writing checklist

- [ ] SC-001 predictions: 5/5 correct on the 5-project panel above.
- [ ] SC-002 recipe located in < 60s + produces expected output.
- [ ] SC-003 cross-references: 100% coverage of per-ecosystem sections.
- [ ] SC-004 contributor pseudocode matches an existing reader on all 4 fields.
- [ ] SC-005 new section line count ≤ 500 (`sed -n '/^## SBOM tiers/,/^## /p' docs/ecosystems.md | wc -l`).
- [ ] SC-006 5/5 random claims verified against source.
- [ ] Pre-PR gate green: `./scripts/pre-pr.sh` (no code changed, should complete quickly with cached compile artifacts; still required per project convention).
