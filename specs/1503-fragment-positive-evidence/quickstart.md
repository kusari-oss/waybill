# Quickstart: verifying #1511

## 1. The classifier, on constructed closures

```sh
cargo test --bin waybill s1503
```

The tests cover:
- a pname-less member built by `kaem` with no earlier evidence → `fragment`;
- the same member built by bash → not `fragment`;
- `setup-hook.sh` built by bash → `scaffolding`;
- `nuke-refs` and `stdenv-linux` built by bash → `package`, and edge endpoints;
- the control (SC-005): with the builder check removed, the bash-built member is `fragment` again.

## 2. The reader, on each captured closure

`$CLOSURE` is one of the five closures captured in CI. Moat comes from run 43535531153. The others come from run 43629708718, artifact `nix-closures-x86_64-linux`, with sha256s in its `SHA256SUMS`. Reading a capture evaluates nothing.

```sh
waybill debug closure-dump --closure-json "$CLOSURE" > dump.json
jq -r '.rows[] | select(.member_role) | [.member_role, .member_role_rung] | @tsv' dump.json | sort | uniq -c
```

Expected after the change:
- `fragment` appears only with rung `live-bootstrap-builder`;
- every `default` member is listed in `measurements/<target>/default-members.tsv` for review.

## 3. The edge delta

```sh
WAYBILL_TEST_CLOSURE_JSON="$CLOSURE" \
  cargo test --bin waybill s1367_edge_accounting -- --nocapture
```

Run this on the base commit (ac2bd28) and on the change, then diff the outputs.
- On moat, `walked-through` falls by the consumer edges into the 15 new `package` members.
- On openai and slack-web, `walked-through` falls by the edges that passed through `stdenv-linux` and the stages.

## 4. The goldens

Dispatch `public-corpus.yml` on the branch with `regen_goldens=true`. Then:
- compare the regenerated closure goldens with the committed ones, normalized and sorted (`LC_ALL=C`);
- check that every changed component is a reclassified member and every changed edge is incident to one (SC-003);
- check that the 49 non-closure fixture files and the three `recall-truth.json` are byte-identical (SC-006).
