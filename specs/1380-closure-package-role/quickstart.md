# Quickstart: verifying spec 1380

1. **Census before:**
   `specs/1380-closure-package-role/measurements/package-role-census.sh` lists today's `package` members per closure golden.
2. **Unit tests:** `cargo test -p waybill --bins member::` covers each rung, the order of the sub-rungs, and the invariant that only `package` members move.
3. **Fixture test:** `cargo test -p waybill --test closure_member_role` uses a synthetic closure with one member per rung (E1 patch, E2 `srcOnly`, `-src-<v>`, `-vendor`, `-get-version-<v>`, `-simple-program-<v>`, a genuine package, an `-env`, a fragment patch with no version token). It asserts every member's role in all three formats, and edges identical to a classification that ignores the new sub-rungs.
4. **Goldens:** dispatch `public-corpus.yml` with `regen_goldens=true`, then compare against research's prediction table:
   - roles by group (re-run the census on the new goldens);
   - named edges identical (sorted, normalized diff of `dependencies[]`);
   - non-closure targets byte-identical.
5. **Gate:** `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`.
