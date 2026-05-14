# Research — milestone 101 Windows smoke test + experimental docs callout

Phase 0 research. All Technical Context unknowns from `plan.md` are resolved here. Per the constitution, the goal is to land decisions in writing before any code or doc churn.

## §1 — How to invoke the locally-built `mikebom.exe` from an integration test

**Decision**: use `env!("CARGO_BIN_EXE_mikebom")` to resolve the absolute path of the test-build binary.

**Rationale**: Cargo sets `CARGO_BIN_EXE_<bin-name>` automatically for any `tests/*.rs` integration test in a crate that has a `[[bin]]` target. `mikebom-cli/Cargo.toml` declares `[[bin] name = "mikebom" path = "src/main.rs"]` (verified at line 7-9 of that file). The env var resolves at test compile-time to the absolute path of the `mikebom` (or `mikebom.exe` on Windows) artifact under `target/<profile>/`. This is the standard cargo integration-test pattern; no helper crate (`assert_cmd`, `escargot`) needed — keeps FR-009's "zero new Cargo deps" constraint.

**Alternatives considered**:
- `cargo run -p mikebom --` — works but spawns a fresh cargo subprocess, doubling test time (cargo-resolve overhead even on a warm target tree).
- `assert_cmd` crate — convenient but new dep; rejected per FR-009.
- Hard-coded `target/debug/mikebom.exe` — fragile across `--release`, custom `CARGO_TARGET_DIR`, etc.

**Used by**: `mikebom-cli/tests/scan_polyglot_monorepo.rs` already uses this pattern (line ~30: `Command::new(env!("CARGO_BIN_EXE_mikebom"))`); we mirror it.

## §2 — Cross-platform 60-second subprocess timeout without `wait-timeout`

**Decision**: spawn `mikebom.exe` via `Command::spawn()` → `Child`; in a dedicated thread call `Child::kill()` after a 60-second `std::thread::sleep()`; in the main thread `Child::wait()` and check exit status. On timeout: the kill-thread fires first, `wait()` returns immediately with a non-zero status, and the test asserts the timeout by checking elapsed time `> 60s`.

**Rationale**: `std::process::Child` does NOT expose a portable `wait_timeout()` API in stable Rust (`wait_timeout` crate exists, but adding it violates FR-009). The spawn-kill-thread approach is std-only, cross-platform (works the same on Windows and Unix because `Child::kill()` calls `TerminateProcess` on Windows / `SIGKILL` on Unix), and well-understood. The 60-second margin (per spec Clarification Q2) gives 12× headroom over the typical <5s scan runtime — false positives are very unlikely.

**Alternatives considered**:
- `wait-timeout` crate — clean API but new dep; rejected per FR-009.
- `tokio` async + `tokio::time::timeout` — requires async runtime in integration test; heavyweight for one timeout.
- `Child::try_wait()` poll loop with sleep — works but consumes CPU and adds polling latency; spawn-kill-thread is simpler.

**Implementation sketch** (will land in `data-model.md §scan_windows_smoke.rs`):
```rust
let mut child = Command::new(env!("CARGO_BIN_EXE_mikebom"))
    .args([...])
    .spawn()?;
let timeout_handle = {
    let child_id = child.id();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(60));
        // Best-effort: if child still running, kill by pid.
        let _ = std::process::Command::new(if cfg!(windows) { "taskkill" } else { "kill" })
            .args(if cfg!(windows) { &["/F", "/PID", &child_id.to_string()] } else { &["-9", &child_id.to_string()] })
            .status();
    })
};
let start = std::time::Instant::now();
let status = child.wait()?;
let elapsed = start.elapsed();
let _ = timeout_handle.join(); // best-effort cleanup
if elapsed > std::time::Duration::from_secs(58) {
    panic!("scan timed out — likely hang regression (elapsed: {:?})", elapsed);
}
```
(Refined in data-model. The 58-second threshold accounts for race between kill thread firing and `Child::wait()` returning.)

## §3 — Two fixtures: cargo `lockfile-v3/` + `polyglot-monorepo/`

**Decision**: use the two fixtures verbatim from `<MIKEBOM_FIXTURES_DIR>/cargo/lockfile-v3/` and `<MIKEBOM_FIXTURES_DIR>/polyglot-monorepo/`.

**Rationale**: Both fixtures are pre-vendored, already exercised by existing tests (`cdx_regression_cargo`, `scan_polyglot_monorepo`), and cached on the Windows runner via milestone 090's fixture cache. Reusing them keeps FR-009 (no new fixtures) and confirms the Windows binary behaves consistently with Linux/macOS for the same input.

