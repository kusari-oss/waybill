# Phase 0 Research — User-Supplied Directory Exclusion

**Feature**: 113-exclude-path-flag
**Date**: 2026-06-12

Resolves the open implementation choices left by the spec + clarifications. Each decision is presented with rationale and the alternatives that were considered and rejected.

---

## R1. Pattern dialect

**Decision**: `globset = "0.4"`. Pattern entries are compiled into `globset::Glob`s, combined into a single `globset::GlobSet`, and matched against each candidate directory's path relative to the scan root.

**Rationale**:
- Spec FR-006 requires patterns to "match directory names at arbitrary depth" — i.e. `**` semantics. POSIX `glob` (already transitive via the `glob` 0.3 crate) does NOT support `**`; it would fail to satisfy US2's "any depth" acceptance scenario.
- `globset` is pure Rust (Constitution Principle I + Strict Boundary 3) and pulls only `regex`, `regex-syntax`, `regex-automata`, `aho-corasick` — all of which are already in the workspace dependency closure for unrelated reasons. The marginal lockfile growth is zero new top-level transitives.
- Negation patterns (`!something`) are out-of-scope per the spec Assumptions. `globset`'s no-negation surface matches that scope exactly; gitignore-style (`ignore` crate) would carry negation machinery we don't want to expose yet.
- `GlobSet::is_match` is a single Aho-Corasick scan over all compiled patterns, amortized O(1) per directory — preserves SC-003's ≤10% performance budget.

**Alternatives considered and rejected**:
- `ignore = "0.4"` (gitignore-compatible): pulls `globset` + adds `crossbeam-channel`, `same-file`, `walkdir`; we already manage descent ourselves, so the walker side is unused weight. Negation machinery would tempt scope creep.
- `wax = "0.6"`: cleaner API but smaller user base, less-tested across the workspace; no compelling advantage over globset for our case.
- Hand-rolled `**` matcher: rejected for cost vs `globset`'s ~10 lines of integration code.

---

## R2. Literal-vs-pattern classification rule

**Decision**: An entry is a pattern iff its text contains `*`, `?`, or `[`. Otherwise it is a literal path interpreted relative to the scan root. The CLI flag accepts both; the scanner inspects each value at parse time. (This is the Q1 clarification answer made concrete.)

**Rationale**:
- Single user-facing flag (`--exclude-path`) is simpler than two parallel flags.
- Matches every dev-tool convention an operator is likely to know — gitignore, rsync `--exclude`, `find -path`, shell glob, `.dockerignore` all use the same rule.
- Literal-anchored mode covers the highest-traffic case (a specific `tests/fixtures` directory) with zero glob ceremony; operators who never write `**` never hit the pattern path.
- Implementation: a 3-character scan of the entry string at parse time.

**Alternatives considered and rejected**:
- Two flags (`--exclude-path` literal, `--exclude-glob` pattern): doubles the CLI surface for one bit of disambiguation.
- Always-pattern: forces operators who want literal `tests/fixtures` to escape characters they don't have. The "no metacharacters → literal" rule is the obvious default for users who haven't read the spec.

---

## R3. CLI flag surface

**Decision**: `--exclude-path <PATH_OR_PATTERN>` on `mikebom scan`. Repeatable via `clap`'s `ArgAction::Append`. Env-var fallback: `MIKEBOM_EXCLUDE_PATH` accepting a list of entries separated by the platform's path-list separator (`:` on Unix, `;` on Windows — same convention as `$PATH`). CLI-supplied entries and env-var entries combine by union (mirrors the milestone-111 `MIKEBOM_PKG_ALIAS` precedent at `cli/scan_cmd.rs:2262`).

**Rationale**:
- Flag name matches the issue body verbatim, minimizing operator surprise.
- Path-list separator follows operating-system convention for cross-platform tools; an operator who already knows how their shell joins paths into `$PATH` knows how to join `MIKEBOM_EXCLUDE_PATH` entries.
- Union semantics (rather than env-var overrides CLI) match the principle of least surprise — operators almost always want both layers active.
- `ArgAction::Append` is the existing mikebom precedent (used by `--exclude-scope`, `--pkg-alias`); no new parsing pattern.

