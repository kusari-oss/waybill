# Feature Specification: m223 Pants pex-lockfile reader follow-up — front-matter tolerance + `[python.resolves]` map

**Feature Branch**: `672-pants-reader-follow-up`
**Created**: 2026-09-01
**Status**: Draft
**Input**: User description: "m223-follow-up: pex-lockfile front-matter tolerance + [python.resolves] map support"

## Clarifications

### Session 2026-09-01

- Q: FR-013 vs Assumptions contradiction — annotation OR log-line-only? → A: Log-line only (Option B). Drop the document-scope annotation from v1 scope; annotation stays a v2 extension point if downstream consumers demand a machine-actionable signal.
- Q: `[python.resolves]` value shape — bare string only, or also table? → A: Bare-string only for v1 (Option A). Table-shape entries WARN-and-skip naming the resolve name; table-shape parsing stays a v2 extension point.
- Q: Prefix-strip strategy — always strip, or retry-on-failure? → A: Always-strip-first (Option A). Uniform code path — every lockfile runs through the (typically no-op) prefix scanner before the JSON parser sees the bytes.

Milestone-671 shipped `--file-inventory=source-tree`. Milestone-223 (2026-07) shipped the initial Pants pex-lockfile reader. During a real-repo sanity check by an early adopter (an early adopter, 24-resolve Pants 2.33 monorepo) two m223 gaps surfaced that block extraction on non-default layouts:

1. **Pants ≤ 2.29 legacy lockfile shape**: pre-2.30 Pants prepended a `//`-comment metadata block to the lockfile bytes, which makes the file invalid JSON. The 2.30 release moved that metadata to a `.lock.metadata` sidecar (default from 2.31). Repos that have carried lockfiles across the 2.29 → 2.31 upgrade may still have stale files with `//` front matter — m223's `serde_json::from_slice` rejects those with a WARN and skips them. Reference: <https://www.pantsbuild.org/blog/2025/12/11/lockfile-metadata-files>.

2. **`[python.resolves]` map override**: m223's `pants.toml` reader understands only the singular `[python].lockfile = "..."` key. The modern (Pants 2.x) idiom is a `[python.resolves]` map of `<name> → <path>`. When any declared resolve path falls outside the default `3rdparty/python/*.lock` glob, m223 misses it entirely.

This follow-up closes both gaps additively — no default-mode churn for any repo already resolving via the m223 default glob.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Legacy `//`-comment lockfiles round-trip through the reader (Priority: P1)

An SRE runs `waybill sbom scan` on a Pants monorepo that upgraded from Pants 2.29 to 2.31+ at some point. The repo has one or more lockfiles that still carry the pre-2.30 `//`-comment front matter (either because the resolve was never re-generated after the upgrade, or because a lockfile in an archived subproject was preserved verbatim). The SRE expects the SBOM to include every locked distribution from every valid resolve, regardless of front-matter shape.

**Why this priority**: This is the failure mode the early adopter hit — one stale `python-default.pants.lock` at repo root drops silently. In a real audit, "silently dropped lockfile" is worse than "warning-only recoverable parse" because Kusari Inspector cannot flag what was never emitted. Also generic: any Pants user who upgraded through 2.30 has this shape lurking somewhere in their history.

**Independent Test**: Craft a fixture directory containing (a) one `//`-comment-prefixed lockfile carrying real `locked_resolves` and (b) one clean 2.31+ lockfile. Run waybill against the fixture; assert both files emit components with correctly-tagged resolve names. Assert that a lockfile carrying INVALID JSON *after* the leading `//` block still fails cleanly (warn + skip, does not abort the scan).

**Acceptance Scenarios**:

1. **Given** a Pants repo with `3rdparty/python/legacy.lock` starting with `// --- BEGIN PANTS LOCKFILE METADATA ---\n// {json body} \n// --- END PANTS LOCKFILE METADATA ---` followed by a valid JSON body, **When** the operator runs `waybill sbom scan`, **Then** the reader parses the body successfully AND emits components tagged with `resolve_name=legacy` AND records no WARN on the parse.
2. **Given** the same repo but the `//` block is followed by malformed JSON, **When** the scan runs, **Then** the reader logs a WARN citing the parse failure AND skips the file AND continues to parse every other lockfile in the repo AND the scan exits 0.
3. **Given** the same repo but with `python-default.pants.lock` at the repo root (outside `3rdparty/python/`) carrying a `//` block + an empty `locked_resolves: []`, **When** the scan runs, **Then** the file is either ignored (out of glob scope) OR — if reachable via `[python.resolves]` — parsed cleanly and emits zero components (empty resolve is not a warn condition).

---

### User Story 2 - `[python.resolves]` map override extends the discovery set (Priority: P1)

An operator runs waybill on a Pants monorepo where `pants.toml` declares a `[python.resolves]` map naming resolves that live under paths OTHER than `3rdparty/python/` (e.g. `build-support/py/mypy.lock`, `services/api/requirements.lock`, or a per-subproject `packages/foo/lockfile.json`). The operator expects every declared resolve to appear in the SBOM.

**Why this priority**: A resolve path outside the default glob is completely invisible today — no WARN, no counter, no way for the operator to know without reading the reader source. On multi-resolve setups this is the majority failure mode.

**Independent Test**: Craft a fixture repo with (a) `pants.toml` containing `[python.resolves]` mapping to a lockfile in a non-default directory (e.g. `build-support/py/foo.lock`) and (b) a duplicate resolve at the default `3rdparty/python/foo.lock` path. Run waybill; assert both paths get discovered (or, if dedup logic prefers the pants.toml-declared path over the glob-picked path, that dedup happens BY resolved absolute path so the same lockfile is never parsed twice). Assert emitted components carry the resolve-name key from the pants.toml map, NOT the file-stem derivation.

**Acceptance Scenarios**:

1. **Given** a repo with `pants.toml` declaring `[python.resolves]` = `{mypy = "build-support/py/mypy.lock", user_reqs = "3rdparty/python/user_reqs.lock"}` AND the two files exist, **When** the scan runs, **Then** both lockfiles are parsed AND emitted components tagged with `resolve_name=mypy` (for the first) and `resolve_name=user_reqs` (for the second).
2. **Given** the same repo but `[python.resolves]` declares a path that does NOT exist on disk, **When** the scan runs, **Then** the reader logs a WARN naming the missing path AND continues to parse every other reachable lockfile AND the scan exits 0.
3. **Given** a repo with BOTH an `[python.resolves]` entry AND a same-path entry picked up by the default glob, **When** the scan runs, **Then** the file is parsed exactly once AND the emitted `resolve_name` is the pants.toml-declared name (not the file-stem-derived name — the map is authoritative).
4. **Given** a repo where `pants.toml` declares the pre-2.x legacy `[python].lockfile = "..."` singular key AND the new `[python.resolves]` map, **When** the scan runs, **Then** both are honored (superset union), matching Pants's own precedence rules.

---

### User Story 3 - Operator-visible summary counts diagnose "no lockfiles picked up" reports (Priority: P2)

An operator runs waybill on a Pants repo and the SBOM contains zero Pants-derived components. They need to know WHY without reading the reader source. Today the `pants-pex reader complete` INFO log names discovered/parsed_ok/skipped_corrupt/components_emitted counts. This story extends that summary so the operator can distinguish "reader looked but found nothing" from "reader never ran because there was no `3rdparty/python/` directory AND no `[python.resolves]` map."

**Why this priority**: The an early adopter said "we ran waybill and it looks like no default.lock files might have gotten loaded" — the current INFO log path is silent when nothing gets discovered (line 111 short-circuits before the summary). Fixing the silent path materially reduces the "why did nothing get emitted" support cost.