**Fixture content** (verified via existing tests):
- `cargo/lockfile-v3/` — contains a `Cargo.lock` with `anyhow`, `my-app` (root crate); cdx_regression_cargo asserts ≥1 `pkg:cargo/` component, the smoke test does the same.
- `polyglot-monorepo/` — python (pip) + npm tree; scan_polyglot_monorepo asserts `pkg:pypi/` AND `pkg:npm/` components present (names like `react`, `axios`, `frontend`).

**Alternatives considered**:
- New Windows-specific fixture — adds maintenance, no value vs reusing existing.
- Synthesize fixture in-test from string-literal `Cargo.toml` — possible but more code; existing fixture loads faster (cache-hit).

## §4 — Backslash check: scoped to path-shaped fields only

**Decision**: walk the emitted SBOM as `serde_json::Value`; for every JSON property whose `name` matches the regex `mikebom:source-files|mikebom:source-path|location`, recursively descend into the `value` and assert no embedded `\` character.

**Rationale**: Milestone 100 surfaced a real bug where blanket `raw.replace('\\', '/')` mangled CPE 2.3 escape sequences (`cpe:2.3:a:github.com\/foo` → `github.com///foo`). The smoke test's backslash check must be SCOPED to path-shaped fields, not blanket-scan the whole JSON. The three field names above are the canonical path-shaped emission sites from milestone 100's data-model §emission-sites: `mikebom:source-files` (component property), `mikebom:source-path` (component property, milestone 100 chokepoint), `location` (CDX evidence.occurrences[].location, SPDX 2.3 annotation occurrences, SPDX 3 statement occurrences).

