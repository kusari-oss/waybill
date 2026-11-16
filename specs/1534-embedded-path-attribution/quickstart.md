# Quickstart: embedded-path attribution

## See it on a synthetic prefix

**The prefix:**
- a `.pc` file gives `waybill-fixture-p@1.2.3`;
- an unattributed shared library carries `/src/waybill-fixture-p/lib/x.cc` and `/src/waybill-fixture-p/third_party/waybill-fixture-fmt/format.cc` in its `.rodata`.

```sh
cargo test -p waybill --test installed_lib_embedded_paths -- --nocapture
```

**What to expect:**
- The library is in `waybill-fixture-p`'s evidence and in its C227 `waybill:embedded-path-binaries`.
- A versionless `pkg:generic/waybill-fixture-fmt` is nested under the library.
- C225's `embedded-paths` reads `attributed: 1, bundled: 1`.

## The ROCm measurement (CI only)

```sh
gh workflow run measure-full.yml -R kusari-sandbox/waybill-fixture-therock-sdk \
  -f waybill_ref=1534-embedded-path-attribution -f base_ref=main
```

Compare the result with `measurements/attribution.json`. The target is 258 binaries, 1.308 GB.
