# Research: milestone 169 — ipk/opkg archive-file reader

**Date**: 2026-07-06
**Feature**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md)

Phase 0 research. Resolves all Technical Context decisions. Where a decision was already made in a prior milestone and remains valid, the prior rationale is referenced by-link.

## R1 — Scope discovery: opkg installed-DB already exists (m107)

**Decision**: m169's PRIMARY delta is `ipk_file.rs` (archive-file reader — US1). The US2 installed-DB coverage the Q1 clarification (2026-07-06) added is largely a no-op — milestone 107 already landed `opkg::read()` at `mikebom-cli/src/scan_fs/package_db/opkg.rs:52`. Small hardening deltas needed:

- **Gap 1 (FR-014)**: m107's opkg.rs early-returns empty when `/var/lib/opkg/status` is absent (line 79). FR-014 requires fallback to enumerating `/var/lib/opkg/info/*.control` files. **Delta**: add fallback branch in `opkg::read()` returning parsed stanzas from `info/*.control`.
- **Gap 2 (FR-015)**: m107 emits `evidence_kind: None` at opkg.rs:203. FR-015 requires `"opkg-status-db"`. **Delta**: set the field.
- **Gap 3 (FR-016)**: no existing cross-source dedup logic. FR-016 requires installed-DB-wins-over-archive-file when both fire in the same scan. **Delta**: implement dedup in `read_all` at the `Vec<PackageDbEntry>` extension point.

**Rationale**: the Explore agent's package-DB survey confirms m107 handled the installed-DB code path in scope. The Q1 clarification was correct in scope but incorrect in size estimate — the "wider scope (~25-30 tasks)" the Q1 answer projected is actually smaller (~20-25 tasks) because most of the installed-DB work is already done.

**Alternatives considered**:
- **Ignore m107 opkg reader; write a fresh one for m169**: rejected — duplicates work + causes double-emission of the same PURLs. m107's implementation is correct.
- **Ship m169 with only archive-file coverage; defer installed-DB gaps to m170**: rejected — the three gaps (FR-014/FR-015/FR-016) are small (~20 lines total) and the Q1 clarification explicitly bundles them into m169.

## R2 — ipk outer envelope: gzipped tarball (empirically verified 2026-07-06)

**Decision (revised during Phase 1 T001 execution)**: parse `.ipk` as `gzip( tar { debian-binary, control.tar.gz, data.tar.gz } )` using the existing workspace `flate2` + `tar` deps. **NO ar-format parsing needed. NO new Cargo deps. NO hand-roll work.**

**Rationale (rewritten based on Phase 1 empirical evidence)**:
- The spec's initial draft (based on issue #500's "subset of Debian's `.deb` format" language) assumed `.ipk` = ar-envelope with `control.tar.gz` + `data.tar.gz` inside. That was CORRECT for pre-2015 `opkg-build` but is NOT what modern `opkg-build` produces.
- Phase 1 T001 downloaded 5 real-world `.ipk` files from OpenWrt 23.05.5 x86_64 base feed. All 5 files start with gzip magic (`0x1f 0x8b 0x08 0x00`), NOT ar magic (`!<arch>\n`). Running `tar tzf <file>.ipk` on each yields uniform structure: `./debian-binary`, `./control.tar.gz`, `./data.tar.gz` — the ar envelope is gone; the whole thing is a gzipped tarball. This matches modern `opkg-utils/opkg-build`'s output (see https://git.yoctoproject.org/opkg-utils/tree/opkg-build).
- Happy consequence: hand-rolled ar parsing is UNNECESSARY. The existing `flate2` (for gunzip) + `tar` (for tarball entry iteration) workspace deps cover the outer envelope. Same crates cover the inner `control.tar.gz` and `data.tar.gz` decompression.
- Zero new Cargo deps — Constitution Principle I posture preserved without hand-roll work.

