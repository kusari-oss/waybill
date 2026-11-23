# T015: public corpus at the final code commit

## Run 45206557193, at `541e2bff`

- **Dispatch**: `public-corpus.yml` with `-f branch=1647-static-liblzma-anchor`.
- **Commit**: the "Resolve the scanned commit" step reads `scanned commit: 541e2bff22e340a829c522fad3fd7e71e872dc7f`, the final code commit (T011–T013).
- **Result**: `test result: ok. 166 passed; 0 failed`.

**No drift.** The log-only commit's run (45201924694) also passed 166 of 166, and the wired change alters output only where a binary defines a liblzma anchor in `.symtab` alone. No public golden contains such a binary. No regeneration was needed, and none was done.
