# T016: public corpus at the final code commit

## Run 45245331467, at `bc7d589c`

- **Dispatch**: `public-corpus.yml` with `-f branch=1648-binary-kind-technique`.
- **Commit**: the "Resolve the scanned commit" step reads `scanned commit: bc7d589c715a6045c2acf0e2356edb2141b58770`, the final code commit (T012–T015).
- **Result**: `test result: ok. 166 passed; 0 failed`.

**No drift**, as the spec's Background predicted: no public golden contains a `dynamic-linkage` or `embedded-source-path` component, and the goldens' 540 `cargo-auditable` components keep `manifest-analysis`. No regeneration was needed, and none was done.
