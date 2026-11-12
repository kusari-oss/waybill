# Measurements behind #1423

Collected 2026-11-10 while comparing waybill with sbomnix for 0.15.0.

- waybill `f891b4fe`, nixpkgs `a799d3e3`, aarch64-darwin
- grype 0.120.0, DB built 2026-11-08, `GRYPE_DB_AUTO_UPDATE=false`
- sbomnix 1.9.0
- targets: the five milestone-1377 wrapper flakes (curl, openssh, httpie, gh, ripgrep) plus moat

The input SBOMs and grype outputs came from `specs/1377-nix-subject-identity/measurements/gen-sboms.sh` and `scan.sh`. These scripts read them as `OUT_DIR/<target>/waybill.cdx.json{,.grype.json}`.

## Files

| file | what it is |
|---|---|
| `rt/<t>.paths` | `nix path-info -r <outPath>` against the local store |
| `rt/<t>.cache.paths` | `nix path-info -r --store https://cache.nixos.org <outPath>` |
| `rt/moat.cache.err` | moat's output is in neither store, so the query fails |
| `split.py` / `split.json` | grype findings split by runtime truth × today's scope mark × closure role |
| `runtime.py` / `runtime.json` | runtime truth per component and per finding, and precision/recall of today's marks |
| `reach.py` / `reach.json` | the fallback: a walk from the evaluated root over runtime-linking edges, scored against `rt/` |
| `lib-links/` | #1455 second rule: what darwin's `lib/links` farm holds, what moat's Mach-O files load (`otool -L`, a dyld simulation), how a loaded farm entry is told from an unloaded one, and the waybill binary on moat before and after |
| `doc-outputs/` | #1455: how a documentation output is identified (output name, from the closure's derivations), the runtime query's references (`path-info -r --json`), and what the rule changes on moat on aarch64-darwin, scanned by the waybill binary before and after |
| `c-runtime/` | #1467: the walk against the runtime closure on x86_64-linux and aarch64-darwin, scanned by the waybill binary itself, before and after the walk keeps the compiler's libc |

Runtime truth for a component means one of two things:

- **NV:** a runtime store path's (pname, version) matches the component's (name, version).
- **EMB:** its PURL appears in a waybill scan of a runtime store path. This covers Go modules and Rust crates compiled into a runtime binary.

## Results (curl, openssh, httpie, gh, ripgrep)

| | findings |
|---|---|
| all components, today | 1,301 |
| on runtime components (truth) | 350 |
| on build-only components | 951 |
| … of which today's document marks `scope: excluded` | 175 |
| … of which today's document leaves unmarked (read as runtime) | 776 |

- **Today's marks, as a findings filter:** 31.1% precision, 100% recall.
- **Local store vs cache.nixos.org:** both return identical closures for all five targets, in 0–1 s each. moat is not cached.
- **Fallback walk** (`reach.json`):
  - It marks 346 findings as runtime, of which 331 are true. That is 95.7% precision and 94.6% recall at the finding level.
  - At the component level, recall is lower on darwin: httpie 29 of 55 runtime derivations, gh 1 of 5. The misses are SDK and system derivations (apple-sdk, libSystem, libiconv, libresolv) that carry no grype findings.
- **sbomnix's runtime SBOM** gives 243 findings, all of which are among waybill's 350. sbomnix's openssh runtime document came out empty.

## Reproduce

```sh
python3 -I -B runtime.py "$OUT_DIR" "$NOISE_DIR"   # needs rt/ and rtscan/ under NOISE_DIR
python3 -I -B reach.py   "$OUT_DIR" "$NOISE_DIR"
python3 -I -B split.py   "$OUT_DIR" "$NOISE_DIR/rt"
```

`rtscan/<t>.purls` holds the PURLs from `waybill sbom scan --path <p>` over each runtime store path. It is not committed: it is large and can be regenerated from `rt/<t>.paths`.
