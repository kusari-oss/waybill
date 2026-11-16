# Quickstart: declared datasets

## Synthetic

```sh
cargo test -p waybill --test cmake_declared_datasets -- --nocapture
```

**The fixture:** a CMake file declares three `waybill-fixture-*` matrices with MD5s, an index loop and a primary plus a fallback `file(DOWNLOAD)`.

**What to expect:**
- three data components in all three formats;
- one per element, with no duplicate from the fallback URL;
- C229 `{"datasets":3,"digest-mismatches":0,"not-classified":0,"read":2,"unresolved":0}`.

## ROCm source (CI only)

The fixture repository's `census-downloads` run checks out rocSPARSE's and hipSPARSE's CMake files at TheRock's pin. Scanning each project root:

```sh
waybill --offline sbom scan --path src/rocm-libraries/projects/rocsparse --format cyclonedx-json --output r.cdx.json
```

This should give 24 dataset components (19 for hipSPARSE), each with its `sparse.tamu.edu` URL and MD5 (SC-001).
