A debug build of waybill panics when it emits CycloneDX for any binary identified through cargo-auditable data. Release builds are unaffected, because the check is a `debug_assert!`.

## Reproduce

waybill `main@72b455e2`, debug profile, on a cargo-auditable binary — nixpkgs' `ripgrep` is one:

```sh
cargo build -p waybill --bin waybill
out=$(nix build --no-link --print-out-paths nixpkgs/a799d3e3886da994fa307f817a6bc705ae538eeb#ripgrep)
./target/debug/waybill sbom scan --no-deep-hash --no-clearly-defined --path "$out" \
  --format cyclonedx-json --output cyclonedx-json=rg.cdx.json; echo "exit=$?"
```

```
thread 'main' panicked at waybill-cli/src/generate/cyclonedx/builder.rs:1744:17:
waybill:evidence-kind value 'cargo-auditable' is not in the canonical enum (rpm-file | rpmdb-sqlite | …)
exit=101
```

The binary reader had identified 45 components before emission stopped.

## Cause

`BINARY_EVIDENCE_KINDS` in `waybill-cli/src/scan_fs/binary/mod.rs` lists `cargo-auditable`; the `debug_assert!` allowlist in the CycloneDX builder does not. Comparing the two, it is the only kind present in the first and absent from the second.

A debug-build test run passes today, so no test sends a cargo-auditable binary through CycloneDX emission.

## Expected

Add the kind, and derive the assertion's allowlist from the shared constant(s) rather than restating it, so the two cannot drift again. Add a test that emits a cargo-auditable component in a debug build.

---
*Drafted with AI assistance (Claude), reviewed and filed by me. Measurements quoted were executed and verified.*
