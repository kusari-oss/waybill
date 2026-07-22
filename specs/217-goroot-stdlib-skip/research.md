# Phase 0 Research: Skip GOROOT stdlib as Go main-module

**Feature**: 217-goroot-stdlib-skip
**Date**: 2026-07-22

## R1 — Detection signal: module-path check vs install-path heuristic

**Decision**: Detect a toolchain-internal `go.mod` by parsing its `module` line and checking for exactly the strings `"std"` or `"cmd"`. Do NOT hardcode install paths (`/usr/local/go`, `/opt/go`, etc.).

**Rationale**:
- The Go toolchain has been shipping `$GOROOT/src/go.mod` (declaring `module std`) and `$GOROOT/src/cmd/go.mod` (declaring `module cmd`) since Go 1.18. These are the ONLY two `module` names the toolchain reserves for its own boundaries. Every user-published module uses a DNS-based path (`example.com/x`, `github.com/user/repo`, etc.). Purl-spec `pkg:golang/` PURL-conformance requires the same — a bare `std` isn't a valid registry-derivable identity.
- Install locations vary widely: `/usr/local/go` (official installer), `/opt/go` (some distros), `/opt/homebrew/opt/go/libexec` (Homebrew on macOS), `$HOME/sdk/go1.22.5/` (asdf/toolchain-manager layouts), `/usr/lib/go-1.22/` (Debian packages), etc. A hardcoded install-path list would be brittle and would leak into the SBOM as a documentation gap ("what about install path X?").
- The `parse_go_mod` helper at `legacy.rs:188` already extracts `module_path: Option<String>` from `go.mod` text; the check is a single `str::eq` after existing work.

**Alternatives considered**:
- **Install-path allowlist**: hardcode `/usr/local/go`, `/opt/go`, etc. REJECTED — brittle, doesn't generalize, requires ongoing maintenance as new distros/managers appear.
- **VERSION-file sibling detection**: check if `<dir>/../VERSION` exists (a signature of GOROOT). Extra I/O; the module-path signal already covers every case with less code.
- **`GOROOT` env-var comparison**: scan the process env for `$GOROOT` and skip anything under that path. REJECTED — waybill scans rootfs contents, not the host filesystem; `$GOROOT` at scan time refers to the SCANNING HOST'S Go install, not the SCANNED IMAGE'S.
- **Content-fingerprint** (SHA-256 of the go.mod matching a known toolchain-issued value): too rigid across Go versions.

## R2 — Filter placement: walker vs emit-time

**Decision**: Place the filter at the walker (`candidate_project_roots` at `legacy.rs:2487`), immediately after the existing `path.join("go.mod").is_file()` check. Read the go.mod at walker time (previously read at emission time), parse it once, and skip the directory if `module_path == Some("std") || module_path == Some("cmd")`.

**Rationale**:
- Skipping at the walker means the downstream pipeline (main-module construction, `go list all` preflight, `go mod why` analysis) NEVER runs against GOROOT/src. That kills the reported stderr flood + false-positive main-module in one place.
- Skipping at emission time (after PackageDbEntry construction) would still incur the `go list` failure cost and the stderr dump would still leak — the fix would eliminate the SBOM correctness issue but not the CI-log-noise issue.
- The walker already reads directory contents (`path.is_dir() && path.join("go.mod").is_file()`); one additional `std::fs::read_to_string` per candidate is proportional to the existing per-directory work.