**Alternatives considered and rejected**:
- Fixed separator (e.g. `;` on all platforms): trips Unix operators who have `;` in directory names (legal on Linux/macOS, rare but legal).
- Repeated env vars (`MIKEBOM_EXCLUDE_PATH_0`, `..._1`, ...): ugly and unbounded.
- Env-var-overrides-CLI: makes shell aliases harder to reason about.

---

## R4. Per-walker integration strategy

**Decision**: Thread `&ExclusionSet` through the existing reader chain (`scan_path` → `read_all` → each per-ecosystem `read`). At every walker's existing descent decision, after the existing built-in skip check, also consult the exclusion set. The shared `should_skip_default_descent` in `project_roots.rs` gains an `&ExclusionSet` parameter; each per-walker `should_skip_descent` gains the same parameter. All walkers receive the path of the candidate child directory (not just the name) so pattern-matching can match against the path-relative-to-scan-root.

**Rationale**:
- Minimal surface change — every existing skip helper is already at the right call site; we add one parameter, not a new layer of indirection.
- Keeps the exclusion logic colocated with the skip logic so future built-in additions and user-supplied additions share the same path.
- Threading `&ExclusionSet` (read-only) maintains the existing single-pass walker semantics; no global state, no thread-locals.
- Pattern-matching against the directory's path-relative-to-scan-root (rather than just its name) is required for FR-006's "literal directory paths interpreted relative to the scan root" — literal `tests/fixtures` matches `<root>/tests/fixtures`, not `<root>/services/a/tests/fixtures`.

**Alternatives considered and rejected**:
- Decorator pattern around the walker: introduces a trait abstraction we don't need elsewhere.
- Global state via `tokio::task_local!`: cross-runtime hazard; mikebom is partially sync.
- Pre-walk pass that builds an excluded-paths set: doubles the descent cost for zero benefit.

---

## R5. Transparency annotation (Principle X compliance)

**Decision**: When `ExclusionSet` is non-empty, the emitted SBOM carries an envelope-level `mikebom:exclude-path` annotation in all three formats:

- **CDX 1.6**: `metadata.properties[]` entry with `name = "mikebom:exclude-path"` and `value` as a comma-separated list of entries (entries are guaranteed to not contain commas by FR-007 — patterns with `,` in glob brackets get expanded out).
- **SPDX 2.3**: `creationInfo.annotations[]` (annotationType=OTHER, annotator=Tool: mikebom) with the same comma-separated list as comment.
- **SPDX 3**: an `Annotation` element on the `SpdxDocument`, statement = same list.

Pattern entries are emitted verbatim; literal entries are normalized to forward-slash form so the annotation is identical across platforms.

**Rationale**:
- Constitution Principle X mandates structured transparency metadata whenever completeness is intentionally narrowed. User-supplied exclusion narrows completeness; consumers cannot otherwise tell from the SBOM that this list is non-exhaustive.
- Constitution Principle V bullet 5 requires a native-construct audit before introducing a `mikebom:*` annotation. **Audit result**: none of CDX 1.6, SPDX 2.3, or SPDX 3 has a native field expressing "the operator excluded these paths during scan." CDX has `metadata.lifecycles` but it expresses build phase, not path filters. SPDX has document-level annotations but no path-filter primitive. The mikebom-specific annotation qualifies under the bullet 5 carve-out for "finer-grained information the standard does not express."
- Documented in `docs/reference/sbom-format-mapping.md` with the justification clause per Principle V.

**Alternatives considered and rejected**:
- Per-component annotation on each suppressed neighbor: noisy, and we have no neighbor to annotate when we never emitted the component in the first place.
- Silent suppression: violates Principle X.
- File-level annotation on the manifest that was skipped: there's no consumed-then-rejected manifest in the model — descent simply doesn't enter the subtree.

---

## R6. Logging strategy

**Decision**: `tracing::debug!` per matched directory at descent time ("excluded directory matched entry X"). One `tracing::info!` summary at end of `scan_path` when the set was non-empty, in the form: `exclude-path: applied N entries, suppressed M directories`. Same stderr destination as milestone-112's FR-013 summary; default env-filter shows it.

**Rationale**:
- Debug-per-match gives operators a way to verify their patterns matched what they expected (a common gotcha with `**`).
- Info-summary gives a single confirmation line that doesn't bury the rest of the scan output.
- Matches the milestone-112 precedent for scan-level summaries.