**Implementation shape** (data-model.md E1 updated to match):
```rust
// Outer format: gzip → tar → find control.tar.gz + data.tar.gz entries
let outer = GzDecoder::new(File::open(path)?);
let mut ar = tar::Archive::new(outer);
for entry_res in ar.entries()? {
    let mut entry = entry_res?;
    match entry.path()?.file_name().and_then(|s| s.to_str()) {
        Some("control.tar.gz") => extract_control(&mut entry, config)?,
        Some("data.tar.gz")    => extract_data_file_list(&mut entry)?,
        Some("debian-binary")  => { /* skip; format-version marker */ }
        _ => { /* ignore extras */ }
    }
}
```

**Alternatives considered**:
- **Hand-roll ar parser (spec's original plan)**: rejected — the empirical evidence shows modern `.ipk` isn't ar. Adding ar parsing for a legacy format we haven't yet seen would be YAGNI.
- **Add `ar` crate as workspace dep**: rejected for the same reason.

## R2b — Legacy ar-format handling (deferred)

**Decision**: log a WARN when a `.ipk` file's first 8 bytes match `!<arch>\n` (ar magic); fall back to filename-only PURL construction per FR-006; do NOT parse the ar body inline.

**Rationale**: modern opkg-build produces the gzipped-tarball format (R2). Ar-format `.ipk` files are legacy artifacts from pre-2015 opkg-build. If a future testbed surfaces the pattern at scale, add ar parsing then (analogous to how m069 rpm handles both rpmdb-sqlite + rpmdb-bdb — two format dialects for the same ecosystem).

**Alternatives considered**:
- **Hand-roll ar parser now**: YAGNI — no empirical evidence any real target ships ar-format `.ipk` today.
- **Reject ar-format `.ipk` outright with an error**: rejected — the filename fallback still yields a usable PURL identity; users of legacy Yocto builds get partial coverage rather than zero.

## R3 — Shared control-file parser reuse (m107)

**Decision**: `ipk_file.rs` reuses `parse_stanzas` at `mikebom-cli/src/scan_fs/package_db/control_file.rs:108` verbatim. Change visibility from `pub(super)` to `pub(super)` (already correct — the function is `pub(super)`, callable from any `package_db/*` sibling).

**Rationale**:
- m107 established `parse_stanzas` as the shared RFC-822 parser between dpkg + opkg. Adding a third caller (ipk_file) is the intended extension pattern.
- opkg's control-file dialect is a strict subset of Debian's (per spec Background). Anything `parse_stanzas` handles for dpkg it also handles for opkg AND for ipk archive-file control files (same syntax, same parser).
- No changes needed to `control_file.rs` itself.

**Alternatives considered**:
- **Write ipk-specific parser**: rejected — pointless duplication. opkg is a strict subset of Debian's control-file syntax.

## R4 — File-tier walker allowlist entry (`.ipk`)

**Decision**: add `.ipk` to the recognized-artifact-suffix allowlist in `mikebom-cli/src/scan_fs/file_tier/content_shape.rs`.

**Rationale**:
- Issue #500's root cause is `shape_skipped=4584` — the file-tier walker's shape-check phase drops `.ipk` files because the suffix isn't recognized. Adding it to the allowlist is FR-001's literal requirement.
- Same one-line delta as prior per-format additions (m069 rpm, m138 composer, m139 cocoapods, etc.).

**Alternatives considered**:
- **Add via a separate configuration file**: rejected — the allowlist is a `const` in Rust code; that's the established pattern.

## R5 — Alternative-list dep-edge semantic (Q2 clarification)

**Decision**: parse `Depends: pkg-a | pkg-b` alternative-lists by splitting on `|` (with trim), take the FIRST alternative as the dep-edge target, and emit the remaining alternatives as a `mikebom:dep-alternative-alternates` per-source-component annotation with value = JSON-array of the fallback names.

**Rationale**:
- Q2 clarification chose Option C — first-alternative-only + annotation.
- Emitting edges to all alternatives would inflate BFS reachability (m158 invariant) and misrepresent opkg's runtime default (first wins if no other constraint).
- The annotation preserves consumer visibility of the alternates without polluting the dep-graph.

**Alternatives considered**:
- **Emit edges to all alternatives** (spec's pre-Q2 Edge Cases claim): rejected via Q2.
- **Emit ONLY the first, no annotation** (Q2 Option B): rejected — loses fallback visibility.
- **Emit a single edge to a synthetic `pkg:opkg/<a>|<b>` node**: rejected — pollutes PURL space with non-purl-spec identities.

## R6 — Fixture strategy (Q3 clarification)

**Decision**: three-tier fixture strategy per Q3 (2026-07-06):

- **CI-time small fixtures** (unit + integration tests): 3-5 vendored real-world `.ipk` files at `mikebom-cli/tests/fixtures/ipk-files/` (analogous to m069 `rpm-files/`). Plus a hand-crafted synthetic runtime-rootfs at `mikebom-cli/tests/fixtures/opkg-installed-db/` with `/var/lib/opkg/status` + 5-10 `info/*.control` + `.list` files.
- **PR-body attestation** (SC-001 + SC-005b + SC-011): the merging maintainer re-runs a Yocto scarthgap `core-image-minimal` build locally, scans, and attaches the reproduction to the PR body. Matches m165 audit-milestone pattern.
- **Out of scope**: scaled ~500 MB real-world fixture with 4587 ipks committed to any repo. Bloat outweighs signal.

**Rationale**:
- The Yocto build isn't reproducible in CI without adding a Yocto container to the CI matrix (kernel-of-clients) — massive cost for minimal signal.
- 3-5 vendored real ipks cover the code paths (well-formed, filename-only-fallback, control-file parsing, License routing).
- Attestation via PR body matches m165's "trust the maintainer's local scan" pattern.

**Alternatives considered**:
- **Q3 Option B — scaled synthetic fixture**: rejected — 100-file synthetic doesn't cover real-world edge cases the vendored ipks catch (multi-arch, non-canonical License strings, empty Description).
- **Q3 Option C — real 4587-file fixture in sibling repo**: rejected — repo bloat + fixture-brittleness across Yocto releases outweighs SC-001 direct-CI validation.

## R7 — Fixture provenance for vendored ipks

**Decision**: source 3-5 vendored `.ipk` files from OpenWrt's stable release feed (`https://downloads.openwrt.org/releases/23.05.5/packages/x86_64/base/`) — publicly-downloadable, MIT-license-compatible, well-known provenance.

**Rationale**:
- OpenWrt release feeds are the most reliable public source of production `.ipk` files.
- 23.05.5 is a stable LTS release — reproducible over time.
- OpenWrt itself is Apache-2.0 + GPL-2.0 (mixed) — vendored artifacts fall under package-author licenses which are documented per-ipk in their control files.
- Alternative Yocto-built ipks would work but are less publicly-fetchable at pinned versions.

**Alternatives considered**:
- **Yocto build outputs**: harder to pin across builds; no public URL for `.ipk`-only artifacts.
- **Custom-built ipks**: overkill; production `.ipk` files are the target and are freely available.

## R8 — m069 rpm-size cap alignment (FR-012)

**Decision**: apply a 16 MB uncompressed `control.tar.gz` size cap symmetric with m069 `RpmReaderConfig::max_control_size` at `mikebom-cli/src/scan_fs/package_db/rpm_file.rs:99-123`. When exceeded, emit filename-only components with `mikebom:archive-size-skipped` annotation.

**Rationale**:
- Real `.ipk` control.tar.gz files are typically <100 KB. A 16 MB cap catches malicious or misconstructed archives without impacting legitimate use.
- Symmetric behavior + magic-number choice with m069 avoids per-format policy drift.

**Alternatives considered**:
- **No cap**: rejected — DoS attack surface via crafted archives.
- **Different cap (e.g., 8 MB)**: rejected without evidence of a difference in ipk vs rpm typical sizes.

## R9 — Empirical validation via SC-011 PR body

**Decision**: SC-011 requires the merging maintainer to attach to the PR body: (a) walker log showing `shape_skipped=0` on the ipk-file portion (was `4584`); (b) component count ≥ 4580 for archive-file path; (c) synthetic-fixture-derived installed-DB validation showing ≥ 36 `opkg-status-db` emissions (matches issue #500's core-image-minimal's 36-installed count).

**Rationale**: Matches m165's Kubernetes+ArgoCD audit-milestone PR-body-attestation pattern where per-target scan output was recorded in the PR body. No new CI infrastructure needed.
