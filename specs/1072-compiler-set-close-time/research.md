# Phase 0 research — milestone 1072 (#1144)

Every finding below was read out of the tree at plan time, with
`file:line`. Two of them contradict the spec; both corrections are already
committed to `spec.md`. Measured behaviour lives in `measurements/`, not
here — this file records what the **code** says.

## R1. The kernel never hashes, so the zero is structural

**Decision**: close-time is the only moment a hash can exist. Confirmed
unchanged from the spec's assumption.

`waybill-cli/src/trace/compiler_pipeline.rs:229` converts the kernel's
`content_hash` bytes into a `ContentHash` with the comment "Zero-hash means
'not yet hashed' … user-space hasher.rs will fill in at close-time". Every
eBPF site sets `content_hash = [0; 32]`
(`waybill-ebpf/src/programs/file_ops.rs`), and all four probes are **entry**
kprobes, firing before the operation completes.

**Consequence for the design**: the observation tier should not merely
*avoid* using the kernel hash — it should be unable to carry one. The
aggregator's in-flight maps currently thread a fake hash through
(`PartialInvocation.read_set: HashMap<PathBuf, (ContentHash, ReadKind)>` at
`compiler_pipeline.rs:114`). Dropping `ContentHash` from those maps makes
the two-tier separation a type-level fact rather than a convention.

## R2. `sha256_file_hex` refuses oversized files — it does not cap

**Decision**: an over-cap path is skipped and counted in `paths_skipped`.
**This corrected the spec.**

```rust
// waybill-cli/src/trace/hasher.rs:49
pub fn sha256_file_hex(path: &Path, max_bytes: u64) -> std::io::Result<String> {
    let meta = std::fs::metadata(path)?;
    if meta.len() > max_bytes {
        return Err(std::io::Error::new(ErrorKind::InvalidData, …));
    }
```

The existing `file_access` pass handles that `Err` with
`tracing::debug!(path, error = %e, "could not hash artifact")` and moves on
(`waybill-cli/src/cli/scan.rs:871-877`). `DEFAULT_SIZE_CAP_BYTES` is 256 MB
(`waybill-cli/src/scan_fs/walker.rs:27`).

The spec had claimed such a file is "hashed to the cap exactly as the
existing `file_access` pass does" — wrong about the function and wrong about
the existing pass. Corrected in `spec.md`'s edge cases.

**Alternatives considered**: teaching `sha256_file_hex` to hash a prefix.
Rejected — a prefix hash is a *different* value that looks like a SHA-256 of
the file, which is the exact class of false statement #1144 is about. The
honest options are "hash it all" or "don't hash it", and `paths_skipped`
already expresses the second.

## R3. Rename handling already exists, and already moves the entry

**Decision**: extend the existing branch rather than add a new path.

`compiler_pipeline.rs:243-250` already handles `ObservedKind::Rename`:

```rust
ObservedKind::Rename { from } => {
    let carried = from.as_ref()
        .and_then(|old| invocation.write_set.remove(old))
        .unwrap_or((Some(hash), true));
    invocation.write_set.insert(path, carried);
}
```

So today's behaviour is exactly what the spec described — the entry moves to
the final path and the intermediate name is lost. US3 is therefore an
accumulation change inside a branch that already exists, not new plumbing.
`from` is `Option<PathBuf>` (`waybill-cli/src/trace/observed.rs:23`), so the
unresolved-source case is already modelled and FR-008's "observed
intermediate names" has a natural empty case.

## R4. `linkat` is not traced, so FR-009a holds by construction

**Decision**: no runtime guard. A regression test documents the invariant
and names the place that must honour it if a probe is ever added.

`ObservedKind` has exactly three variants — `Read`, `Write`,
`Rename { from }` (`observed.rs:18-24`) — and a tree-wide search for
`linkat` / `hard_link` across `waybill-ebpf/src`, `waybill-cli/src/trace`
and `waybill-common/src` returns **nothing**. The kernel side traces
`chdir`, `fchdir`, `renameat` and `renameat2`
(`waybill-ebpf/src/programs/fs_syscalls.rs:115-127`) and no link syscall.

Two consequences:

1. FR-009a cannot currently be violated — there is no link event to
   mistake for a rename. Writing a code guard would be writing a guard
   against an event that cannot arrive. The test asserts the invariant;
   `ObservedKind::Rename` is the single place that would have to keep it.
2. It gives the **proximate** mechanical cause for the `release/<bin>` gap
   in `measurements/findings.md` Q2, which that file explains only in terms
   of cargo's timing: even if the hard link were a compiler action, waybill
   would not see it, because no probe covers `linkat`. Both reasons are
   real and independent, which makes the out-of-scope issue's framing
   stronger, not weaker.

## R5. The wiring point, and `trace` needs to become mutable

**Decision**: the pass runs in `waybill-cli/src/cli/scan.rs` between
`compiler_agg.finalize()` (line 766) and `builder::build_attestation(...)`
(line 789), and writes its coverage record onto
`trace.trace_integrity`.

`trace` is bound immutably at `scan.rs:652` (`let trace = agg.finalize(…)`)
and moved into `build_attestation` whole
(`waybill-cli/src/attestation/builder.rs:47`, which takes
`trace: AggregatedTrace` plus `compiler_pipeline: Option<CompilerPipelineData>`
separately). `TraceIntegrity` is assembled inside `agg.finalize()` at
`waybill-cli/src/trace/aggregator.rs:502`, which is **before** the traced
command's products can be hashed, so the coverage record cannot be produced
there.

