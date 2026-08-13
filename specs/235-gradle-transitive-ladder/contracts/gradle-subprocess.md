# Contract: `subprocess.rs` (US1 subprocess resolver)

**File**: `waybill-cli/src/scan_fs/package_db/gradle/subprocess.rs`
**Consumers**: `ladder.rs`

---

## Entry point

```rust
pub fn resolve_via_subprocess(
    project_dir: &Path,
    flags: &GradleCliFlags,
) -> Result<GradleResolvedGraph, SubprocessOutcome>;
```

## Contract

1. **Discover wrapper**: Prefer `project_dir/gradlew` (POSIX) or
   `project_dir/gradlew.bat` (Windows). If absent, look for
   `gradle` on `$PATH`. If neither found, return
   `SubprocessOutcome::ToolMissing`.

2. **Enumerate subprojects**: First subprocess call is
   `./gradlew projects --no-daemon --quiet`. Parse the tree-format
   output to list `:sub1`, `:sub2`, etc. (empty list means
   single-project build; use the root project.)

3. **Per subproject × configuration**: For each subproject and each
   configuration in the effective set:
   - Effective set = `["runtimeClasspath", "testRuntimeClasspath"]`
     ∪ `flags.gradle_extra_configurations`.
   - If `flags.gradle_resolve_buildscript`, also spawn one call per
     subproject with `--configuration buildscript` scope.
   - Command shape:
     ```
     ./gradlew <path>:dependencies \
         --configuration <config> \
         [--no-daemon]           // unless flags.gradle_daemon
         --quiet
     ```
   - Timeout: `flags.gradle_timeout_secs` seconds. Kill the child
     process cleanly on timeout (SIGKILL after SIGTERM+2s grace).

4. **Parse output**: Use the ASCII-tree parser (research R3). Skip
   until finding `<configName> - <description>` header. Parse
   `<indent>+--- <coord>[ -> <resolved>][ (*)]` lines. Assemble
   `Vec<(coord, depth, parent_index)>` then convert to `edges: Vec<(Purl, Purl, EdgeScope)>`.

5. **On non-zero exit** (script bug, plugin missing, etc.): return
   `SubprocessOutcome::NonZeroExit { status, stderr_tail: last 40 lines }`.

6. **On parse failure**: return `SubprocessOutcome::ParseError { line, snippet }`.

7. **On success**: return `SubprocessOutcome::Success(graph)` where
   `graph.tier = Subprocess` and `graph.fallback_history = vec![]`.

## Failure modes

| Condition | Outcome |
|---|---|
| No `gradlew` and no `gradle` on PATH | `ToolMissing` |
| JDK not on PATH (Java error from wrapper) | `NonZeroExit` |
| Timeout elapsed | `Timeout` (subprocess killed) |
| Script runs but exits non-zero (missing plugin, syntax error) | `NonZeroExit` |
| Output has unexpected shape | `ParseError` |
| Success | `Success(graph)` |

## Subprocess safety

- All `Command::new` calls use `stdin: null` (no inherited stdin).
- All arguments are passed as separate `arg()` calls; no shell
  interpolation of user-provided values (`gradle_extra_configurations`
  is Vec<String> passed as `--configuration <name>` positional).
- `--gradle-extra-configurations` values are validated pre-invoke
  to reject shell-metacharacters (per R8).

## Test hooks

- `#[cfg(test)]` variant accepts an injected `mut env: Command`
  builder so tests can replace `./gradlew` with a fixture shell
  script that emits canned output.
- Integration tests gated behind `WAYBILL_TEST_REAL_GRADLE=1` env
  var run against a real Gradle project fixture.