**Alternatives considered**:
- **Skip after `go list` fails**: pattern-match the stderr for "use of internal package" and drop the entry. REJECTED — masks the real design intent (the stdlib source tree isn't a user project) and leaves the `go list` cost on the critical path.
- **Move the check into `parse_go_mod`**: e.g., return `None` from `parse_go_mod` when the module is `std`/`cmd`. REJECTED — `parse_go_mod` is used elsewhere (upstream `go.mod` fetches per m055 proxy path, etc.) and should stay format-parsing-only; the go/no-go decision belongs at the caller.

## R3 — Companion annotation shape (P2)

**Decision**: When the walker skips a toolchain-internal `go.mod`, record the DETECTED TOOLCHAIN ROOT (defined as: the great-grandparent of the go.mod — `go.mod` is at `$GOROOT/src/go.mod` or `$GOROOT/src/cmd/go.mod`, so the toolchain root is `<go.mod>.parent().parent()` for `module std` and `<go.mod>.parent().parent().parent()` for `module cmd`). Accumulate these in a `Vec<PathBuf>`, sort lex + dedup at walker exit, and thread the result up to `ScanArtifacts.go_toolchains_detected: Option<&'a [PathBuf]>`.

Emitted annotation value: `waybill:go-toolchain-detected = "["<path1>","<path2>",...]"` — a JSON-encoded array of scan-root-relative path strings, sorted lex, dedup. Matches the C121 `waybill:workspaces-detected` shape (JSON-array-in-string; single-detection scans emit a 1-element array; consumers do `.value | fromjson | contains(["path"])` for filtering).

**Rationale**:
- Consistency with the m176 C121 precedent means consumers can reuse the same jq filters + parser code.
- Great-grandparent-of-go.mod correctly identifies `$GOROOT` (not `$GOROOT/src`) because `$GOROOT/src` is a filesystem artifact of the toolchain layout, not the meaningful install root operators reason about.
- Path is scan-root-relative when possible (falls back to absolute if the go.mod is outside scan_root somehow) — matches the m216 `source_dir` normalization approach.

**Alternatives considered**:
- **Per-detection annotation** (one annotation per detected toolchain): duplicates the C121 aggregation convention; harder to consume.
- **Value = go.mod path** (not toolchain root): less useful to operators. `$GOROOT` is the mental model everyone uses for Go install locations; `$GOROOT/src/go.mod` is a plumbing detail.
- **Value = detected Go version** (parsed from `$GOROOT/VERSION`): scope creep. This spec fixes the walker; version reporting is a related-but-separate feature (would need PURL construction for the synthetic component too, per spec's Out of Scope section).

## R4 — Backwards-compat guarantee mechanism

**Decision**: The pre-feature `{cdx,spdx,spdx3}_regression` byte-identity tests continue to run on every existing fixture unchanged. None of those fixtures contain a `module std` or `module cmd` go.mod (verified by grepping `waybill-cli/tests/fixtures/**/go.mod` for the exact strings), so the filter never fires on any existing fixture, so no output drifts. SC-004 gate enforces this at CI time.

**Rationale**:
- The filter's trigger condition is exact-string-match on two toolchain-reserved module names. Any user fixture would use a DNS-based module path and be untouched.
- Explicitly grepping the fixture tree pre-implementation gives a strong prior on non-regression.
- Byte-identity tests catch subtle unintended emissions (annotation ordering, property inclusion, etc.) — passing them = no invisible drift.

**Verification approach**: pre-implement grep + post-implement CI check. Documented in Phase 1 quickstart.

## R5 — Fixture strategy

**Decision**: Author `waybill-cli/tests/fixtures/goroot_stub/` with the following minimal layout:

```
goroot_stub/
├── usr/
│   └── local/
│       └── go/
│           ├── VERSION           # "go1.26.3\ntime 2026-05-04T20:36:18Z\n"
│           └── src/
│               ├── go.mod        # "module std\n\ngo 1.26\n"
│               └── cmd/
│                   └── go.mod    # "module cmd\n\ngo 1.26\n"
└── app/                          # sibling user project (FR-004 non-regression)
    ├── go.mod                    # "module example.com/app\n\ngo 1.22\n"
    ├── go.sum                    # empty
    └── main.go                   # `package main; func main() {}`
```

The `VERSION` file mirrors a real Go 1.26 install (verified against the local Homebrew installation). The user project is a minimal `main.go` so the go reader's downstream emission works normally on it.

**Rationale**:
- Fixture is <100 lines total, single-purpose, reviewable in one screen.
- Mirrors both the `$GOROOT/src/go.mod` (module std) and `$GOROOT/src/cmd/go.mod` (module cmd) cases in one fixture (proves FR-002 covers both).
- Includes a user-project sibling to prove FR-004 non-regression (the app's main-module must STILL be emitted while stdlib/cmd are skipped).
- No `.git` directory anywhere → the m053 git-describe ladder falls back to `v0.0.0-unknown` for the user project, keeping the fixture deterministic across CI runs.

**Alternatives considered**:
- **Full Go toolchain checkout as fixture**: too large (GBs); breaks the pre-m090 fixture-cache convention.
- **Fixture with only stdlib go.mod (no user project)**: doesn't prove FR-004 non-regression; requires a separate second fixture for the user-project case. Combining them into one is cheaper.

## R6 — Constitution V audit for the new annotation

**Decision**: `waybill:go-toolchain-detected` is a genuinely new signal. No format-native equivalent exists:

- **CycloneDX 1.6**: no document-scope "toolchain observation" field. `metadata.tools[]` describes tools that PRODUCED the SBOM (waybill itself), not tools OBSERVED in the scanned rootfs. Not equivalent.
- **SPDX 2.3**: no document-scope field for observed-but-not-emitted toolchains. `creationInfo.creators[]` is again about the producer.
- **SPDX 3.0.1**: no `SpdxDocument.observedToolchains` or similar. `Element.type` values are per-component classifications, not document-scope observation records.

The annotation is parity-bridging under the KEEP-NO-NATIVE audit polarity (same as C121, C132, C133, C134). Documented in `docs/reference/sbom-format-mapping.md` per Constitution Principle V requirement.

**Rationale**: no reinvention of an existing native construct. The audit-cite text lands in the C136 row alongside the C121-C134 precedents.

## R7 — Log-level choice

**Decision**: `tracing::debug!` for the "skipped toolchain-internal go.mod" line. Not `info!`, not `warn!`.

**Rationale**:
- `warn!` is wrong: this isn't a warning condition — waybill is doing the right thing when it skips.
- `info!` is too spammy: any Go-heavy image scan would produce multiple `info` lines per invocation (once per `cmd/go.mod`, `src/go.mod`, and any nested std-family go.mods like `$GOROOT/src/debug/buildinfo/testdata/go117/go.mod` under `testdata/` — though those are already skipped by `should_skip_descent`).
- `debug!` matches the m053 precedent for "boring negative decisions" in the golang walker.

## R8 — Documentation surface

**Decision**: 3 doc touch-points, minimally invasive:

1. `docs/reference/sbom-format-mapping.md` — new C136 catalog row for `waybill:go-toolchain-detected` (Constitution V requirement).
2. No update to `docs/user-guide/cli-reference.md` — this is not a new CLI surface; it's a walker-behavior fix.
3. No update to `docs/user-guide/split-manifest.md` — the annotation is on the whole-scan SBOM, not on split-mode sub-SBOMs (though it would flow through m215's `ScanArtifacts::narrow` if a split-mode scan of a Go-toolchain-carrying image ran; that's covered without any doc addition).

**Rationale**: minimal doc surface = minimal doc rot. The bug fix is the interesting part; the annotation is one row of context.
