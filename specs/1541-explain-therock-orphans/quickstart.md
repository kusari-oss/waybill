# Quickstart

Measure the I7 population of any CycloneDX document with the corpus harness's own predicate:

```sh
WAYBILL_I7_PROBE_DOC=path/to/doc.cdx.json \
  cargo test -p waybill --test public_corpus probe_i7_on_document -- --ignored --nocapture
```

It prints:
- the document's completeness reason;
- the I7 unreachable count, and how many file-tier components it includes;
- how many unreachable components are explained, and by what;
- the unexplained ones, by PURL type, tier, mechanism and test-data path.

**On TheRock, in CI only:**
- `measure-source` (the source tree with submodules) at the branch against `main`;
- `measure-full` (the built SDK).

Download the documents and run the probe on each.
