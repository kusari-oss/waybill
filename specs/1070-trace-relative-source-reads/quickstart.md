# Quickstart: verifying milestone 1070

Needs a Linux host with eBPF and Docker (Colima works on macOS). Mount a directory under `$HOME` so the VM can share it.

```bash
docker build -f Dockerfile.ebpf-test -t waybill-ebpf-test .
OUT=$HOME/.cache/waybill-1070; mkdir -p "$OUT"
FX=/waybill/waybill-cli/tests/fixtures/compiler_pipeline/two_binaries_diverge
docker run --rm --privileged -v /sys/kernel/debug:/sys/kernel/debug -v "$OUT":/out \
  --entrypoint bash waybill-ebpf-test -c "cd /waybill && target/release/waybill trace capture \
    --attestation-format waybill-v1 --output /out/att.json \
    -- cargo build --release --manifest-path $FX/Cargo.toml --target-dir $FX/target"
```

## US1: crate roots are in the read sets (SC-001)

```bash
jq -r '.predicate.compiler_pipeline.invocations[] | select(.compiler=="rustc")
       | .read_set[].path' "$OUT/att.json" \
  | grep -E '/(libsafe|libvuln)/src/lib\.rs$|/binaries/(safe-only|vuln-included)/src/main\.rs$' | sort -u
```

Expect 4 lines, each an absolute path. The baseline is 0.

## US2: outputs are in the write sets (SC-002)

```bash
jq -r '.predicate.compiler_pipeline.invocations[]
       | "\(.compiler) \(.write_set | map(.path) | join(" "))"' "$OUT/att.json"
```

Expect:
- the two linker invocations to write `deps/safe_only-<hash>` and `deps/vuln_included-<hash>`;
- the two library compiles to write `deps/liblibsafe-<hash>.rlib` / `.rmeta` and `deps/liblibvuln-<hash>.rlib` / `.rmeta`, under their final names, not `rmeta*/full.rmeta`.

## US3: unresolved opens are visible (SC-004)

```bash
jq '.predicate.trace_integrity.unresolved_relative_opens' "$OUT/att.json"
jq '[.predicate.file_access.operations[] | select(.unresolved_relative)] | length' "$OUT/att.json"
```

The two numbers are equal. On the fixture, expect the directory-fd-relative opens (toolchain name, `raw-dylibs`) and the failed loader probes. Nothing else is expected.

## SC-003: no overflows

```bash
jq '.predicate.trace_integrity.ring_buffer_overflows' "$OUT/att.json"   # 0
```

The same assertions run in `scripts/ebpf-integration-test.sh`, which CI runs in the eBPF lane.