**Independent Test**: Run waybill against a directory that has NO `3rdparty/python/` AND NO `pants.toml`. Assert that when the operator invokes with `RUST_LOG=info`, the log includes an explicit "no pants layout detected" summary line (or the existing `pants-pex reader complete` line with all-zero counts) rather than emitting no log at all.

**Acceptance Scenarios**:

1. **Given** a scan-root that has no `3rdparty/python/` directory AND no `pants.toml`, **When** the scan runs with `RUST_LOG=info`, **Then** the reader emits a single-line diagnostic naming zero-discovered outcome (byte-count-negligible cost — one log line, no counter allocations).
2. **Given** a scan-root that has `pants.toml` but no `[python.resolves]` and no `[python].lockfile` and no `3rdparty/python/*.lock` files, **When** the scan runs, **Then** the diagnostic states discovered=0 with a hint pointing at the two supported override keys.
3. **Given** a scan-root that has a valid Pants layout, **When** the scan runs, **Then** the pre-m672 log content is unchanged (SC-005 byte-identity for the happy path).

---

### Edge Cases

- A `//`-comment block that is malformed (e.g. missing the `--- END` marker, or the JSON inside the block is invalid): the outer JSON parse must still be attempted on whatever survives after the last `//` line. The reader MUST NOT try to interpret the metadata itself — it only skips the block. If the post-block bytes are still invalid JSON, standard WARN-and-skip applies.
- A lockfile that contains a `//` line as legitimate JSON content (e.g. inside a string value). Only leading `//` lines are stripped; any `//` character that appears after the first non-`//`, non-whitespace character is preserved.
- A `pants.toml` declaring `[python.resolves]` with duplicate values pointing at the same file under two different resolve names. The reader MUST parse the file once and emit components tagged with the LEXICALLY FIRST resolve name (deterministic).
- A `pants.toml` where `[python.resolves]` values are not bare strings (e.g. inline arrays, inline tables, or nested `[python.resolves.<name>]` sections). The reader MUST WARN once per entry naming the key + the observed TOML type + a "migrate to bare-string OR file a v2 follow-up issue" hint, and MUST skip only that entry — other bare-string entries in the same map remain honored. Table-shape parsing is explicitly out of scope for v1 per the 2026-09-01 clarification.
- A `pants.toml` where `[python.resolves]` and the legacy `[python].lockfile` disagree (same resolve name, different paths). Both are honored (superset union) matching Pants's own precedence rules; if the union collapses to the same absolute path via canonicalization, dedup applies per Acceptance Scenario 3.
- A `//`-comment block prepended to bytes that are already valid JSON (i.e. the JSON body starts with `{`). Strip only lines whose FIRST non-whitespace character is `//`; any leading whitespace lines are preserved so operators can see byte-identity for files that don't need stripping.
- A lockfile larger than a reasonable read-size cap (say, > 100 MB) — the stripper MUST NOT scan the entire file byte-by-byte before feeding to the JSON parser; the strip pass MUST be O(prefix-length) not O(file-length).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The pants-pex reader MUST tolerate a leading `//`-comment metadata block before the JSON body. The prefix stripper MUST run UNIFORMLY on every lockfile — no first-pass-parse-then-retry-on-failure branching (per the 2026-09-01 clarification). Every consecutive line whose first non-whitespace character is `//` MUST be stripped before the JSON parser sees the input. Stripping MUST stop at the first line whose first non-whitespace character is NOT `//` (e.g. `{` or `[`). On a clean-JSON file (first non-whitespace byte is `{`), the stripper is a no-op that consumes only the first line's worth of bytes.
- **FR-002**: The stripper MUST NOT interpret the metadata block's contents. Whatever the `//` lines say is discarded verbatim — waybill only cares about the JSON body.
- **FR-003**: The stripper MUST be O(prefix-length), not O(file-length). It reads the file bytes from the start until the first non-`//` line, then hands the remaining slice to the JSON parser.
- **FR-004**: When the JSON body after stripping is still invalid, the reader MUST emit a WARN naming the file path and the parse error, MUST skip the file, and MUST NOT abort the scan (matches m223 FR-007 fail-open contract).
- **FR-005**: The pants-config parser MUST recognize the `[python.resolves]` TOML table (a map of `<resolve-name> → <path-string>`) in addition to the pre-existing `[python].lockfile` singular string.
- **FR-006**: When both `[python.resolves]` and `[python].lockfile` are declared in the same `pants.toml`, both MUST be honored (superset union). Resolves declared in `[python.resolves]` MUST use their map key as `resolve_name`; the legacy `[python].lockfile` MUST use the file-stem-derived name (backward-compatible with m223).
- **FR-007**: When a `[python.resolves]` value is not a bare string (inline array, inline/nested table, integer, boolean, etc.), the reader MUST WARN once naming the offending resolve name AND the value's TOML type AND advising the operator to migrate to bare-string form or file a follow-up issue for table-shape support. Other entries in the same map MUST still be honored. Table-shape parsing (e.g. `[python.resolves.<name>] path = "..."`) is out of scope for v1 — deferred to a v2 milestone.
- **FR-008**: When a `[python.resolves]` value names a path that does not exist on disk, the reader MUST WARN once naming both the resolve name and the missing path, MUST NOT count the missing path in `lockfiles_discovered`, and MUST continue processing other resolves.
- **FR-009**: The reader MUST canonicalize every candidate lockfile path (via absolute-path resolution) before deduplication, so a resolve declared via `[python.resolves]` and the same file matched via the default `3rdparty/python/*.lock` glob are counted once. When dedup fires, the pants.toml-declared `resolve_name` wins (map is authoritative over file-stem derivation).
- **FR-010**: The `pants-pex reader complete` INFO log line MUST fire on every scan where the reader was invoked, INCLUDING the zero-discovered case. When zero lockfiles are found AND `pants.toml` has no override, the log MUST include a hint naming the two supported keys (`[python].lockfile`, `[python.resolves]`) so operators can self-diagnose.
- **FR-011**: The zero-discovered log line MUST be a single-line INFO diagnostic with allocation cost bounded to one `String` formatter invocation. No new file I/O in the negative path.
- **FR-012**: When neither `3rdparty/python/` nor `pants.toml` exists at the scan root, the reader MUST NOT log anything (matches m223 SC-003 byte-identity for non-Pants repos). The FR-010 diagnostic fires only when at least one Pants signal is present.
- **FR-013**: When the reader successfully strips-and-parses one or more legacy `//`-shape lockfiles, the `pants-pex reader complete` INFO log line MUST include a `legacy_shape_lockfiles=<N>` field naming the count so operators can nudge repo maintainers to regenerate. The count is log-only in v1 — no document-scope annotation is emitted (preserves post-m671 SBOM byte-identity for fresh Pants 2.30+ layouts; a machine-actionable annotation stays a v2 extension point if downstream consumers demand it). When zero legacy-shape files were seen, the field is absent (or emitted with value 0 — implementer's choice, both preserve SBOM byte-identity).

### Key Entities *(include if feature involves data)*

- **Legacy front-matter block**: A leading sequence of lines in a Pex lockfile file, each starting (after optional whitespace) with `//`. Bounded by the first non-`//` non-whitespace character (typically `{`). Content is opaque to waybill — the block is discarded before JSON parsing.
- **`[python.resolves]` entry**: A key-value pair inside the `[python.resolves]` TOML table where the KEY is the resolve name (operator-supplied identifier, e.g. `mypy`, `internal-libs`) and the VALUE is a **bare TOML string** naming a filesystem path (relative to the scan root, matches Pants's own resolution rules). Non-bare-string values (arrays, inline tables, nested `[python.resolves.<name>]` sections with `path = "..."`) are out of scope for v1 per the 2026-09-01 clarification — the reader WARNs and skips those entries.
- **Legacy-lockfile counter**: A per-scan integer counter of how many lockfiles the reader successfully parsed after stripping `//` front matter. Surfaced in the `pants-pex reader complete` INFO log line only (v1 scope). A future v2 milestone may promote this counter to a document-scope annotation if downstream consumers demand a machine-actionable signal.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On a Pants monorepo with N `//`-shape legacy lockfiles under the default glob directory, waybill emits components from all N files (previously 0 emitted from those files).
- **SC-002**: On a Pants monorepo where `[python.resolves]` declares K resolves at non-default paths, waybill emits components from all K resolves.
- **SC-003**: On a Pants monorepo that has ONLY default-shape lockfiles under the default glob (no legacy `//` shape, no `[python.resolves]` override), the emitted SBOM is byte-identical to pre-m672 output. Locked by a golden-fixture test.
- **SC-004**: On an early-adopter-shape repo (24 lockfiles, all under `3rdparty/python/*.lock`, all 2.33 clean JSON, one legacy `python-default.pants.lock` at repo root with empty `locked_resolves`), waybill emits ≥ 9,800 pypi components across all 24 resolves (≥ 99% of the ~9,838 total).
- **SC-005**: On a scan-root with no Pants signals (no `3rdparty/python/`, no `pants.toml`), zero log lines are emitted from the pants reader — matches m223 SC-003 for non-Pants repos.
- **SC-006**: On a scan-root with at least one Pants signal but zero lockfiles discovered (e.g. empty `3rdparty/python/`, or a `pants.toml` with an unresolvable `[python.resolves]` entry), the operator sees a single-line INFO diagnostic naming the outcome + the two supported override keys.
- **SC-007**: The reader parses a `//`-front-matter lockfile in under 5 ms of overhead vs. a clean-JSON lockfile of the same size (i.e. the prefix stripper adds no meaningful latency).

## Assumptions

- Pants ≤ 2.29 (December 2024 release) is the last version that emitted the `//` inline metadata block. 2.30 (January 2025) moved the block to a `.lock.metadata` sidecar; 2.31 (February 2025) made the sidecar the default. Ref: <https://www.pantsbuild.org/blog/2025/12/11/lockfile-metadata-files>.
- Every real-world Pants 2.x setup uses one of: (a) default glob-only, (b) `[python.resolves]` map, or (c) legacy `[python].lockfile` singular. No exotic third override is known to exist.
- Resolve names in `[python.resolves]` are safe strings — TOML key names permitted by the Pants spec (alphanumeric + `-` + `_` per Pants's own tests) match waybill's annotation-value hygiene.
- Deduplication of lockfile paths uses `std::fs::canonicalize` when both paths resolve — the pre-m223 code already does this indirectly via `if !out.iter().any(|d| d.path == resolved)` at `mod.rs:71`. This milestone extends dedup to canonical form.
- The `.lock.metadata` SIDECAR shape (2.30+) is out of scope. That file is a separate on-disk artifact that Pants writes for its own tooling; waybill does NOT need to read it. This milestone concerns only the INLINE `//`-comment shape.
- Filenames that end in `.lock` remain the discovery signal for the default glob. Nothing in this milestone changes the glob shape or the `.lock.metadata` sibling-exclusion rule (which happens naturally because `.metadata` != `.lock` as the last extension).
- The FR-013 legacy-lockfile counter is log-line only in v1 (see 2026-09-01 clarification). No parity catalog work, no new C-row, no golden churn. A v2 follow-up can promote the counter to a document-scope annotation if downstream consumers need a machine-actionable signal.
- The Inspector 2,000-dependency-change ceiling flagged by the early adopter is out of scope for this milestone — that's an Inspector-side policy, not a waybill emission constraint. Waybill continues to emit all components regardless of downstream diff-size ceilings.
