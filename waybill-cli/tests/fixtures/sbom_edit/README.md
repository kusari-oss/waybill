# `sbom edit` fixtures (milestone 1071)

`full.cdx.json`, `full.spdx.json` and `full.spdx3.json` are one scan of
`project/`, a small npm project with:

- a scoped internal dependency (`@acme/internal-utils`) with its own transitive (`lodash`);
- a public dependency (`express` → `safe-buffer`);
- a development chain (`jest-lite` → `pretty-format-lite`);
- two file-tier components (`src/index.js`, `lib/pricing.js`);
- an internal host in the repository URL and homepage (`*.corp.acme.example`).

Regenerate with the waybill binary at the commit that changes them:

```sh
HOME=$(mktemp -d) waybill --offline sbom scan --path project --no-deep-hash \
  --file-inventory=source-tree --repo https://git.corp.acme.example/acme/shop.git \
  --format cyclonedx-json,spdx-2.3-json,spdx-3-json \
  --output cyclonedx-json=full.cdx.json \
  --output spdx-2.3-json=full.spdx.json \
  --output spdx-3-json=full.spdx3.json
```

The tests read these as inputs, not goldens: they assert properties of the
edited output, so a regeneration needs no test change unless the scan's
shape changes.

Two things a plain npm scan cannot produce, and where they are covered
instead:

- **A test-only dependency.** npm has no test scope. The selector's `test`
  value is covered by the unit tests in `waybill-cli/src/edit/select.rs`,
  and by the corpus test against `public_corpus/npm-express`.
- **A vulnerability.** An offline scan emits none. `sbom_edit_filter.rs`
  injects one onto a dev dependency and one onto a runtime dependency.
