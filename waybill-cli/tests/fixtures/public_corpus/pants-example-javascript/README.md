# pants-example-javascript — m195 corpus goldens

These goldens capture the **pre-option-A baseline** for how waybill's
npm reader stack (m066 + m147 + m180) emits SBOMs when scanning a
Pants-managed JavaScript monorepo.

## Contents

- `cdx.json` — CycloneDX 1.6 JSON, JS-filtered
- `spdx-2.3.json` — SPDX 2.3 JSON, JS-filtered
- `spdx-3.json` — SPDX 3.0.1 JSON-LD, JS-filtered

All three formats are **JS-filtered** per FR-008 in
`specs/675-pants-js-corpus/spec.md` — non-`pkg:npm/*` components are
present in the emitted SBOM but excluded from the golden diff. This
isolates regression signal from unrelated ecosystem drift in the
pinned fixture.

## When to regenerate

- **Legitimate**: [issue #760 option A](https://github.com/kusari-oss/waybill/issues/760)
  lands and starts decorating `pkg:npm/*` components with
  `waybill:pants-target` annotations. Regenerating these goldens
  should show ONLY additive Pants-provenance annotations on existing
  `pkg:npm/*` components (no dropped components, no changed PURLs, no
  reordered edges).
- **Legitimate**: pinned SHA refresh via `scripts/corpus/refresh-pins.sh`.
  Regenerate + review diff to distinguish upstream-content churn from
  waybill-behavior churn.
- **Legitimate**: waybill release bump — version-string in
  `annotator` field rotates. Per
  memory `feedback_release_bump_regen_all_golden_tests`, add
  `public_corpus` to the release-bump regen checklist.

## Regeneration recipe

```bash
WAYBILL_RUN_PUBLIC_CORPUS=1 \
WAYBILL_UPDATE_PUBLIC_CORPUS_GOLDENS=1 \
WAYBILL_CORPUS_SKIP_OCI=1 \
cargo test --test public_corpus corpus_pants_example_javascript
```

The regen PR body MUST justify any regeneration.