**Alternatives considered**:
- Blanket-scan whole JSON for `\` — false-positives on CPE strings (the iteration-6 lesson).
- Scan only string values matching `^/` or `^[A-Z]:/` (path-shape heuristic) — fragile, misses relative paths.
- Use a regex to look for `\` followed by a path-shape continuation — overengineered for the smoke-test scope.

## §5 — CI YAML split shape

**Decision**: replace the existing single `Tests` step with TWO steps in `lint-and-test-windows`:

```yaml
      - name: Smoke test (blocking — milestone 101)
        run: cargo +stable test --test scan_windows_smoke

      - name: Tests (non-blocking, see issue #210)
        continue-on-error: true
        run: cargo +stable test --workspace
```

The smoke step has NO `continue-on-error:` — failures block the merge. The broader workspace step keeps milestone 100's `continue-on-error: true` so the #210 backlog stays visible but non-blocking.

**Rationale**: Cargo test's `--test <name>` filters to a single integration-test binary, fast (~30 sec post-build on Windows including cargo's incremental link). The blocking step gives the gate FR-008 requires. The non-blocking workspace step preserves visibility per milestone 100's descope rationale.

**Sequencing note**: the smoke step MUST run BEFORE the workspace step. If the workspace step ran first and failed (and it will, per the #210 backlog), `continue-on-error: true` lets the lane continue to the smoke step — but the smoke step would have to compile the test crate fresh (cargo-incremental does help). Listing smoke first means it runs against a warm cargo cache from the clippy step's compilation, minimizing total CI time.

**Alternatives considered**:
- Single step with all tests, `continue-on-error: true` — fails FR-008 (no blocking gate).
- Single blocking step running everything — fails milestone 100's descope (full workspace tests will fail).
- Three steps (smoke / workspace / cleanup) — overengineered.

## §6 — Docs callout placement + wording

**Decision**: README.md gets a `> ⚠️ **Experimental** (milestone 100, follow-up [#210](https://github.com/kusari-sandbox/mikebom/issues/210))` block-quote callout INSIDE the existing "Windows install" subsection (between the heading and the download instructions). The platform-support table's Windows row's cell text changes from `✅ supported (milestone 100)` to `🧪 experimental (milestone 100)`. `docs/user-guide/installation.md` gets the same blockquote callout with consistent wording.

**Rationale**: The user's stated phrasing was "🧪 experimental" — kept verbatim for emoji consistency with the table cell. Blockquote (`>`) renders as a visually distinct callout in GitHub's markdown view AND in most other renderers. Linking `#210` by number (not URL) keeps the markdown source readable; GitHub auto-links.

**Wording** (canonical, used in both files):
```markdown
> 🧪 **Experimental.** Windows builds are available as of milestone 100,
> but are not feature-equivalent to Linux/macOS yet. Known gaps include:
> Linux-only OS package readers (dpkg/rpm/apk), HOME-env-var-derived
> cache paths, OCI image cache atomic-rename, path-resolver pattern
> matcher, and Python stdlib collapse. Full Windows runtime test parity
> + production-code fixes are tracked in [#210](https://github.com/kusari-sandbox/mikebom/issues/210).
> Do not rely on the Windows binary for production SBOM workflows
> until #210 closes.
```

**Alternatives considered**:
- "Alpha" wording — vaguer, less specific about what's wrong.
- "Preview" — implies near-completion; not accurate given #210's open scope.
- "Beta" — implies feature-complete + bug-finding mode; doesn't match the known-gap reality.
- "Experimental" — matches the actual state and the user's request.

## §7 — Path-shaped-field detection algorithm

**Decision**: implement a recursive `walk_for_backslash_in_path_fields(value: &serde_json::Value, path_field_names: &[&str]) -> Vec<(String, String)>` helper inside `scan_windows_smoke.rs`. The first arg is the JSON value; the second is the canonical path-field name list `["mikebom:source-files", "mikebom:source-path", "location"]`. Returns a `Vec<(field_name, offending_value)>` of every (field, string-value) pair where the value contains `\`. On non-empty return, the test fails with the offending values printed.

**Rationale**: Recursive walk handles both flat CDX (`components[].properties[]`) and nested SPDX 3 (`@graph[]` with sub-elements). The path-field name list is small and stable across formats. The function is ~30 lines, no new dep, easy to test.

**Implementation sketch**:
```rust
fn walk_for_backslash_in_path_fields(
    val: &serde_json::Value,
    path_field_names: &[&str],
) -> Vec<(String, String)> {
    let mut hits = Vec::new();
    walk_inner(val, path_field_names, "", &mut hits);
    hits
}

fn walk_inner(
    val: &serde_json::Value,
    path_field_names: &[&str],
    parent_key: &str,
    hits: &mut Vec<(String, String)>,
) {
    match val {
        serde_json::Value::Object(map) => {
            for (k, v) in map {
                // CDX `properties[]` shape: { "name": "mikebom:source-files", "value": "..." }
                if k == "name" {
                    if let Some(name_str) = v.as_str() {
                        if path_field_names.contains(&name_str) {
                            if let Some(value_str) = map.get("value").and_then(|x| x.as_str()) {
                                if value_str.contains('\\') {
                                    hits.push((name_str.to_string(), value_str.to_string()));
                                }
                            }
                        }
                    }
                }
                // Direct `location` field shape (CDX evidence.occurrences[].location)
                if path_field_names.contains(&k.as_str()) {
                    if let Some(value_str) = v.as_str() {
                        if value_str.contains('\\') {
                            hits.push((k.clone(), value_str.to_string()));
                        }
                    }
                }
                walk_inner(v, path_field_names, k, hits);
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr {
                walk_inner(v, path_field_names, parent_key, hits);
            }
        }
        _ => {}
    }
}
```

**Alternatives considered**:
- Stringify entire JSON and regex-search for `"\\\\.*?[A-Za-z]"` — false-positives on CPE strings (the iteration-6 lesson again).
- Manually-coded extractors per CDX path — fragile to schema drift.

## §8 — Test crate `tests/` integration vs `mikebom-cli/src/` unit

**Decision**: place the smoke test at `mikebom-cli/tests/scan_windows_smoke.rs` (integration test, separate binary), NOT inside any `src/` module's `#[cfg(test)] mod tests`.

**Rationale**: Integration tests get the `CARGO_BIN_EXE_mikebom` env var; in-source unit tests do NOT (cargo only sets that for the separate `tests/` integration-test binaries). FR-001 explicitly calls for invoking the locally-built binary via subprocess — that's the integration-test pattern.

**Alternatives considered**: see §1 — no good alternative for invoking the built binary from a unit test.

## §9 — Diff-scope discipline check

**Decision**: per FR-009 + FR-010, the PR diff scope MUST be:
- 1 NEW file: `mikebom-cli/tests/scan_windows_smoke.rs`
- 3 MODIFIED files: `README.md`, `docs/user-guide/installation.md`, `.github/workflows/ci.yml`
- 0 production-code (`mikebom-cli/src/`) changes
- 0 Cargo.toml / Cargo.lock changes

Verification step in tasks.md will run `git diff --name-only main` and assert this exact set.

**Rationale**: tight diff scope keeps the PR easy to review + matches the spec's SC-008.

---

## Summary — research is settled

No remaining NEEDS CLARIFICATION. All decisions are anchored to:
- The user's spec clarifications (3 questions answered).
- The constitution (no production-code risk, no eBPF surface, no new crates).
- Pre-existing patterns in the codebase (`scan_polyglot_monorepo.rs` for `CARGO_BIN_EXE_mikebom`, milestone-100 docs structure for the callout placement).

Ready for Phase 1.
