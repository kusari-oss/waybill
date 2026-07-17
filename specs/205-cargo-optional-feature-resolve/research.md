# Research: Fix Cargo Optional-Dep Over-Exclusion

**Date**: 2026-07-17
**Purpose**: Resolve 3 mechanical unknowns before task decomposition.

## R1 — Use `cargo metadata --format-version 1` as the resolver source of truth

**Investigation**: Cargo's resolver is the authoritative source of "which packages actually get compiled." Options for accessing it:

1. **Shell out to `cargo metadata --format-version 1`** — the JSON output includes a `resolve` field with per-package `nodes[]` where each node's `deps[]` lists ONLY the deps activated under the resolved feature set. Optional deps that are NOT activated do not appear in any node's `deps[]`. Empirically verified against `/tmp/m205-test`: `[dependencies] serde = { optional = true } + [features] default = ["serde"]` produces `resolve.nodes[root].deps[]` containing `serde` (activated); `[dependencies] regex = { optional = true } + [features] enable-regex = ["regex"]` (not in `default`) omits `regex` from that list.
2. **Reimplement the resolver in Rust** — parse `[features]` tables + `default = [...]` + implicit features (Cargo 1.60+ auto-`<name> = ["dep:<name>"]`) + workspace feature unification. Requires reproducing Cargo's resolver semantics faithfully; every Cargo release risks divergence.
3. **Use the `cargo` crate as a library** — Cargo publishes its resolver as a library, but the crate has heavy transitive deps (rustc-ap-*, git2, openssl-sys, etc.) that violate Constitution Principle I ("no C toolchain" — `openssl-sys` links against libssl).

**Decision**: Option 1 (shell out to `cargo metadata --format-version 1`). Rationale:

- Cargo's resolver is the ground truth by definition; no faithfulness risk.
- Shell-out precedent already ships in mikebom four times (m053 `git describe`, m055 `go mod graph`, m173 `go mod download`, m203 `helm template`). Reviewer-familiar, no new architecture.
- `cargo` binary is a hard implicit prereq of every mikebom dev + CI env; when absent, FR-004 fallback fires.
- Zero new Cargo deps. `serde_json` (workspace-pervasive) parses the output.

**Alternatives considered + rejected**:

- **Option 2** (reimplement): rejected — Cargo's resolver has edge cases (implicit features, weak-optional `dep?/feature` syntax, workspace `resolver = "1"|"2"` divergence, target-cfg feature gating) that would take a milestone to get right, and mikebom would be forever chasing Cargo's semantics.
- **Option 3** (cargo as library): rejected — Constitution Principle I (`openssl-sys` C dependency). Also adds >100MB of transitive Cargo internals to the mikebom binary.

**Concrete cargo metadata shape verified** (local test at `/tmp/m205-test`):

```json
{
  "resolve": {
    "root": "path+file:///private/tmp/m205-test#0.1.0",
    "nodes": [
      {
        "id": "path+file:///private/tmp/m205-test#0.1.0",
        "features": ["default", "serde"],
        "deps": [
          {"name": "serde", "pkg": "registry+…#serde@1.0.228"}
        ]
      },
      { /* transitive nodes */ }
    ]
  }
}
```

`regex` (also `optional = true`, not in `default`) does NOT appear in root's `deps` — correct.

**References**:
- Cargo docs — `cargo metadata` at https://doc.rust-lang.org/cargo/commands/cargo-metadata.html
- Cargo docs — Features 2.0 & implicit features at https://doc.rust-lang.org/cargo/reference/features.html#optional-dependencies

## R2 — Fallback semantics when `cargo metadata` fails

**Investigation**: FR-004 mandates fallback + WARN, but the semantic of the fallback matters. Two options:

- **Fallback A: keep pre-m205 behavior** — treat name-only match as Optional (the current buggy behavior). Zero regression risk vs alpha.63.
- **Fallback B: treat ALL optional deps as Runtime** — safe over-inclusion. Vuln-scanners see every dep; risk of over-reporting vulns for actually-not-shipped deps.

**Decision**: Fallback B (safe over-inclusion). Rationale:

