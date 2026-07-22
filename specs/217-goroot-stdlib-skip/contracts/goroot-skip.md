# Contract: GOROOT-stdlib skip predicate + go-toolchain-detected annotation

**Feature**: 217-goroot-stdlib-skip
**Kind**: Reader-behavior contract (walker predicate) + document-scope annotation shape
**Consumers**: every downstream Go-reader path (main-module emission, `go list all` preflight, `go mod why` analysis); every SBOM emitter (CDX / SPDX 2.3 / SPDX 3); operators inspecting emitted SBOMs.

## Filter predicate

The Go rootfs walker (`waybill-cli/src/scan_fs/package_db/golang/legacy.rs::candidate_project_roots`) MUST skip any directory `D` if ALL of:

1. `D/go.mod` exists (pre-existing walker predicate — unchanged).
2. Reading `D/go.mod` succeeds AND parsing yields `module_path == Some("std")` OR `module_path == Some("cmd")` (case-sensitive; whitespace trimmed).

"Skip" means: `D` is NOT added to the returned candidates list. Downstream `read()`, `go list all` preflight, `go mod why` analysis, and PackageDbEntry construction never observe `D`.

**Non-blocking failure mode**: if reading or parsing `D/go.mod` fails for any reason (I/O error, malformed content, encoding issue), the filter falls through to the pre-existing behavior (add `D` to candidates and let downstream code handle it). Rationale: the filter is an optimization, not a validator; genuinely broken go.mods should surface as pre-feature errors.

## Toolchain-root derivation

When the filter fires with `module_path == Some("std")`:
- Toolchain root = `D.parent()` (equivalently, `<go.mod>.parent().parent()`).
- Example: `<rootfs>/usr/local/go/src/go.mod` → toolchain root `<rootfs>/usr/local/go`.

When the filter fires with `module_path == Some("cmd")`:
- Toolchain root = `D.parent().parent()` (equivalently, `<go.mod>.parent().parent().parent()`).
- Example: `<rootfs>/usr/local/go/src/cmd/go.mod` → toolchain root `<rootfs>/usr/local/go`.

Each derivation is guarded — if any `.parent()` step returns `None` (pathological), the accumulator entry is skipped for that observation but the skip decision itself still fires.

## Aggregation

The walker returns (`Vec<PathBuf> candidates`, `Vec<PathBuf> toolchain_roots`) where `toolchain_roots` is:
- Populated inside the walker callback per the derivation rules above
- Sorted lexicographically at walker exit
- Deduplicated at walker exit (a scan with both `module std` and `module cmd` inside `$GOROOT/src/` yields ONE entry: `$GOROOT`)

## Threading

`toolchain_roots` flows from the walker → `read()` return value → `scan_path()` local → `ScanArtifacts.go_toolchains_detected: Option<&'a [PathBuf]>`:
- `None` when the vec is empty (no toolchain observed)
- `Some(&vec)` when the vec has ≥ 1 entry

## Annotation contract

When `ScanArtifacts.go_toolchains_detected.is_some()`, every emitter MUST emit a document-scope `waybill:go-toolchain-detected` annotation.

**CDX 1.6 shape** (added to `metadata.properties[]` by the m216-precedent-following block in `cyclonedx/metadata.rs`):
```json
{"name": "waybill:go-toolchain-detected", "value": "[\"usr/local/go\"]"}
```

**SPDX 2.3 shape** (added to document-level annotations on `SPDXRef-DOCUMENT` via the `MikebomAnnotationCommentV1` envelope):
```json
{
  "annotationType": "OTHER",
  "annotator": "Tool: waybill",
  "annotationDate": "<created>",
  "comment": "{\"schema\":\"waybill-annotation/v1\",\"field\":\"waybill:go-toolchain-detected\",\"value\":\"[\\\"usr/local/go\\\"]\"}"
}
```

**SPDX 3.0.1 shape**: document-scope `Annotation` element on the `SpdxDocument` root IRI; same envelope shape (matches the C121, C132, C133, C134 precedents).

**Value regex**: `^\[(\"[^\"]+\"(,\"[^\"]+\")*)?\]$` — non-empty when emitted (empty array MUST NOT be emitted; absence of annotation is the wire-visible signal for "no toolchain observed").

## Path normalization

Toolchain-root paths in the annotation value are:
- Scan-root-relative when the toolchain root is under `scan_root` (typical `--path` and `--image` cases). Example: scan_root = `/tmp/waybill-image-abc/rootfs`; toolchain root = `/tmp/waybill-image-abc/rootfs/usr/local/go`; annotation value contains `"usr/local/go"`.
- Absolute path when the toolchain root is outside `scan_root` (edge case; e.g., a scanning-host path leaked into the walker somehow). Normalization mirrors the m176 workspace-detected path normalization.

## Backwards-compat guarantees

- **Non-Go scans**: NO change. Walker never observes a `go.mod`, filter never fires.
- **User-Go-project scans (no toolchain in rootfs)**: NO change. Every user go.mod has a non-`std`/non-`cmd` module path, filter never fires, `go_toolchains_detected` stays `None`, annotation absent.
- **User-Go-project + toolchain in same rootfs**: filter fires ONLY on the toolchain-internal go.mods. User project's main-module IS still emitted. Annotation is present. Enforced by acceptance scenario US1 AS #3.

**Pre-feature regression tests** (`cdx_regression`, `spdx_regression`, `spdx3_regression`, `transitive_parity_gem`, etc.) MUST continue to pass byte-identically — none of the existing fixtures contain a `module std` or `module cmd` go.mod (SC-004 gate at CI time).

## Log-level contract

The walker MUST emit a `tracing::debug!` log line each time the filter fires:
```
DEBUG waybill::scan_fs::package_db::golang::legacy: gorooted-{stdlib|cmd} go.mod skipped (waybill#631) path=<go.mod path> module=<std|cmd>
```

NOT `warn!` (this isn't a warning — waybill is doing the right thing). NOT `info!` (would spam on Go-toolchain-heavy repos).

## Consumer-side semantics

**SBOM viewer tools**: no `pkg:golang/std@*` main-module appears in the emitted SBOM. If the operator wants to see "did waybill observe a Go toolchain in this scan?", they inspect the document-scope `waybill:go-toolchain-detected` annotation.

**Vuln scanners keyed on Go stdlib CVEs**: consumers wanting to cross-reference against Go stdlib CVEs need to (a) infer the Go version from other signals (Go build info on emitted binaries per m003, `VERSION` file if surfaced by future work), and (b) apply their own advisory-DB lookups. The annotation surfaces the observation; interpretation is the consumer's.

**Downstream consumers of the SBOM's main-module identity**: post-feature, images with only a Go toolchain and no user Go project will fall through the m127 root-selector ladder to `synthetic-placeholder` (as they did pre-existence-of-`$GOROOT/src/go.mod` — pre-1.18 behavior restored, effectively). The `waybill:go-toolchain-detected` annotation tells them the reason.

## Contract stability

- **Filter predicate** — tightening (adding more required conditions) is breaking; loosening (skipping fewer directories) is non-breaking.
- **Reserved module names** (`std`, `cmd`) — the list is fixed at v1. Adding a new reserved name would be a breaking change (previously-user-project directories become skipped) and requires spec + migration.
- **Annotation name** (`waybill:go-toolchain-detected`) — public; renaming requires MAJOR constitution amendment.
- **Value format** (JSON-array-of-strings) — matches C121; changes to the shape are breaking.
- **Toolchain-root derivation rule** — public; changing from parent-of-src to something else (e.g., dirname of go.mod) would break consumers that key on `$GOROOT`-shaped paths.
