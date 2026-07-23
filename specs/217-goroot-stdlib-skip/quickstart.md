# Quickstart: GOROOT-stdlib skip verification

**Feature**: 217-goroot-stdlib-skip
**Date**: 2026-07-22

Verification recipe for the waybill#631 fix.

## The 30-second happy path

Any container image that ships a Go toolchain at `/usr/local/go/` (or another install location) previously produced a noisy scan. Post-fix:

```bash
waybill sbom scan --image cgr.dev/chainguard/wolfi-base:latest --format cyclonedx-json --output /tmp/x.cdx.json 2>&1 | tee /tmp/x-scan.log

# Pre-fix: log contained ~180 `Error: <path>:<line>: use of internal package X not allowed` lines
# Post-fix: log contains ZERO such lines from waybill itself
grep -c "use of internal package" /tmp/x-scan.log
# → 0
```

## Confirming the SBOM correctness fix

```bash
# Pre-fix SBOMs contained a spurious pkg:golang/std@* main-module component.
# Post-fix: zero.
jq '[.components[]? | select(.purl | startswith("pkg:golang/std@"))] | length' /tmp/x.cdx.json
# → 0
jq '[.components[]? | select(.purl | startswith("pkg:golang/cmd@"))] | length' /tmp/x.cdx.json
# → 0

# metadata.component still identifies whatever the whole-scan root PURL is;
# it should NOT be pkg:golang/std or pkg:golang/cmd.
jq '.metadata.component.purl' /tmp/x.cdx.json
```

## Confirming the transparency annotation (P2)

When waybill detects a Go toolchain, a document-scope annotation surfaces the observation:

```bash
jq '.metadata.properties[]? | select(.name == "waybill:go-toolchain-detected")' /tmp/x.cdx.json
# On an image with Go at /usr/local/go, expect:
# {
#   "name": "waybill:go-toolchain-detected",
#   "value": "[\"usr/local/go\"]"
# }
```

Multiple toolchains in one rootfs (multi-stage Docker build oddity) yield a JSON array with multiple entries:

```json
{
  "name": "waybill:go-toolchain-detected",
  "value": "[\"opt/go\",\"usr/local/go\"]"
}
```

Path values are scan-root-relative. Array is sorted lex + deduplicated.

## Confirming FR-004 non-regression

Scan a repo with BOTH a real user Go project AND a Go toolchain (typical Docker builder-image layout):

```bash
waybill sbom scan --path /path/to/mixed-fixture --format cyclonedx-json --output /tmp/mixed.cdx.json

# The user project's main-module IS emitted.
jq '.metadata.component | {name, purl}' /tmp/mixed.cdx.json
# → {"name": "example.com/app", "purl": "pkg:golang/example.com/app@v0.0.0-unknown"}

# NO stdlib pseudo-main-module.
jq '[.components[]? | select(.purl | test("^pkg:golang/(std|cmd)@"))] | length' /tmp/mixed.cdx.json
# → 0
```

## Reproducer for the waybill#631 CI-noise fix

The bug reported in waybill#631 fires on any CI job scanning a Go-toolchain-carrying image. The `##[error]` annotation count on the workflow run is the visible symptom.

Post-fix, running:

```bash
# In a GitHub Actions workflow — image is a Go-toolchain-carrying rootfs
waybill sbom scan --image "$IMAGE" --format cyclonedx-json --output sbom.cdx.json
```

… produces:

- Zero `##[error]` annotations from waybill (was ~10-190 pre-fix depending on how many lines the Go problem-matcher regex matched)
- Zero `pkg:golang/std@*` components in the emitted SBOM (was 1)
- Zero `pkg:golang/cmd@*` components (was 0 or 1 depending on whether the Go install shipped `src/cmd/go.mod`)
- One `waybill:go-toolchain-detected = "[\"<goroot-path>\"]"` document-scope annotation surfacing the observation

## Verification checklist

After a scan of a Go-toolchain-carrying image:

```bash
# 1. Zero stderr flood from waybill's Go reader.
grep -cE "use of internal package .* not allowed" scan.log
# → 0

# 2. Zero pkg:golang/std or pkg:golang/cmd components.
jq '[.components[]? | select(.purl | test("^pkg:golang/(std|cmd)@"))] | length' out.cdx.json
# → 0

# 3. Toolchain-observation annotation present.
jq -e '.metadata.properties[]? | select(.name == "waybill:go-toolchain-detected")' out.cdx.json
# → JSON entry (exit 0)

# 4. User Go projects still emit main-modules (if the rootfs has one).
jq '.metadata.component.purl' out.cdx.json
# → non-empty; not pkg:golang/std, not pkg:golang/cmd, not pkg:generic/<rootname>@0.0.0 (synth-placeholder)
```

## Rollback / opt-out

No opt-out flag. The filter is unconditional whenever the walker observes a `module std` or `module cmd` go.mod. Rationale: those two module names are toolchain-reserved and no legitimate user project uses them (spec Assumptions section). If a downstream consumer breaks because they were previously depending on the presence of the `pkg:golang/std` pseudo-main-module in waybill's output, that's a consumer-side bug — the pre-feature emission was itself incorrect.
