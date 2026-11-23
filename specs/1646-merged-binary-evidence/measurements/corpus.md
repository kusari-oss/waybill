# T013: public corpus at the final code commit

## Run 45118972912, at `49106847`

- **Dispatch**: `public-corpus.yml` with `-f branch=1646-merged-binary-evidence`.
- **Commit**: the summary reads `Scanned commit: 491068473a0851f2798814890f5886d23d228222`, the final code commit (T010–T012), and the workflow definition comes from the same commit.
- **Result**: `test result: ok. 166 passed; 0 failed`.

This is the **no-drift** outcome T013 allowed for, and its basis is measured:
- the public corpus's CycloneDX goldens carry no `embedded-version-string` or `symbol-fingerprint` component (spec 1645's Background, at `bb9d6cc4`);
- the log-only census found no binary match in any target outside TheRock and `pants-backend-ai`, neither of which is a public-corpus target.

So no public golden contains a component whose technique or methods could change. No regeneration was needed, and none was done.