**Alternatives considered and rejected**:
- Always print at info level: noisy on large monorepos.
- Don't log at all: makes "why didn't my pattern work" debugging impossible.

---

## R7. Cross-platform path semantics (FR-009)

**Decision**: At parse time, normalize literal entries by replacing platform-specific separators with forward slashes (`/`). At match time, normalize the candidate directory's path-relative-to-scan-root the same way. Pattern entries are passed verbatim to `globset` (which is already platform-agnostic on separator handling). The result: `--exclude-path tests/fixtures` matches the same directories on Linux, macOS, and Windows regardless of which separator the operator typed.

**Rationale**:
- `globset` already handles both `/` and `\` in patterns transparently on Windows. Normalizing the candidate side too eliminates the literal-path case where the operator types `tests\fixtures` on Windows expecting it to match.
- Normalization is a single `path.to_string_lossy().replace('\\', "/")` call; cheap.

**Alternatives considered and rejected**:
- Use native `Path::components` on both sides for structural comparison: more correct in theory, but `globset` works on strings, so we'd need both representations. Strings are simpler.

---

## R8. Symlink handling (Edge Case 7)

**Decision**: A symlink that points into an excluded subtree resolves to the excluded subtree at match time. We canonicalize both the candidate directory (existing behavior at `golang/legacy.rs:1999` and `project_roots.rs:67`) AND the literal-entry path before comparison. Pattern entries match against the candidate's logical path (not the canonical resolution), so an operator who writes `**/testdata` matches every directory NAMED testdata regardless of where any symlinks point.

**Rationale**:
- The existing canonicalize-keyed visited-set already prevents symlink loops. Our addition piggybacks on it.
- For literal entries, canonicalization avoids the gotcha where two different relative path strings reach the same physical directory.
- For pattern entries, name-based matching is what operators expect; canonicalizing would surprise users whose pattern intentionally targets a symlink-name.

---

## R9. Malformed-pattern error class (FR-007 / SC-005)

**Decision**: A new `thiserror`-derived `ExcludePathError` enum in `exclude_path.rs`:

```text
ExcludePathError::MalformedPattern { entry: String, source: globset::Error }
ExcludePathError::EmptyEntry
```

The CLI parser collects all entries first, then attempts to classify+compile every one. If any entry fails, the parser returns the first error wrapped in `anyhow::Error` with context naming the entry verbatim. The scanner never begins walking. Exit code 2 (matching clap's "invalid argument" convention).

**Rationale**:
- SC-005 requires exit-before-walk with a single error line naming the bad entry verbatim. Failing at the parse boundary (before `scan_path` is called) satisfies this trivially.
- Distinguishing `EmptyEntry` (`--exclude-path ""`) from `MalformedPattern` (`--exclude-path '['` — unmatched bracket) gives operators actionable diagnostics.

---

## R10. Test fixture strategy

**Decision**: Vendor a small polyglot fixture under `mikebom-cli/tests/fixtures/exclude_path/` containing one real top-level project per ecosystem (cargo, maven, gem, pip, npm, gradle, nuget, yocto, go) AND a `tests/fixtures/<ecosystem>/` sibling for each that would otherwise emit a spurious main-module component. Each integration test exercises ONE ecosystem with `--exclude-path tests/fixtures` and asserts the real component appears, the fixture doesn't. One additional polyglot test scans the whole fixture root with the same exclusion and asserts no fixture components from any ecosystem leak. Byte-identity test uses no flag and compares against a committed golden.

**Rationale**:
- One test per ecosystem proves per-walker integration.
- One polyglot test proves the union semantics work across walkers (no walker holds state that would leak across ecosystems).
- Byte-identity protects FR-003 / SC-002 against future regression.
- Fixture lives in mikebom-cli/tests/fixtures (vendored, not in the milestone-090 split fixture repo) because it's small (~50 KB total) and tightly coupled to this milestone's acceptance tests.

**Alternatives considered and rejected**:
- Synthesize fixtures per-test via tempfile + write_*: works but the polyglot setup is verbose; vendoring is cleaner.
- Add to the milestone-090 split fixture repo: overkill for a feature that ships in one PR.
