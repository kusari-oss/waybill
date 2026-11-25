# Quickstart: checking spec 1656

```sh
cargo test -p waybill --bin waybill graph_completeness
cargo test -p waybill --test closure_structured_attrs --test closure_binary_contents --test boot_library_edges
python3 -I -B specs/1656-closure-read-completeness/measurements/read_gaps.py waybill-cli/tests/fixtures/public_corpus
```

After the golden refresh, `read_gaps.py` shows:
- shellcheck at `C104=partial`;
- every closure target's C105 ending with the two clauses research R5 lists;
- every `root-record-dependencies` unchanged.

Golden refresh, by dispatch only:

```sh
gh workflow run public-corpus.yml -R kusari-oss/waybill --ref main \
  -f branch=1656-closure-read-completeness -f regen_goldens=true
```