**Chosen**: change line 652 to `let mut trace` and assign the coverage
field after the pass. Minimal diff; `build_attestation`'s signature is
untouched.

**Alternative considered**: a seventh parameter on `build_attestation`.
Rejected — it widens a shared signature for one field that belongs to a
struct the function already receives.

## R6. The pass must not be cfg-gated, only its call site

**Decision**: `waybill-cli/src/trace/close_time.rs` compiles
unconditionally; the call in `scan.rs` keeps the existing
`#[cfg(all(target_os = "linux", feature = "ebpf-tracing"))]`.

This is the single highest-leverage decision in the plan. The pass is pure
`std::fs` plus `hasher.rs` over a `CompilerPipelineData` — it contains no
eBPF, so there is no reason for it to be invisible to a macOS
`cargo check`. With the module unconditional:

- `cargo +stable clippy --workspace --all-targets` type-checks it on the dev
  host, instead of the m666-style trap recorded in memory where
  `#[cfg(all(target_os = "linux", feature = "ebpf-tracing"))]` code is only
  ever compiled in Colima;
- its unit tests run in the default CI lane, satisfying Principle VII
  ("unit tests MUST run without elevated privileges") for the whole of
  FR-001 – FR-016;
- Colima is then needed only for the genuinely kernel-dependent
  assertions — SC-001 through SC-006 on a real trace.

**Testing approach**: tempdir-based unit tests against the real filesystem.
Build a synthetic `CompilerPipelineData` whose paths point into a
`tempfile::tempdir()`, delete some, `chmod 000` one, replace one with a
directory, then run the pass and assert the counts and hashes.

**Alternative considered**: a `CloseTimeFs` trait with a mock
implementation. Rejected — it adds an indirection layer whose only consumer
is the test, and a mock filesystem cannot catch the cases that actually
matter here (a path that is a directory, a dangling symlink, an unreadable
file). The real filesystem in a tempdir tests the real code path and needs
no privileges.

## R7. No golden or fixture JSON carries the changed fields

**Decision**: no golden regeneration task is needed.

A tree-wide search for `survived_trace_window` outside `specs/` and
`target/` returns exactly three source files —
`waybill-common/src/attestation/compiler_pipeline.rs`,
`waybill-cli/src/trace/compiler_pipeline.rs`,
`waybill-cli/src/generate/compiler_pipeline_annotation.rs` — and no `.json`
under `waybill-cli/tests/fixtures/`. The compiler-pipeline fixtures are
hand-authored Cargo projects, not recorded attestations
(`waybill-cli/tests/fixtures/compiler_pipeline/REGEN.md`: "this is a
hand-authored fixture, not generated").

This is why the release-bump golden lessons in memory do not apply to this
milestone: there is no recorded attestation to regenerate. The in-tree test
constructors that will need updating are
`compiler_pipeline.rs:340-360` (`write_set_entry_omits_none_sha256`,
`zero_sha256`) and `compiler_pipeline_annotation.rs:291-302`.

## R8. The attestor URI is duplicated in two constants

**Decision**: bump both, and note the duplication rather than refactor it.

`compiler-invocation/v0.1` appears as a `const` in two places —
`waybill-common/src/attestation/compiler_pipeline.rs:216` and
`waybill-common/src/attestation/witness.rs:57` — each asserted by its own
test (`:399` and `:568`). FR-017 needs both. Collapsing them to one
constant is a tidy-up this feature does not need and would widen the diff
for no behavioural gain.

The enclosing `predicateType` stays `build-trace/v1` per FR-018, and the
m210 contract that documents the old shape
(`specs/210-compiler-pipeline-trace/contracts/attestor-predicate.md` C-1,
C-2) is **superseded** by this milestone's contract rather than edited:
milestone specs are historical records of what was decided then.

## R9. C130's payload builder dereferences the field being removed

**Decision**: emit `sha256_at_close`, omitted when absent.

`waybill-cli/src/generate/compiler_pipeline_annotation.rs:129` is
`"sha256": entry.sha256.value.as_str()` — an unconditional deref that stops
compiling the moment `ReadSetEntry::sha256` is gone. So the C130 decision
cannot be silently deferred to #1142, which is what FR-019 exists to say.

The same labelling discipline that motivates the whole feature applies to
the annotation key: a consumer reading `"sha256"` on a *source read set*
would believe it was the content read. The key becomes `sha256_at_close`
and is omitted when there is no hash. Emission wiring stays #1142's.

## R10. Budget default: deliberately unresolved

**Status**: NEEDS MEASUREMENT — and that is the first implementation task,
not a clarification to chase.

SC-008 forbids an absolute cost figure entering the spec or plan before
measurement, per the project rule on measuring external behaviour. The
budget is therefore a **required parameter** of the pass from the first
commit, with the default constant landing only after the measurement task
records a figure under `measurements/`.

Flag shape follows the existing numeric-flag precedent
(`--rekor-timeout-secs` at `waybill-cli/src/cli/edit.rs:136`), but as
`Option<u64>` with **no** `default_value_t`:

```rust
#[arg(long, env = "WAYBILL_COMPILER_HASH_BUDGET_BYTES", value_name = "BYTES")]
pub compiler_hash_budget_bytes: Option<u64>,
```

on `ScanArgs` (`waybill-cli/src/cli/scan.rs:6`). `None` → the default
constant and `BudgetSource::Default`; `Some(_)` → `BudgetSource::Operator`.
This makes FR-016's "which source applied" fall out of the type instead of
requiring `ArgMatches::value_source`, which the `Args`-derive path does not
hand back, and it avoids a sentinel value standing in for "unset".
