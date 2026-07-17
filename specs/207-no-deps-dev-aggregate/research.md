# Research: Fix `--no-deps-dev` Flag UX

**Date**: 2026-07-17
**Purpose**: Resolve 2 mechanical unknowns before task decomposition.

## R1 — Add a new `--no-deps-dev-license` flag rather than rely solely on `--enrich-sources`

**Investigation**: FR-003 leaves the fine-grained "license only" case open — either a new named flag OR the existing `--enrich-sources` allowlist mode. Trade-off:

- **Option A**: Add `--no-deps-dev-license` as a fine-grained flag mirroring the existing `--no-deps-dev-graph`. Operators who want to keep the pre-m207 behavior migrate by renaming the flag in their scripts.
- **Option B**: Rely solely on `--enrich-sources deps-dev-graph,clearly-defined` for the "license only" case. No new flag; operators use the existing allowlist mode.

**Decision**: Option A. Rationale:

- **Symmetry**: `--no-deps-dev-graph` already exists as the fine-grained "graph only" flag. Adding `--no-deps-dev-license` makes the fine-grained pair complete + memorable ("no-X-license" + "no-X-graph" mirror each other cleanly).
- **Migration path clarity**: Operators updating scripts from the old `--no-deps-dev` semantic can do a simple search-and-replace: `--no-deps-dev` → `--no-deps-dev-license`. Zero mental gymnastics; the change is well-signposted.
- **`--enrich-sources` is a heavier UX**: switching to allowlist mode means the operator now has to enumerate every source they DO want (clearly-defined, dep-graph, etc.), which is more brittle if new sources are added in future milestones.
- **Cost is trivial**: one new `#[arg(long)] pub no_deps_dev_license: bool` field; ~5 LOC.

**Alternatives considered + rejected**:
- Option B: rejected per the reasoning above — asymmetric with `--no-deps-dev-graph` + heavier UX for the common migration case.
- Deprecate `--no-deps-dev-graph` in favor of `--enrich-sources`: rejected — would break far more existing scripts than the current fix.

**References**:
- `mikebom-cli/src/cli/scan_cmd.rs:625-630` — existing `--no-deps-dev-graph` flag definition (semantic + doc-comment pattern to mirror).

## R2 — Migration WARN log fires when the aggregate flag is used alone

**Investigation**: FR-006 codifies a SHOULD-emit migration signal. Options:

- **Option A**: Log at INFO level, ALWAYS emit when `--no-deps-dev` is set. Simple; matches Constitution Principle X (transparency).
- **Option B**: Log at INFO level, emit ONLY when the operator is using the aggregate flag without the fine-grained escape hatches (i.e., `--no-deps-dev` is set AND neither `--no-deps-dev-license` nor `--no-deps-dev-graph` is also set). More targeted — signals only to operators likely affected by the semantic change.
- **Option C**: Don't log at all. Documentation-only migration via `--help` text updates.

**Decision**: Option B. Rationale:

- Operators using `--no-deps-dev` alongside `--no-deps-dev-license` or `--no-deps-dev-graph` are ALREADY thinking about fine-grained semantics — the WARN adds no signal for them and is noise.
- Operators using `--no-deps-dev` alone are the exact set whose behavior might change vs pre-m207: they get the one INFO log line explaining the semantic + linking to the fine-grained escape hatch.
- INFO level (not WARN) matches the informational nature: "here's a semantic to notice," not "you did something wrong." Aligns with the m204/m205 tracing-level convention (WARN = fallback fired; INFO = advisory).
- Non-noise: only fires once per scan (single-shot; not per-component).

**Log message text** (pinned for the integration test's stderr grep):

```
--no-deps-dev now disables ALL deps.dev enrichment paths (m207 aggregate semantic per #596). \
For the pre-m207 "license only" behavior, use --no-deps-dev-license instead.
```

**Alternatives considered + rejected**:
- Option A (always log): rejected — noisy for operators who are already fine-grained-aware.
- Option C (no log): rejected — silently changing behavior for operators upgrading through this milestone violates Principle X.
- WARN level: rejected — `--no-deps-dev` post-fix isn't a failure or unexpected condition; INFO matches the semantic. Reserving WARN for failure conditions preserves signal-to-noise for operators using `RUST_LOG=warn` filters.

**References**:
- Memory `feedback_native_fields_first` — Constitution Principle X + XI compliance patterns for m199-m204.
- m203's helm-render-fallback WARN convention — informational-emit-only-when-relevant precedent.

## Decision Summary

| Decision | Chosen | Alternative | Rationale |
|---|---|---|---|
| Fine-grained "license only" mechanism | New `--no-deps-dev-license` flag | Rely on `--enrich-sources` allowlist | Symmetric with existing `--no-deps-dev-graph`; simple search-and-replace migration path |
| Migration signal | INFO log, ONLY when `--no-deps-dev` used alone (no fine-grained escape hatches) | Always-emit / no-log | Targets the exact operator set affected by the semantic change; INFO level matches advisory nature |
| WARN log text | Fixed message pinned in research.md above | Free-form / auto-generated | Integration test greps stderr for this substring; determinism required |
| New Cargo deps | Zero | (n/a) | Nothing needed |
