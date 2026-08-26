# Quickstart: adopting `--no-binary-scan=<MODE>`

**Audience**: waybill operators wanting to trade Go-binary content probing for faster scans on large trees.

## When to use this flag

Use `--no-binary-scan=go` when:

- You're scanning a large repo (thousands of files) that DOESN'T contain statically-linked Go binaries you need to identify by module.
- Your downstream SBOM consumer doesn't consume `pkg:golang/*` components derived from binary probing (i.e., you're OK with dpkg / apk / rpm / pip-derived components only).
- Scan wall-time is a bottleneck for your CI pipeline.

DO NOT use this flag when:

- You need statically-linked Go binary module attribution (e.g., you're auditing container images that ship Go binaries not owned by any OS-package manager).
- You're producing SBOMs for compliance regimes that require binary-content-based provenance (CISA 2026 §"Explicitly Identifying Unknown Information" — a suppressed reader still emits the completeness signal via the `waybill:binary-scan-suppressed` annotation, but consumers must be able to interpret that signal).

## 5-step operator recipe

### Step 1 — Baseline your current scan

```sh
time waybill sbom scan --offline --file-inventory=off \
  --path /path/to/repo --format cyclonedx-json --output /tmp/current.cdx.json
```

Record the wall-time. This is your baseline.

### Step 2 — Run with `--no-binary-scan=go`

```sh
time waybill sbom scan --offline --file-inventory=off \
  --no-binary-scan=go \
  --path /path/to/repo --format cyclonedx-json --output /tmp/fast.cdx.json
```

Expected wall-time reduction depends on file count. Reference (macOS APFS, warm cache):

- ansible (5.8k files):   0.777s → ~0.3s
- pytorch (21k files):    1.117s → ~0.4s
- mongo (55k files):      3.04s → ~0.7s

### Step 3 — Verify the suppression annotation is present

```sh
jq '.metadata.properties[] | select(.name == "waybill:binary-scan-suppressed")' /tmp/fast.cdx.json
```

Expected output:

```json
{
  "name": "waybill:binary-scan-suppressed",
  "value": "go"
}
```

Absence of this annotation on `/tmp/current.cdx.json` (from Step 1) is also expected — the annotation is present iff the flag was set.

### Step 4 — Diff component counts

```sh
COMP_WITHOUT=$(jq '.components | length' /tmp/current.cdx.json)
COMP_WITH=$(jq '.components | length' /tmp/fast.cdx.json)
echo "Baseline: $COMP_WITHOUT components; --no-binary-scan=go: $COMP_WITH components"
echo "Suppressed: $((COMP_WITHOUT - COMP_WITH))"
```

Expected: the delta equals the number of `pkg:golang/*` components that WOULD have been emitted from binary probing (their sources' components — pip / npm / etc. — remain).

### Step 5 — Adopt via env var for CI

For CI pipelines that always want the fast mode:

```yaml
# .github/workflows/sbom.yml (example)
env:
  WAYBILL_NO_BINARY_SCAN: go
steps:
  - run: waybill sbom scan --offline --file-inventory=off --path . --output sbom.cdx.json
```

The env var applies globally to all scans in the workflow. Individual scan invocations can override with `--no-binary-scan=<other>` (once other modes ship in future waybill releases).

## Troubleshooting

### "Error: invalid value 'xyz' for '--no-binary-scan'"

You passed an unrecognized mode. v1 accepts only `go`. Full list emitted by `waybill sbom scan --help`.

### "Error: '--no-binary-scan' requires a value"

Bare `--no-binary-scan` isn't allowed. Add `=<MODE>` (e.g., `--no-binary-scan=go`).

### "My scan still has `pkg:golang/*` components after setting `--no-binary-scan=go`"

Those components came from a source OTHER than binary probing — most likely `pkg:golang/*` from an OS-package reader (dpkg-owned Go binary) or from a `go.mod` source-tree scan (waybill's Go-source reader, m053+, is NOT affected by this flag). Only components derived from `go_binary::finalize`'s BuildInfo probe are suppressed.

## Reference

- Spec: [`spec.md`](./spec.md)
- Plan: [`plan.md`](./plan.md)
- Data model: [`data-model.md`](./data-model.md)
- CLI contract: [`contracts/cli-flag.md`](./contracts/cli-flag.md)
- Perf motivation: [`../664-single-pass-walker/perf-comparison.md §Follow-up backlog`](../664-single-pass-walker/perf-comparison.md)