- The bug being fixed is under-reporting of vulnerabilities. Fallback A preserves the under-reporting; Fallback B reverses it (over-reporting is the recoverable direction — the operator can inspect and dismiss false positives, but silently-missed vulns are catastrophic).
- Constitution Principle III (fail-closed) and Principle IX (accuracy over fabrication) both point to over-inclusion as the safe default: an SBOM that includes deps the actual build doesn't ship is inaccurate-but-scannable; an SBOM that silently drops shipped deps is inaccurate-and-blind.
- Downstream vuln-scanner UX: a scanner reporting extra vulns is a false-positive triage burden (fixable). A scanner silently missing vulns is a security incident (unfixable without independent audit).
- The WARN log makes the fallback observable — operators can install cargo to get the precise answer.

**Alternatives considered + rejected**:
- **Fallback A**: rejected per the reasoning above — preserving the bug in fallback mode is worse than a slight over-inclusion that vuln-scanners can triage.
- **Silent fallback** (no WARN): rejected — violates Constitution Principle X (transparency); operator has no way to know they're on the reduced-fidelity path.
- **Hard error / abort scan**: rejected — violates Constitution Principle III (fail-closed spirit is "graceful degradation with signal", not "block the operator from getting a scan result"); operators without cargo (some CI environments) would be unable to scan Rust code at all.

**WARN log shape** (matches m203 pattern):

```rust
tracing::warn!(
    workspace = %ws_root.display(),
    reason = %failure,
    "cargo metadata failed; falling back to name-only optional classification \
     (safe over-inclusion — deps marked Runtime instead of Optional)"
);
```

## R3 — Subprocess pattern + timeout choice

**Investigation**: mikebom's subprocess-with-timeout pattern is documented in m055 (`golang/go_mod_graph.rs:81-158`) and reused verbatim by m173 (warm-go-cache) + m203 (helm template). Structure: `thread::spawn(move || tx.send(Command::new(...).output()))` + `mpsc::channel` + `rx.recv_timeout(Duration::from_secs(N))`. On timeout the worker thread + subprocess get reaped eventually.

**Decision**: Reuse the pattern verbatim for `resolve_activated_deps_via_cargo_metadata`. Timeout = 60 seconds (matches m203 helm-render default; `cargo metadata` on `test-vaultwarden` takes ~2s so 60s is generous). Env var override `MIKEBOM_CARGO_METADATA_TIMEOUT_SECS` for pathological monorepos (Linux kernel-scale workspaces might exceed 60s cold-cache). Clamp `[1, 3600]` per m203 precedent.

**Rationale**: Zero new subprocess architecture. Four existing sites prove the pattern is stable + reviewer-familiar.

**Alternatives considered + rejected**:
- **No timeout** (block indefinitely): rejected — a wedged cargo (git-registry-lookup blocked on stale lock, etc.) would hang mikebom forever.
- **Async tokio::process::Command**: rejected — mikebom's readers are synchronous today; cascading async through the cargo reader would touch dozens of files. m055's synchronous pattern is deliberate.

**References**:
- `mikebom-cli/src/scan_fs/package_db/golang/go_mod_graph.rs:81-158` — canonical pattern.
- `mikebom-cli/src/scan_fs/package_db/helm.rs::extract_image_refs_rendered` (m203) — the most recent reuse.

## Decision Summary

| Decision | Chosen | Alternative | Rationale |
|---|---|---|---|
| Resolver source | `cargo metadata --format-version 1` shell-out | Reimplement Cargo resolver / use cargo-as-library | Ground truth; existing subprocess precedent (4×); no C deps |
| Fallback semantic | Safe over-inclusion (all optional → Runtime) with WARN | Preserve pre-m205 name-only classification / hard error | Under-reporting vulns is unrecoverable; over-reporting is triage-able; Constitution III + IX align |
| Subprocess pattern | m055 verbatim (thread + mpsc + recv_timeout, 60s default, `MIKEBOM_CARGO_METADATA_TIMEOUT_SECS` override, `[1, 3600]` clamp) | tokio async / no timeout | Zero new architecture; existing pattern is stable |
| New Cargo deps | Zero | (n/a) | Nothing needed |
