# Research: spec 1660

## R1. The candidate list in #1727 was mostly wrong (finding)

**Method.** I scanned every `.rs` file under `waybill-cli`, production code included. For each temporary-file constructor, the scan took the statement from the constructor to the next `;` and flagged it if `.path()` appears in that statement. The constructors searched:
- `NamedTempFile::new(`
- `NamedTempFile::new_in(`
- `Builder::new()`
- `tempfile::tempdir(` and bare `tempdir()`
- `TempDir::new(` and `TempDir::new_in(`

**Result.** 24 statements, all in `waybill-cli/tests`, none in production code:
- **23 are the dropped shape** `NamedTempFile::new().expect|unwrap(…).path().to_path_buf()`, across 22 files. `scan_image.rs` has 2.
- **1 is a false hit:** `policy_layout.rs:255`, where `.path()` is the argument passed to `new_in(...)`. Its result is bound to `f` and held.

**#1727's list came from a different method:** a grep for `NamedTempFile::new()` with 3 lines of context, then a match on `.path()` and `to_path_buf`. That caught files that bind the guard and *then* take its path on the next line, which is the held shape. `warm_go_cache.rs:51-52`, for example, is `let tmp = …new()…; let out_path = tmp.path().to_path_buf();`.
- **Overlap:** only `scan_image.rs` is on both lists.
- **Missed:** #1727's list doesn't contain `scan_pip.rs`, the file whose test failed.
- **Action:** correct #1727 with the statement-level list, and use that list for this spec. The per-site review (FR-003) still checks each one by hand.

**Lesson recorded:** a line-context grep measures proximity, not ownership. The check (R3) works per statement for the same reason.

## R2. Stress harness (FR-001)

**Decision:** a standalone throwaway crate at `specs/1660-held-temp-paths/measurements/collide/`.
- **Dependencies:** `tempfile = "=3.27.0"`, the same version as the workspace lockfile, and an empty `[workspace]` table so it isn't part of the workspace. Nothing is added to the repository's Cargo dependencies.
- **What it does:** the binary creates a `NamedTempFile`, drops it, and prints the name, which is the tests' shape.
- **The driver** (`collide.sh`) starts N copies at once (N = 18, 36 and 72), repeats that R times, and counts duplicate names with `sort | uniq -d | wc -l`.
- **Control:** a second binary (`--fixed-seed`) builds the name from a `fastrand::Rng` with a constant seed, the way `tempfile` would if every process were seeded identically. It must report duplicates; if it doesn't, the counting is broken. `fastrand` is already a dependency of `tempfile`, so this adds nothing.
- **Report:** host, N, R, names drawn, duplicates, for every run, including 0.

**Why not inside the test suite:** this is a one-off measurement, not a regression test. The regression guard is R3.

## R3. Enforcement check (FR-006)

**Decision:** `scripts/check-held-temp-paths.py`, Python 3 standard library only, run as `python3 -I -B`, like `check-homoglyphs.py`.
- **What it scans:** every `*.rs` file in every workspace member (`waybill-cli/`, `waybill-common/`, `waybill-ebpf/`, `xtask/`), excluding `target/`.
- **What it flags:** a statement that calls a temporary-file or temporary-directory constructor (the R1 list), then a method chained onto that constructor's result, ending in `.path()` and optionally followed by `.to_path_buf()`, `.to_owned()` or `.join(…)`, without binding the guard. The check is per statement, from the constructor to the matching `;`, so `new_in(tmp.path())` is not flagged: there, `.path()` belongs to the argument, not to the chain.
- **What it allows:**
  - binding the guard: `let tmp = …;` then `tmp.path()`;
  - deliberate persistence (`.keep()`, `.into_temp_path().keep()`, `.persist(`);
  - a `// held-temp-path: <reason>` marker on the line before.
- **Output:** each violation as `file:line: statement`, and exit 1.
- **Controls:** `--self-test` runs the check on built-in snippets. The 23 old shapes, a `TempDir` variant and a `Builder` variant must be flagged; the held shapes, `new_in(tmp.path())`, `.keep()` and the marker must pass. A positive control on the real tree is recorded: run the check before the fix, and it must list all 23 sites.
- **Where it runs:** `scripts/pre-pr.sh`, next to `check-walker-audit.sh`, and CI's Linux lint job, as one more step beside the walker-audit step. One script, so the two can't drift (#1111).

**Alternatives considered:**
- **A lint test in the suite:** it would only run after compiling, and wouldn't fail fast.
- **Clippy `disallowed_methods`:** it can't express "dropped in the same statement".

## R4. Fix shape (FR-004)

**Decision:** in each of the 23 sites the helper does `let out = NamedTempFile::new()…path().to_path_buf();`, then runs the scan writing to `out`, then reads it. Replace the line with a held `TempDir` and a file inside it:

```rust
let out_dir = tempfile::tempdir().expect("tempdir");
let out = out_dir.path().join("out.json");
```

Here `out_dir` lives until the end of the helper, after the read.

**Rationale:**
- It's the spec's preferred shape.
- On Windows, a held `NamedTempFile` keeps an open handle that can block the scan's write (spec edge case).
- The output name is fixed, which changes nothing the tests assert. Several helpers derive the format from `--format`, not from the file name, and the review confirms that per site.

**Format-sensitive names:** where a helper's output name carries an extension the scan reads (for example `.cdx.json`), keep that extension in the joined name.

## R5. Verification (FR-007)

- **Locally:** `cargo nextest run --workspace --no-fail-fast --color never`, 3 times, after the fix.
- **On CI:** every lane once, enumerating every job.
- **Timing:** none. This spec changes correctness, not speed.
