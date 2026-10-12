# Feature Specification: Close-time facts for compiler read and write sets

**Feature Branch**: `1144-compiler-set-close-time`
**Created**: 2026-10-10
**Status**: Draft
**Input**: Issue #1144 — "Compiler write/read sets: close-time hashing and
`survived_trace_window` were never implemented (every entry is hash 0,
survived=true)"

## Context

Milestone 1070 (#614) populated `compiler_pipeline.invocations[].read_set`
and `.write_set` for the first time. Both carry fields that have never held
a true value:

- every `write_set[].sha256` and `read_set[].sha256` is 64 zeros;
- every `write_set[].survived_trace_window` is `true`, including rustc
  temporaries (`deps/rmeta*/full.rmeta`, `deps/rustc*/lib.rmeta`) that are
  deleted before the build ends.

`trace/compiler_pipeline.rs:236` records a placeholder and defers the real
work to "a future task (T047 wiring around hasher.rs at close-time)". That
task never landed. The defect was invisible while both sets were empty.

The naive fix — compute the hashes at close-time and write them into the
existing fields — would trade one false statement for a subtler one. The
kernel never hashes (`waybill-ebpf/src/programs/file_ops.rs` sets
`content_hash = [0; 32]` at every site), so close-time is the only moment a
hash can be taken. For a **write** set that is nearly the right fact: the
product is on disk and its content at close is what the invocation
produced. For a **read** set it is a different fact entirely — the bytes the
compiler read are gone, and what remains is whatever the path holds minutes
later.

### The SBOMit pattern this follows

SBOMit separates content derived from in-toto metadata, which a verifier can
check, from its **mutation** layer, which exists to make the SBOM "more
complete than what can be strictly verified" and is explicitly not verified
— a tool warns when a mutation overrides in-toto-derived content, and
installation of such a document requires an explicit
`--insecure-allow-in-toto-override` flag.

This feature applies that separation inside the predicate. Each set entry
gains two structurally distinct tiers:

- an **observation tier** — what the kernel actually saw: the path, the
  read/write kind, the ordering, the rename chain. Verifiable against the
  raw `file_access` events in the same attestation.
- a **close-time tier** — separately named fields asserting the file's state
  when the trace ended. A verifier reading `sha256_at_close` cannot mistake
  it for the bytes that were read.

### Known asymmetry, deliberately not addressed

`waybill-cli/src/cli/scan.rs:608` already performs an unlabelled close-time
hash for `file_access`, with a comment giving this feature's exact
reasoning: the uprobes see ~512 B of each TLS record, so the observed hash
"would not match the bytes actually on disk", and the real files are
stream-hashed after the command finishes. That is the same defect class in a
different field. It is out of scope here and should get its own issue; this
spec does not change `file_access`.

### Maturity, and what happens if this is wrong

The tracing subsystem is experimental. `compiler_pipeline` populates only on
Linux behind `--features ebpf-tracing`, off by default everywhere, and the
witness attestor is explicitly pre-stable. This feature does not have to be
right the first time.

So the bar is **not** "never ship an approach we later remove" — it is
"never remove one without recording why it did not work". If the two-tier
shape, the cumulative-bytes budget or the once-per-path rule proves
ineffective in practice, the finding goes in `measurements/findings.md`
alongside the evidence that motivated it, and the spec records the reversal.
A design that was tried and abandoned with its reasoning intact is a
contribution; one that is quietly reverted costs the next person the same
investigation.

What this does *not* relax: a signed attestation must not contain a false
statement (Principle IX) for however long it ships. Being experimental is a
reason the shape may change, not a reason a field may lie in the meantime.

## Clarifications

### Session 2026-10-10

- Q: For read-set entries, what should the attestation say about content
  hashes? → A: Two tiers, SBOMit-style. The observation tier records only
  what the kernel saw and carries no hash; a separately named close-time
  field carries the hash with stated semantics. A verifier can tell the two
  apart.
- Q: When a write is renamed into place (rustc's usual pattern), what should
  the write set record? → A: One entry at the surviving name, carrying the
  intermediate names it passed through in `renamed_from`. One artifact, one
  entry, history intact for reconciliation against raw `file_access` events.
- Q: How should the version URIs handle this shape change? → A: In place
  under `build-trace/v1`, and bump the witness attestor
  `compiler-invocation/v0.1` → `v0.2`. `compiler_pipeline` is an optional
  sub-object behind an off-by-default feature, and the removed field only
  ever emitted 64 zeros, so no shipped consumer can depend on its value. The
  attestor is explicitly pre-stable, so the bump is free and signals the
  change.
- Q: What's the hashing policy for the close-time pass? → A: Budgeted, with
  the coverage and any truncation disclosed in `trace_integrity` per
  Constitution Principle X.
- Q: What unit should the budget cap, given attestations must stay
  byte-identical across re-runs of a reproducible build (feature 006 US2)?
  → A: Cumulative bytes hashed. A wall-clock budget was rejected: it
  truncates at a different path on every run and so destroys the byte
  identity that signing depends on. Bytes are also the honest proxy for
  cost, and `hasher.rs` already takes a per-file `max_bytes`.
- Q: When the budget truncates, what should the command do? → A: Succeed,
  set `truncated: true`, and emit a WARN. Principle X's remedy for "cannot
  guarantee" is structured disclosure; discarding a complete observation
  tier because an advisory tier was partial loses real data.

### Session 2026-10-12 (spec review)

- Q: Does the close-time pass count and charge per set entry or per distinct
  path, given a dependency's `.rmeta` is read by every invocation that
  depends on it? → A: Per distinct path. One filesystem check and one hash
  per path per trace, every entry naming it carrying that result, and the
  coverage counts over distinct paths. Charging per entry would bill the
  same bytes N times against the budget and would let two entries disagree
  about one path's close-time state.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — A deleted intermediate is reported as deleted (Priority: P1)

A build engineer reads a traced Rust build's attestation to see what each
`rustc` invocation produced. Files that rustc wrote and then deleted are
reported as absent at close; files that are still on disk are reported as
present, with their content hash.

**Why this priority**: This is #1144's stated defect. Today every write-set
entry claims `survived_trace_window: true`, which is a false statement in a
signed attestation and a direct Constitution Principle IX violation.

**Independent Test**: Trace `two_binaries_diverge` on a real kernel
(Colima aarch64, 6.8) and confirm that rustc's `full.rmeta` /
`lib.rmeta-link` temporaries report `existed_at_close: false` with no hash,
while the final `.rlib` / binary report `existed_at_close: true` with a hash
matching an independent `sha256sum` of the file on disk.

**Acceptance Scenarios**:

1. **Given** a traced build in which the compiler wrote a file and deleted
   it before the trace ended, **When** the attestation is written, **Then**
   that write-set entry has `existed_at_close: false` and no
   `sha256_at_close` field.
2. **Given** a traced build whose compiler products are still on disk at
   trace end, **When** the attestation is written, **Then** each
   corresponding write-set entry has `existed_at_close: true` and a
   `sha256_at_close` equal to the SHA-256 of that file computed
   independently of waybill.
3. **Given** any traced build, **When** the attestation is written, **Then**
   no write-set entry carries an all-zero content hash.

---

### User Story 2 — A read-set entry never claims a hash it does not have (Priority: P1)

A verifier consuming the build-trace predicate distinguishes what waybill
observed from what waybill computed afterwards. A read-set entry states the
path and how it was read; any hash it carries is explicitly a close-time
hash, not the content the compiler consumed.

**Why this priority**: `read_set[].sha256` is non-optional today and so
cannot express "unknown" — it emits 64 zeros instead. A consumer treating
that as a content hash would bind a component to a hash that identifies
nothing. This is the structural change the rest of the feature rests on.

**Independent Test**: Deserialize an attestation from a real kernel trace
and confirm no read-set entry carries a `sha256` field, that any
`sha256_at_close` present matches the file's current content, and that a
read path deleted during the build carries `existed_at_close: false` with no
hash.

**Acceptance Scenarios**:

1. **Given** a compiler invocation that read a source file, **When** the
   attestation is written, **Then** the read-set entry carries no `sha256`
   field, and its close-time state appears only in `existed_at_close` and
   (when hashed) `sha256_at_close`.
2. **Given** a compiler invocation that read from standard input, **When**
   the attestation is written, **Then** the `<stdin>` entry has
   `existed_at_close: false` and no `sha256_at_close`, and no zero-filled
   sentinel hash appears anywhere in the predicate.
3. **Given** a read path that the build deleted before the trace ended,
   **When** the attestation is written, **Then** the entry reports
   `existed_at_close: false` rather than omitting the read.

---

### User Story 3 — A renamed artifact is one entry with its history (Priority: P2)

A verifier reconciling the write set against the attestation's raw
`file_access` events can follow an artifact that rustc wrote under a
temporary name and renamed into place, without seeing it counted twice.

**Why this priority**: Rename-into-place is rustc's normal pattern, so
getting it wrong distorts most entries. It is P2 rather than P1 because
today's behaviour (the entry moves to the new path) is not a false
statement — just a lossy one.

**Independent Test**: Trace a build in which a compiler renames an output
into place, and confirm the write set contains exactly one entry for that
artifact, keyed by the final path, whose `renamed_from` lists the
intermediate path, and that the intermediate path also appears in the raw
`file_access` events.

**Acceptance Scenarios**:

1. **Given** a compiler that wrote `X.tmp` and renamed it to `X`, **When**
   the attestation is written, **Then** the write set contains one entry
   with `path: X` and `renamed_from: ["X.tmp"]`, and no separate entry for
   `X.tmp`.
2. **Given** a chain of two or more renames, **When** the attestation is
   written, **Then** `renamed_from` lists every intermediate name in the
   order observed.
3. **Given** a write with no rename, **When** the attestation is written,
   **Then** `renamed_from` is absent from the serialized entry.

---

### User Story 4 — An operator can see how much was hashed (Priority: P2)

An operator tracing a large build sees how many paths the close-time pass
hashed, how many it skipped, and whether a budget cut the pass short — so
partial coverage is never mistaken for a clean result.

**Why this priority**: Required for US1 and US2 to be safe on a large build,
and Principle X requires disclosure whenever accuracy cannot be guaranteed.
P2 because an unbudgeted pass is still correct, just unbounded.

**Independent Test**: Trace a build with the budget set deliberately below
the build's hashable bytes, and confirm the attestation reports
`truncated: true` with `paths_hashed < paths_total`, that a WARN names the
exhaustion, and that the command still exits zero with the attestation
written.

**Acceptance Scenarios**:

1. **Given** a trace in which every path was hashed within budget, **When**
   the attestation is written, **Then** `trace_integrity` reports
   `truncated: false` and the counts satisfy
   `paths_hashed + paths_absent + paths_skipped == paths_total`.
2. **Given** a trace whose hashable bytes exceed the budget, **When** the
   attestation is written, **Then** `truncated: true` is reported, a WARN
   names the exhaustion and the counts, and the command exits zero.
3. **Given** two runs of the same reproducible build with the same budget,
   **When** both attestations are written, **Then** the same set of paths
   was hashed in both and the pre-signature canonical payloads are
   byte-identical.
4. **Given** a trace that captured zero compiler invocations, **When** the
   attestation is written, **Then** the coverage field is absent and the
   serialized `trace_integrity` is byte-identical to a pre-m1072
   attestation.

---

### Edge Cases

- A write-set path that exists at close but cannot be read (permissions,
  unmounted): `existed_at_close: true`, no hash, counted in
  `paths_skipped`, and the failure does not abort the trace.
- A path replaced by a directory, symlink or device node between the write
  and close: hashed only if it is a regular file; otherwise
  `existed_at_close` reflects the stat and the path is counted in
  `paths_skipped`.
- A file larger than `hasher.rs`'s per-file `max_bytes`: **skipped**, with
  `existed_at_close: true`, no hash, and counted in `paths_skipped`.
  Corrected at plan time: `sha256_file_hex` *refuses* an oversized file
  (`Err(InvalidData)`) rather than hashing to the cap, and the existing
  `file_access` pass logs at debug and moves on
  (`waybill-cli/src/cli/scan.rs:871`, `hasher.rs:49-61`). The earlier
  wording asserted the opposite of both halves.
- A rename whose source was never observed as a write (the compiler renamed
  a file it did not create in this invocation): the destination gets an
  entry; `renamed_from` records the observed source.
- A rename target that collides with an existing write-set entry: the
  surviving entry keeps the final path, and `renamed_from` accumulates both
  histories in observed order.
- Two write-set paths sharing one inode (cargo hard-links `deps/<bin>-<hash>`
  to its user-facing name on Linux): these are two distinct paths, so
  FR-011's once-per-path rule does not merge them. Both are hashed
  independently and legitimately report the same `sha256_at_close`.
  Identical hashes across entries are not an error and MUST NOT be
  deduplicated.
- One path in several invocations' read sets (the normal case — every
  invocation that depends on a crate reads its `.rmeta`): hashed once, with
  every entry naming it carrying that one hash, and counted once in
  `paths_total`. This is what distinguishes a distinct-path count from an
  entry count, and it is why the budget is not charged per entry.
- One path in both a write set and a read set (one invocation's output read
  by another): one check, one hash, charged once, in the write-set
  position.
- One path in two invocations' write sets: not specified for, because it
  does not occur — see Assumptions and `measurements/findings.md` Q1. Were
  it to occur, FR-011 would give both entries the same hash rather than
  two conflicting ones.
- A path whose size exceeds the remaining budget: skipped, counted in
  `paths_skipped`, `truncated` set, and the pass continues to the next path
  — so one oversized artifact early in sort order does not blind the pass to
  everything after it.
- Zero compiler invocations: no pass runs, no coverage field, byte-identity
  with pre-m1072 preserved.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: `ReadSetEntry` MUST NOT carry a `sha256` field. Its
  observation tier is `path` and `kind`.
- **FR-002**: `ReadSetEntry` and `WriteSetEntry` MUST each carry
  `existed_at_close: bool` and an optional `sha256_at_close`, omitted from
  serialization when absent.
- **FR-003**: `existed_at_close` MUST be derived from a filesystem check
  performed after the traced command exits, never defaulted to a constant.
- **FR-004**: `sha256_at_close` MUST be present only when the path was
  hashed, and MUST be the SHA-256 of the file's content at close time.
- **FR-005**: The field `survived_trace_window` MUST be replaced by
  `existed_at_close` on `WriteSetEntry`, so both sets state the same fact
  under the same name.
- **FR-006**: No entry in either set may carry an all-zero content hash.
- **FR-007**: The `<stdin>` read-set entry MUST express "no file" through
  `existed_at_close: false` and an absent `sha256_at_close`. The 64-zero
  sentinel MUST be removed.
- **FR-008**: `WriteSetEntry` MUST carry `renamed_from: Vec<PathBuf>`,
  listing observed intermediate names in observed order, omitted from
  serialization when empty.
- **FR-009**: A rename observed within an invocation MUST yield exactly one
  write-set entry, keyed by the final observed path.
- **FR-009a**: `renamed_from` MUST record renames only. A hard link is not a
  rename — both names survive and share an inode — and MUST NOT populate
  `renamed_from`.
- **FR-010**: The close-time pass MUST run after the compiler aggregator has
  finalized and before the attestation is built, and MUST NOT mutate the
  observation tier.
- **FR-011**: The close-time pass MUST operate over the union of distinct
  paths across all invocations, performing at most one filesystem check and
  at most one hash per distinct path per trace. Every set entry naming that
  path MUST carry the same resulting `existed_at_close` and, when hashed,
  the same `sha256_at_close`; two entries naming one path can therefore
  never disagree about its close-time state.
- **FR-011a**: The pass MUST be bounded by a cumulative-bytes budget, with
  write-set paths processed before read-set paths and each set processed in
  lexicographic path order. A path named by more than one entry is charged
  to the budget once, at its first position in that order.
- **FR-012**: A path whose size exceeds the remaining budget MUST be skipped
  and the pass MUST continue to the next path, so two runs of the same
  reproducible build under the same budget hash the same set of paths.
- **FR-013**: `TraceIntegrity` MUST carry a coverage record stating
  `paths_total`, `paths_hashed`, `paths_absent`, `paths_skipped`,
  `truncated`, and whether the budget was the default or
  operator-supplied, such that
  `paths_hashed + paths_absent + paths_skipped == paths_total`. All four
  counts MUST be over distinct paths, not over set entries: a path read by
  N invocations counts once.
- **FR-014**: The coverage record MUST be additive and placed last in
  `TraceIntegrity`, omitted when no compiler invocations were captured, so
  pre-m1072 attestations round-trip and stay byte-identical.
- **FR-015**: Budget exhaustion MUST set `truncated: true`, emit a WARN
  naming the counts, and leave the exit status zero with the attestation
  written.
- **FR-016**: The budget MUST be operator-overridable, and the coverage
  record MUST state which source applied.
- **FR-017**: The witness attestor type MUST become
  `https://waybill.dev/attestation/compiler-invocation/v0.2`.
- **FR-018**: `predicateType` MUST remain
  `https://waybill.dev/attestation/build-trace/v1`, and the spec MUST record
  why the in-place change is sound.
- **FR-019**: The C130 `waybill:source-read-set` payload builder MUST be
  updated to the new field shape. Wiring C130 into emission remains #1142's
  scope; this feature only keeps the dormant builder compiling and correct.
- **FR-020**: Default behaviour when the `ebpf-tracing` feature is off MUST
  be unchanged, and no new Cargo dependency may be added.

### Key Entities

- **ReadSetEntry** — one path a compiler invocation read. Observation tier:
  `path`, `kind`. Close-time tier: `existed_at_close`, optional
  `sha256_at_close`.
- **WriteSetEntry** — one artifact a compiler invocation produced, keyed by
  its final observed path. Observation tier: `path`, `renamed_from`.
  Close-time tier: `existed_at_close`, optional `sha256_at_close`.
- **CompilerHashCoverage** — per-trace disclosure of what the close-time
  pass covered, counted over **distinct paths** rather than set entries:
  `paths_total`, `paths_hashed`, `paths_absent` (did not exist at close),
  `paths_skipped` (existed but not hashed — unreadable, not a regular file,
  or over budget), `truncated`, and the budget source. Lives last in
  `TraceIntegrity`, absent when no compiler invocations were captured.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On a real kernel trace of `two_binaries_diverge`, zero
  write-set entries report `existed_at_close: true` for a path absent from
  the filesystem at trace end, and at least one reports `false` — the
  positive control proving the pass ran.
- **SC-002**: On the same trace, zero entries in either set carry an
  all-zero content hash, and zero read-set entries carry a `sha256` field.
- **SC-003**: Every `sha256_at_close` emitted on that trace matches an
  independent `sha256sum` of the same path, for a sampled set of at least
  ten paths spanning both sets.
- **SC-004**: Every write-set entry for an artifact that rustc renamed into
  place has a non-empty `renamed_from`, and the write set contains no entry
  whose path is also listed in another entry's `renamed_from`.
- **SC-005**: Two consecutive traces of the same build produce byte-identical
  pre-signature canonical payloads, including the coverage record.
- **SC-006**: With the budget set below the build's hashable bytes, the
  attestation reports `truncated: true`, the command exits zero, and the set
  of hashed paths is identical across two such runs.
- **SC-007**: An attestation from a trace with zero compiler invocations has
  a serialized `trace_integrity` byte-identical to one produced before this
  feature.
- **SC-008**: The close-time pass adds no more than a ratio of total trace
  wall-clock established by the measurement committed under
  `measurements/`, against a baseline the same measurement records. No
  absolute cost figure enters this spec or the plan until measured.
- **SC-009**: `cargo +stable clippy --workspace --all-targets` and
  `cargo +stable test --workspace --no-fail-fast` pass, and the
  `ebpf-tracing` feature-on lane passes in a Linux container.
- **SC-010**: On a real kernel trace of `two_binaries_diverge`, every pair
  of set entries naming one path reports the same `existed_at_close` and the
  same `sha256_at_close`, and `paths_total` equals the number of distinct
  paths across all read and write sets — strictly less than the number of
  entries on this fixture, because `deps/liblibsafe-<hash>.rlib` is read
  under three distinct rustc invocations (measured; `measurements/findings.md`
  Q3).

## Assumptions

- Close-time is the only moment a hash can be taken. The kernel never
  hashes: every eBPF site sets `content_hash = [0; 32]` and defers to
  userspace. Hashing at ring-buffer drain time was considered and rejected —
  it is still after the read, races with the build, and costs a hash per
  read event.
- No external consumer depends on `read_set[].sha256` or
  `write_set[].survived_trace_window`. Both have only ever emitted constants,
  and `compiler_pipeline` is populated only on Linux with
  `--features ebpf-tracing`, which is off by default everywhere.
- The existing `hasher.rs` streaming hasher and its per-file `max_bytes` cap
  are reused unchanged; this feature adds no new hashing primitive.
- **A path is written by at most one compiler invocation per trace**, so a
  single close-time hash is never attributed to an invocation that did not
  produce the bytes. Measured, not assumed: no real path in milestone 1070's
  kernel strace is opened for writing by more than one pid, and the detector
  was proven to fire on an injected second writer. It is also structural —
  cargo embeds a metadata hash in every output filename and compiles each
  crate once per build, so two invocations sharing a final path would
  require two compilations sharing a metadata hash. Evidence and the one
  unmeasured route (build scripts writing a shared `OUT_DIR` path, which are
  not compiler invocations) are in `measurements/findings.md` Q1.
- Compiler outputs reach their final names by three measured mechanisms:
  binaries are opened directly with `O_TRUNC`; libraries are renamed into
  place from a randomized temporary, their final path never opened for
  writing; and cargo materialises the user-facing `release/<bin>` afterwards
  by hard link on Linux and by copy on macOS. `two_binaries_diverge`
  exercises the first two, which is why it is the fixture for US1 and US3.
- Verification requires a real kernel. macOS `cargo check` cannot see
  `#[cfg(all(target_os = "linux", feature = "ebpf-tracing"))]` code, so
  every acceptance scenario above is exercised in a Linux container.

## Out of Scope

- Wiring C130 `waybill:source-read-set` into SBOM emission (#1142). This
  feature updates the dormant payload builder's field shape only.
- Labelling the existing unlabelled close-time hash on `file_access`
  (`scan.rs:608`). Same defect class, separate issue.
- Any form of read-time or drain-time hashing.
- The missing user-facing artifact. `release/<bin>` — the file an operator
  actually ships — appears in no compiler write set, because cargo creates
  it after compilation and no compiler ever wrote it
  (`measurements/findings.md` Q2). A genuine provenance gap, found while
  writing this spec, but a different defect from #1144; it needs its own
  issue.
- Changes to `network_trace`, `trace_integrity`'s existing fields, or the
  `build-trace` predicate's URI.
