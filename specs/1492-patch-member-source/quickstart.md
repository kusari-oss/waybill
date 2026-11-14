# Quickstart: verifying #1502

## 1. The classifier, on a constructed closure

```sh
cargo test --bin waybill s1492
```

The tests cover:
- a consumer that lists a bare-named local patch: the patch is `source`;
- an unlisted twin: `fragment`;
- a generated patch with `env.text` that is listed: `source`;
- the endpoint helper given the closure's patch set: it agrees with the full classification for every member.

## 2. The reader, on the captured moat closure

`$CLOSURE` is the x86_64-linux moat closure from public-corpus run 43535531153, artifact `moat-closure-x86_64-linux` (sha256 `898fcdd3…44ab`). Reading it evaluates nothing.

```sh
waybill debug closure-dump --closure-json "$CLOSURE" --filter .patch \
  | jq -r '.rows[] | [.member_role, .name, .edge_endpoint] | @tsv' | sort | uniq -c
```

Expected after the change: no `fragment` row, and the 5 members are `source` with `edge_endpoint: true`.

## 3. The edge delta

```sh
WAYBILL_TEST_CLOSURE_JSON="$CLOSURE" \
  cargo test --bin waybill s1367_edge_accounting -- --nocapture
```

Run it on the base commit and on the change, and diff the two outputs. The `walked-through` and `from-unemitted` buckets shrink by the edges incident to the 5 members, and `projected, identity level` gives the new C184 `edges`.

## 4. The goldens

Dispatch `public-corpus.yml` on the branch with `regen_goldens=true`. Then compare the regenerated closure goldens with the committed ones, normalized and sorted (`LC_ALL=C`). Every changed component must be a reclassified member, and every changed edge must be incident to one (SC-003, SC-004).
