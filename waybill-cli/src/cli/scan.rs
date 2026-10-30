use std::path::PathBuf;

use clap::Args;

#[derive(Args)]
pub struct ScanArgs {
    #[arg(long)]
    pub target_pid: Option<u32>,
    #[arg(long, default_value = "waybill.attestation.json")]
    pub output: PathBuf,
    #[arg(long)]
    pub trace_children: bool,
    #[arg(long)]
    pub libssl_path: Option<PathBuf>,
    #[arg(long)]
    pub go_binary: Option<PathBuf>,
    #[arg(long, default_value = "8388608")]
    pub ring_buffer_size: u32,
    #[arg(long, default_value = "0")]
    pub timeout: u64,
    #[arg(long)]
    pub json: bool,
    /// Directories to scan for freshly-landed artifact files after the
    /// traced command exits. Any recognised package file (`.deb`,
    /// `.crate`, `.whl`, `.tar.gz`, …) whose mtime is newer than the
    /// trace start is hashed and added to the file-access record, so
    /// the resulting SBOM carries real content hashes even when the
    /// kernel-side kprobe misses the output-file open (observed with
    /// curl's -O and cargo's .crate writes).
    /// Accepts the flag multiple times or comma-separated.
    #[arg(long, value_delimiter = ',')]
    pub artifact_dir: Vec<PathBuf>,
    /// Auto-detect artifact directories from the traced command. Matches
    /// `argv[0]` against a table of known build tools (cargo, pip, npm,
    /// go, apt-get, …) and merges the canonical cache paths with any
    /// explicit `--artifact-dir` values. Skipped for shell-wrapped
    /// commands (`bash -c "…"`) — those are too dynamic to introspect.
    #[arg(long)]
    pub auto_dirs: bool,

    // ─────────────────────────────────────────────────────────────
    // Feature 006 — DSSE signing flags. See specs/006-sbomit-suite/
    // contracts/cli.md for the full contract.
    // ─────────────────────────────────────────────────────────────
    /// Path to a PEM-encoded private key for local-key DSSE signing.
    /// Mutually exclusive with `--keyless`.
    #[arg(long, conflicts_with = "keyless")]
    pub signing_key: Option<PathBuf>,

    /// Name of the env var holding the passphrase for an encrypted
    /// `--signing-key`. No effect on unencrypted keys. No interactive
    /// prompt — CI-friendly by design.
    #[arg(long, value_name = "NAME")]
    pub signing_key_passphrase_env: Option<String>,

    /// Use keyless signing via OIDC → Fulcio → Rekor. Mutually
    /// exclusive with `--signing-key`.
    #[arg(long)]
    pub keyless: bool,

    /// Override the Fulcio certificate-issuance URL.
    #[arg(long, default_value = "https://fulcio.sigstore.dev")]
    pub fulcio_url: String,

    /// Override the Rekor transparency-log URL.
    #[arg(long, default_value = "https://rekor.sigstore.dev")]
    pub rekor_url: String,

    /// Skip Rekor upload + inclusion-proof embedding. Keyless mode
    /// only; with this flag the envelope carries the Fulcio cert alone.
    #[arg(long)]
    pub no_transparency_log: bool,

    /// Fail the command if no signing identity was configured. Flips
    /// the default "emit unsigned + warn" behavior to a hard error.
    #[arg(long)]
    pub require_signing: bool,

    /// Explicit subject artifact path. Repeatable. When set,
    /// auto-detection is suppressed — waybill signs exactly what you
    /// told it to (FR-009).
    #[arg(long = "subject", value_name = "PATH")]
    pub subject: Vec<PathBuf>,

    /// Attestation output format. `witness-v0.1` emits an in-toto
    /// Statement v0.1 wrapped around a witness attestation-collection
    /// (`material` + `command-run` + `product` + `network-trace`
    /// inner attestors), directly consumable by `sbomit generate` and
    /// any go-witness-aware verifier. `waybill-v1` emits waybill's
    /// native `BuildTracePredicate` Statement v1 — richer network-
    /// trace semantics but only waybill understands it.
    #[arg(long = "attestation-format", value_name = "FORMAT", default_value = "witness-v0.1")]
    pub attestation_format: String,

    /// Milestone 210 (FR-016) — bypass the compiler-pipeline aggregator's
    /// default denylist for `/etc/`, `/proc/`, `/sys/`, `/dev/`, user
    /// cache directories, `/tmp/`, and secret-adjacent paths. Off by
    /// default so per-component `waybill:source-read-set` annotations
    /// stay signal-heavy (system reads dominate a build's read syscalls
    /// but almost never disambiguate which SOURCE input produced a
    /// binary). Turn on for kernel/toolchain SBOM audits where reads
    /// under `/usr/include`, `/usr/lib`, or `/dev/urandom` are relevant.
    #[arg(long)]
    pub include_system_reads: bool,

    /// Milestone 1072 (FR-016) — cumulative byte budget for the
    /// close-time hashing pass. Absent means the measured default;
    /// supplying it marks the coverage record `budget_source: "operator"`
    /// so a reader can tell a tuned run from a stock one (C-1072-8).
    ///
    /// `0` is meaningful: hash nothing, report every path's existence,
    /// and set `truncated: true`. The command still exits zero — hashing
    /// is an advisory tier, and truncating it loses no observation
    /// (C-1072-10).
    #[arg(
        long,
        env = "WAYBILL_COMPILER_HASH_BUDGET_BYTES",
        value_name = "BYTES"
    )]
    pub compiler_hash_budget_bytes: Option<u64>,

    #[arg(last = true)]
    pub command: Vec<String>,
}

/// Default cumulative budget for the close-time hashing pass: 16 GiB.
///
/// **Measured against a real workspace** — see
/// `measurements/cost.md` §"Sizing the default against a real workspace"
/// and `measurements/budget-probe.sh`, which `contracts/cli-budget.md`
/// C-1072-9 requires this definition to cite.
///
/// This was 2 GiB until the probe ran, sized at ~2,200x the
/// `two_binaries_diverge` fixture's 955,494 bytes. A four-crate fixture
/// is the wrong denominator for a global default, and measuring the right
/// one moved the headroom by three orders of magnitude. Demand for a
/// `--release --workspace --all-targets` build of this repository, over
/// six runs and two toolchain layouts:
///
/// | traced build | bytes demanded | % of the old 2 GiB |
/// |---|---|---|
/// | `two_binaries_diverge` fixture | 955,494 | 0.04% |
/// | `--release --bin waybill` | 880,376,492 | 41% |
/// | `--all-targets`, toolchain under `/usr/` | 1.39–1.52 GB | 65–71% |
/// | `--all-targets`, toolchain under `~/` | 1.77–2.00 GB | **82–93%** |
///
/// The last row is the ordinary case: `~/.cargo` and `~/.rustup` match no
/// m213 filter pattern, so dependency-source and sysroot reads enter the
/// read set, where under `/usr/` the System filter hides them. At 82–93%
/// consumed by a normal build of this repository the old default was
/// effectively breached already, and truncation costs real data. 16 GiB
/// is ~8-9x that demand.
///
/// Ranges, not points, deliberately: path capture varied 15,029–23,200
/// between runs of one build. Whether that variance is disclosed loss or
/// silent non-determinism is open — findings.md Q7.
///
/// This constant does **not** enforce SC-008's 5% ratio and cannot: a
/// full spend against a one-second trace would be thousands of percent.
/// What the probe established is that raising the budget does not
/// endanger the ratio either — the pass hashes only what the trace
/// observed, so the budget caps that work rather than creating it.
///
/// SC-008's ceiling is nonetheless **exceeded in 2 of 9 observations**
/// (5.2% and 7.1%), which is a property of the criterion rather than of
/// the pass: its absolute cost is 6.3–24.8 s for ~2 GB, against builds
/// that took 118.9–857.6 s for identical work on one host, so the ratio
/// is breached exactly when the host is fast. Recorded in findings.md
/// Q7 with three options and a recommendation; the ceiling is unchanged
/// pending that decision, per spec.md's rule against silent
/// re-baselining. No throughput figure is quoted either: it spread
/// 80–479 MB/s on one host, so the ~120 MB/s an earlier revision of this
/// comment cited was as unfounded as the ~478 MB/s it replaced.
///
/// The budget bounds the case where bytes stop scaling with build
/// duration, and the probe names it: **the read set, not the write set.**
/// Artifacts cannot be produced faster than the build runs, but read-set
/// paths existed before the trace and nothing it can see bounds their
/// size — 184,628 read entries against 7,012 write entries in the wider
/// build. `truncated` plus the FR-015 WARN is what an operator sees when
/// it engages.
///
/// Still 64x the 256 MB per-file cap, so the two limits do not interact
/// in the normal case.
///
/// Only the gated call site reads it, so off the eBPF path it is dead —
/// same bind as `trace/close_time.rs` and `trace/cwd.rs`, and the same
/// attribute.
#[cfg_attr(
    not(all(target_os = "linux", feature = "ebpf-tracing")),
    allow(dead_code)
)]
pub const DEFAULT_COMPILER_HASH_BUDGET_BYTES: u64 = 16 * 1024 * 1024 * 1024;

impl ScanArgs {
    /// Build a [`SigningIdentity`] from the current flag combination.
    /// Returns an error when `--require-signing` is set but no identity
    /// was configured. Only invoked from the Linux-only
    /// `execute_scan` block; gate the method itself.
    #[cfg(all(target_os = "linux", feature = "ebpf-tracing"))]
    pub fn build_signing_identity(
        &self,
    ) -> anyhow::Result<crate::attestation::signer::SigningIdentity> {
        use crate::attestation::signer::{OidcProvider, SigningIdentity};
        match (self.signing_key.as_ref(), self.keyless) {
            (Some(path), false) => Ok(SigningIdentity::LocalKey {
                path: path.clone(),
                passphrase_env: self.signing_key_passphrase_env.clone(),
            }),
            (None, true) => Ok(SigningIdentity::Keyless {
                fulcio_url: self.fulcio_url.clone(),
                rekor_url: self.rekor_url.clone(),
                oidc_provider: OidcProvider::detect(),
                transparency_log: !self.no_transparency_log,
            }),
            (None, false) => {
                if self.require_signing {
                    anyhow::bail!(
                        "--require-signing set but no signing identity configured; \
                        pass --signing-key <PATH> or --keyless"
                    );
                }
                Ok(SigningIdentity::None)
            }
            (Some(_), true) => {
                // `conflicts_with` on clap prevents this path, but keep
                // the defensive check in case the struct is built by hand.
                anyhow::bail!("--signing-key and --keyless are mutually exclusive")
            }
        }
    }
}

pub async fn execute(args: ScanArgs) -> anyhow::Result<()> {
    if args.target_pid.is_none() && args.command.is_empty() {
        anyhow::bail!("either --target-pid or a command (after --) is required");
    }
    if args.target_pid.is_some() && !args.command.is_empty() {
        anyhow::bail!("--target-pid and command are mutually exclusive");
    }
    match args.attestation_format.as_str() {
        "witness-v0.1" | "waybill-v1" => {}
        other => anyhow::bail!(
            "unknown --attestation-format {other:?}; accepted: witness-v0.1, waybill-v1"
        ),
    }
    execute_scan(args).await
}

/// Derive a human-readable `Collection.name` from the traced command.
/// Uses `argv[0]` basename — matches the convention `go-witness` uses
/// when you pass `--step <name>`, but auto-derived. Only invoked from
/// the Linux-only execute_scan block.
#[cfg(all(target_os = "linux", feature = "ebpf-tracing"))]
fn default_collection_name(cmd: &str) -> String {
    cmd.split_whitespace()
        .next()
        .unwrap_or("build")
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("build")
        .to_string()
}

#[cfg(all(target_os = "linux", feature = "ebpf-tracing"))]
async fn execute_scan(args: ScanArgs) -> anyhow::Result<()> {
    use std::time::{Duration, Instant};

    use aya::maps::{MapData, RingBuf};

    use crate::attestation::builder::{self, AttestationConfig};
    use crate::attestation::serializer;
    use crate::error::MikebomError;
    use crate::trace::aggregator::EventAggregator;
    use crate::trace::close_time;
    use crate::trace::compiler_pipeline::{
        CompilerPipelineAggregator, FilterConfig as CompilerFilterConfig,
    };
    use crate::trace::cwd::{CwdTracker, Resolution};
    use crate::trace::loader::{self, LoaderConfig};
    use crate::trace::observed::{self, ObservedKind, ObservedOp};
    use crate::trace::aggregator::TraceStats;
    use waybill_common::events::{
        CompilerExecEvent, FileEvent, FileEventType, LineageEvent, NetworkEvent, RenameEvent,
    };
    use waybill_common::types::timestamp::Timestamp;

    let trace_start = Timestamp::now();
    // Wall-clock at trace start, used below to filter artifact directories
    // for files that appeared during this trace (mtime ≥ trace_start_wall).
    // Subtract 1 s to tolerate filesystem timestamp granularity — the worst
    // case is that we hash one file that pre-existed, which is harmless.
    let trace_start_wall = std::time::SystemTime::now()
        .checked_sub(std::time::Duration::from_secs(1))
        .unwrap_or_else(std::time::SystemTime::now);
    tracing::info!("Starting eBPF trace");

    // Sample CLOCK_BOOTTIME vs CLOCK_REALTIME up front. bpf_ktime_get_ns
    // returns CLOCK_BOOTTIME; adding this offset converts it to wall clock.
    let boot_offset_ns = compute_boot_offset_ns();
    tracing::debug!(boot_offset_ns, "computed boot→wall offset");

    // Load eBPF FIRST so probes are active before child spawns
    let target_pid = args.target_pid.unwrap_or(std::process::id());
    // #1185 — the per-trace handshake nonce. Only command-execute mode can
    // seed a traced root: `--target-pid` attaches to a process the tracer
    // never spawned, so there is no `pre_exec` in which to announce it.
    //
    // Derived from the clock rather than a CSPRNG, and that is sufficient
    // for what the nonce defends against: it keeps one trace's handshake
    // from arming another's on the same host. It is not a secret, because
    // the gate is positive — forging the handshake adds the forger to the
    // traced set and can exclude nothing. Masked to 31 bits so the value
    // stays a positive `int` when it is passed as the syscall's flags
    // argument, and forced non-zero because zero means "unarmed".
    let handshake_nonce: Option<u32> = if args.target_pid.is_none() {
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos() ^ (d.as_secs() as u32))
            .unwrap_or(0x5A5A_5A5A);
        Some((t & 0x7FFF_FFFF).max(1))
    } else {
        None
    };

    let mut handle = loader::load_and_attach(&LoaderConfig {
        target_pid,
        libssl_path: args.libssl_path.clone(),
        ring_buffer_size: args.ring_buffer_size,
        ebpf_object: None,
        trace_children: args.trace_children,
        // Milestone 213 (issue #616) — plumbs --include-system-reads
        // to the kernel-side FILTER_WIDEN[0] slot per FR-010.
        include_system_reads: args.include_system_reads,
        handshake_nonce,
    })?;
    // #618 / #1185 — the drop slots as they stand now, before the child
    // exists. The open probes attach exit-before-entry deliberately, and
    // the probes are system-wide, so the attach window counts every open
    // on the machine as an exit without a request. Baselined here and
    // subtracted at trace end, which is what makes the remainder mean
    // "entry events were lost during the build".
    let event_drop_baseline =
        crate::trace::counters::read_kernel_event_drop_baseline(&mut handle.bpf);

    // #1252 — the probe names, kept for the attestation. Cloned here
    // because `handle.bpf` is borrowed mutably further down.
    let probe_attach_failures = handle.attach_failures.kprobe.clone();
    if probe_attach_failures.is_empty() {
        tracing::info!("eBPF probes attached");
    } else {
        tracing::warn!(
            failed = ?probe_attach_failures,
            "eBPF probes attached with gaps; this capture is a floor, not a total"
        );
    }

    // THEN spawn child
    //
    // Milestone 1070 (#614): the child is given an explicit working
    // directory, though it is the one it would inherit anyway. Setting it
    // makes the child perform a `chdir` before exec, which the trace
    // observes like any other.
    //
    // #1185: the directory is also kept here, because this is the one cwd
    // in the system the tracer *established* instead of inferring. It seeds
    // the model directly below, so the build still resolves its relative
    // reads when that `chdir` record is lost. Spawning is the only path that
    // has it — `--target-pid` attaches to a process whose directory the
    // tracer never chose, and that case stays purely observational.
    let spawn_dir = if args.target_pid.is_none() {
        Some(std::env::current_dir().map_err(|e| {
            anyhow::anyhow!("cannot read the working directory to start the traced command in: {e}")
        })?)
    } else {
        None
    };
    let mut child = if let Some(dir) = spawn_dir.as_ref() {
        let cmd = &args.command;
        tracing::info!(command = %cmd.join(" "), "Spawning traced command");
        let mut builder = std::process::Command::new(&cmd[0]);
        builder.args(&cmd[1..]).current_dir(dir);

        // #1185 — the traced root announces itself to the kernel, from
        // inside the child, after fork and before exec.
        //
        // It has to come from the child, and the kernel has to learn the
        // pid by observing rather than being told: the tracer cannot see
        // its own or its child's initial-namespace pid (`std::process::id()`
        // and `/proc/self/status`'s `NSpid` both report the container's
        // view), and that mismatch is what silently disabled the
        // tracer-exclusion filter in every containerised run.
        //
        // `pre_exec` runs in a forked child, so the closure must be
        // async-signal-safe: one raw syscall, no allocation, no locks, no
        // panic path. A null path is passed deliberately — the handshake
        // carries no filename, and the kernel's entry program reads only
        // `dfd` and `flags` for it. The call fails with `EBADF`, which is
        // the intended outcome and is why the result is discarded here
        // rather than reported: its success is not the signal, being
        // *observed* is, and that is verified from the map below.
        if let Some(nonce) = handshake_nonce {
            unsafe {
                use std::os::unix::process::CommandExt;
                builder.pre_exec(move || {
                    libc::syscall(
                        libc::SYS_openat,
                        waybill_common::maps::TRACE_HANDSHAKE_DFD,
                        core::ptr::null::<libc::c_char>(),
                        nonce as libc::c_int,
                    );
                    Ok(())
                });
            }
        }

        let c = builder
            .spawn()
            .map_err(|e| anyhow::anyhow!("failed to spawn: {e}"))?;
        tracing::info!(pid = c.id(), "Child started");
        Some(c)
    } else {
        None
    };

    // #1185 — the root must be established before the trace proceeds.
    //
    // Deliberately before the drain loop and deliberately fatal. The
    // handshake is the one thing the whole subtree rests on: with it, the
    // kernel knows the traced root and propagates to every descendant on
    // fork; without it the set stays empty, and an empty set is the state
    // in which a positive gate drops every open and the attestation still
    // says `complete`. That is #1185's exact signature, so it is made
    // unreachable here rather than guarded later.
    //
    // 2 seconds: the handshake is one syscall between fork and exec, so
    // this is orders of magnitude of headroom. A timeout means the
    // mechanism is broken, not that the machine was busy.
    if handshake_nonce.is_some() {
        let seeded = crate::trace::counters::wait_for_traced_root(
            &mut handle.bpf,
            std::time::Duration::from_secs(2),
        )?;
        debug_assert!(seeded > 0);
    }

    let child_pid = child.as_ref().map(|c| c.id()).unwrap_or(target_pid);

    // Poll ring buffers while child runs
    let mut agg = EventAggregator::with_boot_offset(boot_offset_ns);
    // Milestone 210 — compiler-pipeline aggregator. Populated by the
    // sched_process_exec + sched_process_fork + sched_process_exit
    // tracepoints via the COMPILER_EXEC_EVENTS ring buffer + the
    // existing FILE_EVENTS ring buffer (stamped in userspace against
    // pid_to_invocation_id). `home_dir` seeds the FR-016 secrets
    // denylist glob expansion for `~/.ssh/*`, `~/.aws/*`, etc.
    let mut compiler_agg = CompilerPipelineAggregator::new(CompilerFilterConfig {
        include_system_reads: args.include_system_reads,
        home_dir: std::env::var("HOME").ok().map(std::path::PathBuf::from),
    });
    // #1252 — a read set assembled without one of the open-family kprobes
    // is incomplete by construction, so it must not finalize as `Complete`.
    if !probe_attach_failures.is_empty() {
        compiler_agg.mark_probe_attach_failed();
    }
    // Milestone 1070 — working directory per traced process, from observed
    // forks and directory changes, used to resolve relative paths.
    let mut cwd = CwdTracker::new();
    // #1185 — seed the root from the directory the tracer set, before any
    // event is drained. Both the tracer and the child are seeded: the
    // child's `sched_process_fork` record names the tracer as its parent, so
    // without the tracer's entry that record would overwrite the child's
    // seed with `Unknown`. With both, either event surviving is enough and
    // the two agree. Descendants keep inheriting through fork records as
    // before; this hardens the root, which is what poisons everything
    // downstream when it is lost.
    if let Some(dir) = spawn_dir {
        cwd.seed(std::process::id(), dir.clone());
        cwd.seed(child_pid, dir);
    }
    let mut compiler_count: u64 = 0;
    let mut net_count: u64 = 0;
    let mut file_count: u64 = 0;
    let start = Instant::now();
    let timeout = if args.timeout > 0 {
        Some(Duration::from_secs(args.timeout))
    } else {
        None
    };

    // Per-iteration drain is capped so a high event rate cannot starve the
    // child-exit check. The post-exit drain uses the same cap with a short
    // settling loop so events queued before the probes see the exit still
    // land in the aggregator.
    const MAX_PER_ITER: usize = 4096;

    // Userspace PID filter. Semantics:
    //   --trace-children  → no userspace filter (kernel still drops the
    //                       tracer's own events via should_trace). Build
    //                       processes frequently fork short-lived helpers
    //                       (apt-get's http method, cargo's rustc workers)
    //                       that exit before a /proc scan catches them, so
    //                       following the subtree conservatively drops
    //                       legitimate events. Pick up system noise over
    //                       missing the build's real activity.
    //   default           → restrict to the direct child PID. Good when the
    //                       traced command does all its own I/O (curl,
    //                       wget, a single binary that links libssl).
    let mut target_pids: std::collections::HashSet<u32> = std::collections::HashSet::new();
    target_pids.insert(child_pid);
    // Milestone 211 (issue #611) — pid-filter now applies ONLY when the
    // operator explicitly attached to an existing pid via `--target-pid`.
    // The "run a command and trace it" path (`waybill trace run -- ...`)
    // ALWAYS wants to see events from the entire process subtree —
    // cargo builds fan out to rustc / cc / ld / etc., all of which are
    // legitimate build activity the SBOM depends on. Pre-m211, the
    // default filter dropped every child-process event silently,
    // producing empty `file_access.operations[]` on every build trace
    // even when the kprobes were firing. `--trace-children` is now
    // implicitly always-true for command-execute mode; the flag stays
    // for backwards compat + as an explicit opt-in for --target-pid
    // scenarios where descendant tracking is desired.
    let filter_by_pid = args.target_pid.is_some() && !args.trace_children;

    fn drain_network(
        rb: &mut RingBuf<MapData>,
        agg: &mut EventAggregator,
        count: &mut u64,
        max: usize,
        target_pids: &std::collections::HashSet<u32>,
    ) -> usize {
        let mut n = 0;
        while n < max {
            match rb.next() {
                Some(item) => {
                    let data: &[u8] = item.as_ref();
                    if data.len() >= core::mem::size_of::<NetworkEvent>() {
                        let ev = unsafe {
                            core::ptr::read_unaligned(data.as_ptr() as *const NetworkEvent)
                        };
                        if target_pids.is_empty() || target_pids.contains(&ev.pid) {
                            agg.handle_network_event(&ev);
                            *count += 1;
                        }
                        n += 1;
                    }
                }
                None => break,
            }
        }
        n
    }

    /// Milestone 210 — drain the compiler-pipeline ring buffer. Feeds
    /// events into the CompilerPipelineAggregator + the file-op
    /// events into the file-aggregator via the existing pid map.
    /// When the COMPILER_EXEC_EVENTS map is unavailable (older eBPF
    /// object, tracepoint attach failed), returns 0 no-op.
    fn drain_compiler(
        rb: Option<&mut RingBuf<MapData>>,
        compiler_agg: &mut CompilerPipelineAggregator,
        count: &mut u64,
        max: usize,
    ) -> usize {
        let Some(rb) = rb else {
            return 0;
        };
        let mut n = 0;
        while n < max {
            match rb.next() {
                Some(item) => {
                    let data: &[u8] = item.as_ref();
                    if data.len() >= core::mem::size_of::<CompilerExecEvent>() {
                        let ev = unsafe {
                            core::ptr::read_unaligned(
                                data.as_ptr() as *const CompilerExecEvent,
                            )
                        };
                        compiler_agg.handle_compiler_event(&ev);
                        *count += 1;
                        n += 1;
                    }
                }
                None => break,
            }
        }
        n
    }

    /// Milestone 1070: `FILE_EVENTS` carries four record shapes. Each starts
    /// with its kind as a `u32`, which is checked against the known values
    /// *before* the record is decoded: an unknown value is not a valid
    /// `FileEventType`, so decoding first would be undefined behaviour.
    ///
    /// Fork and directory-change records feed the working-directory model.
    /// Opens and renames are resolved against it, then handed to both
    /// aggregators as one `ObservedOp`, so they agree on path and kind.
    #[allow(clippy::too_many_arguments)]
    fn drain_file(
        rb: &mut RingBuf<MapData>,
        agg: &mut EventAggregator,
        compiler_agg: &mut CompilerPipelineAggregator,
        cwd: &mut CwdTracker,
        count: &mut u64,
        max: usize,
        target_pids: &std::collections::HashSet<u32>,
        drops: &mut crate::trace::counters::EventDropCounts,
    ) -> usize {
        fn resolved(r: Resolution, raw: &str) -> (String, bool) {
            match r {
                Resolution::Resolved(p) => (p.to_string_lossy().into_owned(), false),
                Resolution::Unresolved => (raw.to_string(), true),
            }
        }
        let admitted = |pid: u32| target_pids.is_empty() || target_pids.contains(&pid);

        let mut n = 0;
        while n < max {
            let Some(item) = rb.next() else { break };
            n += 1;
            let data: &[u8] = item.as_ref();
            let Some(head) = data.get(..4) else { continue };
            let raw_kind = u32::from_ne_bytes([head[0], head[1], head[2], head[3]]);
            let Some(kind) = FileEventType::from_raw(raw_kind) else {
                // #618: a record kind this build does not know. Skipping it
                // is right; doing so invisibly was not.
                drops.record(waybill_common::events::EventDropReason::UserUnknownRecordKind);
                continue;
            };
            match kind {
                FileEventType::Fork => {
                    if data.len() >= core::mem::size_of::<LineageEvent>() {
                        // SAFETY: length checked; the kind was validated above.
                        let ev = unsafe { core::ptr::read_unaligned(data.as_ptr() as *const LineageEvent) };
                        cwd.fork(ev.parent_pid, ev.child_pid);
                    }
                }
                FileEventType::Rename => {
                    if data.len() < core::mem::size_of::<RenameEvent>() {
                        continue;
                    }
                    // SAFETY: length checked; the kind was validated above.
                    let ev = unsafe { core::ptr::read_unaligned(data.as_ptr() as *const RenameEvent) };
                    if !admitted(ev.pid) {
                        continue;
                    }
                    let from = match cwd.resolve(ev.pid, ev.old_dfd, ev.old_path_str(), ev.old_truncated != 0) {
                        Resolution::Resolved(p) => Some(p),
                        Resolution::Unresolved => None,
                    };
                    let raw = ev.new_path_str();
                    let (path, unresolved) =
                        resolved(cwd.resolve(ev.pid, ev.new_dfd, raw, ev.new_truncated != 0), raw);
                    let op = ObservedOp {
                        kind: ObservedKind::Rename { from },
                        path,
                        raw_path: raw.to_string(),
                        unresolved,
                        pid: ev.pid,
                        tid: ev.tid,
                        comm: ev.comm_str().to_string(),
                        timestamp_ns: ev.timestamp_ns,
                        bytes: 0,
                        content_hash: [0u8; 32],
                    };
                    agg.handle_op(&op);
                    compiler_agg.handle_op(&op);
                    *count += 1;
                }
                _ => {
                    if data.len() < core::mem::size_of::<FileEvent>() {
                        drops.record(waybill_common::events::EventDropReason::UserShortRecord);
                        continue;
                    }
                    // SAFETY: length checked; the kind was validated above.
                    let ev = unsafe { core::ptr::read_unaligned(data.as_ptr() as *const FileEvent) };
                    match kind {
                        FileEventType::Chdir => cwd.chdir(ev.pid, ev.path_str(), ev.path_truncated != 0),
                        FileEventType::Fchdir => cwd.fchdir(ev.pid),
                        // #1252 — a discarded open request, reported only
                        // when the diagnostic slot is on. Logged and
                        // dropped: it is the evidence that a read-set entry
                        // which disappeared belongs to a failed request, and
                        // it must never become a claim that the build read
                        // the file. `flags` carries the negated errno for
                        // this record kind.
                        //
                        // Resolved against the process's working directory
                        // first, so a relative request can be matched
                        // against the read set it was removed from.
                        FileEventType::OpenFailed => {
                            if !admitted(ev.pid) {
                                continue;
                            }
                            let raw = ev.path_str();
                            let (path, _unresolved) = resolved(
                                cwd.resolve(ev.pid, ev.dfd, raw, ev.path_truncated != 0),
                                raw,
                            );
                            tracing::info!(
                                path = %path,
                                errno = ev.flags,
                                pid = ev.pid,
                                comm = %ev.comm_str(),
                                "#1252 discarded open: the request did not succeed"
                            );
                        }
                        _ => {
                            if !admitted(ev.pid) {
                                drops.record(waybill_common::events::EventDropReason::UserPidNotAdmitted);
                                continue;
                            }
                            // Not a drop: a record kind that is not a file
                            // operation has nothing to contribute to a read
                            // or write set by construction.
                            let Some(op_kind) = observed::kind_of(&ev) else { continue };
                            let raw = ev.path_str();
                            if raw.is_empty() || raw == "<invalid>" {
                                // #618 / #1185: where a kernel-side
                                // `kernel_path_unreadable` lands if the event
                                // is submitted anyway. The two should move
                                // together; if this one moves alone, the path
                                // was lost between the read and here.
                                drops.record(waybill_common::events::EventDropReason::UserEmptyPath);
                                continue;
                            }
                            let (path, unresolved) =
                                resolved(cwd.resolve(ev.pid, ev.dfd, raw, ev.path_truncated != 0), raw);
                            let op = ObservedOp::with_path(&ev, op_kind, path, unresolved);
                            // Milestone 211 (post-#611 follow-up): route to
                            // both aggregators, so the compiler pipeline's
                            // read and write sets fill alongside the
                            // document-level file operations.
                            agg.handle_op(&op);
                            compiler_agg.handle_op(&op);
                            *count += 1;
                        }
                    }
                }
            }
        }
        n
    }

    // #991 — open each ring buffer once, for the whole loop. The drain
    // helpers used to build a `RingBuf` on every call, three times per 5 ms
    // iteration, and in CI that made reads cost ~7 ms per item (89.9 s for
    // 12,290 network events). The loop only checks for child exit between
    // drain rounds, so a 0.12 s traced `cargo build` took 80-121 s to reap.
    // Held open: 159 ms, and ring_buffer_overflows went from 152-3379 to 0.
    let mut network_rb = RingBuf::try_from(
        handle
            .bpf
            .take_map("NETWORK_EVENTS")
            .expect("NETWORK_EVENTS ring buffer is statically declared in the eBPF object"),
    )
    .expect("NETWORK_EVENTS map shape is BPF_MAP_TYPE_RINGBUF by construction");
    let mut file_rb = RingBuf::try_from(
        handle
            .bpf
            .take_map("FILE_EVENTS")
            .expect("FILE_EVENTS ring buffer is statically declared in the eBPF object"),
    )
    .expect("FILE_EVENTS map shape is BPF_MAP_TYPE_RINGBUF by construction");
    // Absent from older eBPF objects, or when the tracepoint attach failed.
    let mut compiler_rb = handle
        .bpf
        .take_map("COMPILER_EXEC_EVENTS")
        .and_then(|m| RingBuf::try_from(m).ok());

    // #618 / #1185 — every site below that skips an event records why.
    let mut event_drops = crate::trace::counters::EventDropCounts::default();

    loop {
        let done = if let Some(ref mut c) = child {
            c.try_wait().ok().flatten().is_some()
        } else {
            !std::path::Path::new(&format!("/proc/{target_pid}")).exists()
        };

        // If filter_by_pid is off, pass an empty set so the drain functions
        // admit every event. Building the empty set once per iteration is
        // cheap and keeps the drain signature uniform.
        let empty: std::collections::HashSet<u32> = std::collections::HashSet::new();
        let active_filter = if filter_by_pid { &target_pids } else { &empty };

        drain_network(&mut network_rb, &mut agg, &mut net_count, MAX_PER_ITER, active_filter);
        // Milestone 211 post-#611 follow-up: drain compiler exec events
        // BEFORE file events on each iteration. compiler exec/exit
        // events populate `compiler_agg.pid_to_invocation_id` which the
        // per-invocation read_set/write_set routing in
        // `compiler_agg.handle_file_event` reads. If file events drain
        // first, they arrive for pids not yet in the map → the
        // compiler_agg drops them silently → invocation buckets stay
        // empty even when both event streams are healthy.
        drain_compiler(
            compiler_rb.as_mut(),
            &mut compiler_agg,
            &mut compiler_count,
            MAX_PER_ITER,
        );
        drain_file(&mut file_rb, &mut agg, &mut compiler_agg, &mut cwd, &mut file_count, MAX_PER_ITER, active_filter, &mut event_drops);

        if done {
            // Settling drain: pull remaining events with a hard deadline so
            // we never loop forever if probes keep firing from unrelated PIDs.
            let deadline = Instant::now() + Duration::from_millis(250);
            let mut settled = false;
            while Instant::now() < deadline {
                let n = drain_network(&mut network_rb, &mut agg, &mut net_count, MAX_PER_ITER, active_filter)
                    + drain_compiler(
                        compiler_rb.as_mut(),
                        &mut compiler_agg,
                        &mut compiler_count,
                        MAX_PER_ITER,
                    )
                    + drain_file(&mut file_rb, &mut agg, &mut compiler_agg, &mut cwd, &mut file_count, MAX_PER_ITER, active_filter, &mut event_drops);
                if n == 0 {
                    settled = true;
                    break;
                }
            }
            // #618's original subject: the deadline expired while events
            // were still arriving, so whatever remained went away with the
            // programs. The size of the remainder is not knowable from
            // here — that it happened at all is, and was not before.
            if !settled {
                event_drops.record(waybill_common::events::EventDropReason::UserDrainAbandoned);
                tracing::warn!(
                    "drain deadline expired with events still arriving; the \
                     remainder was abandoned and the capture is a floor"
                );
            }
            break;
        }

        if timeout.is_some_and(|t| start.elapsed() > t) {
            tracing::warn!("Trace timeout");
            break;
        }

        tokio::time::sleep(Duration::from_millis(5)).await;
    }

    if let Some(mut c) = child {
        let st = c.wait()?;
        tracing::info!(?st, "Child exited");
    }

    let trace_end = Timestamp::now();
    tracing::info!(net = net_count, file = file_count, "Collection done");

    // Post-trace artifact-dir scan: walk user-supplied directories for
    // files that appeared during this trace (mtime ≥ trace_start_wall).
    // Each hit becomes a synthetic FileOperation (with real SHA-256)
    // whether or not the kernel-side kprobe captured it. This closes
    // the coverage gap observed with curl's -O and cargo's .crate writes.
    //
    // With --auto-dirs, the command argv is also inspected for known
    // build tools and their canonical cache paths are merged in. Explicit
    // --artifact-dir values always win (they come first) and duplicates
    // are dropped while preserving order.
    let merged_dirs: Vec<PathBuf> = {
        let mut v = args.artifact_dir.clone();
        if args.auto_dirs {
            for d in crate::cli::auto_dirs::detect(&args.command) {
                if !v.contains(&d) {
                    v.push(d);
                }
            }
        }
        v
    };
    if !merged_dirs.is_empty() {
        let added = scan_artifact_dirs(&merged_dirs, trace_start_wall, &mut agg);
        if added > 0 {
            tracing::info!(added, "post-trace artifact scan");
        }
    }

    // Post-trace hash pass: the SSL uprobes only ever see ~512 B of each
    // TLS record, so the "response hash" computed from what we observed
    // would not match the bytes actually on disk. Instead, stream-hash the
    // real files now that the traced command has finished writing them.
    // This also covers probe-captured paths (if any) for which the
    // artifact-dir scan wasn't configured.
    let file_hashes = hash_captured_artifacts(&agg);
    if !file_hashes.is_empty() {
        tracing::info!(count = file_hashes.len(), "hashed captured artifacts");
        agg.apply_file_hashes(&file_hashes);
    }

    // Milestone 212 (issue #615) — read per-CPU drop counters from
    // the three ring-buffer companion maps + populate
    // TraceIntegrity.ring_buffer_overflows with the aggregate. Pre-m212
    // this field was hardcoded to 0, silently hiding the drop-rate
    // bug that #614 investigation surfaced. The failing-map names (if
    // any) flow into TraceIntegrity.kprobe_attach_failures[] per Q3.
    // events_dropped stays 0 per Q2 (deferred to waybill#618).
    let drops = crate::trace::counters::read_ring_buffer_drops(&mut handle.bpf);

    // Milestone 213 (issue #616) — read the FILTER_CATEGORY_HITS map
    // + emit `filter_categories_applied[]` on TraceIntegrity per FR-006.
    // This is the transparent-aggregate mitigation for Principle VIII
    // that makes US1's kernel-side event-drop constitutionally sound:
    // operators see WHICH noise categories the trace suppressed even
    // though the specific paths are dropped kernel-side.
    let filter_hits = crate::trace::counters::read_filter_category_hits(&mut handle.bpf);
    let filter_categories_applied = filter_hits.applied_categories();
    tracing::info!(
        applied = ?filter_categories_applied,
        "m213 filter-category summary"
    );

    // Merge counter-map + filter-hits attach failures into a single
    // stats field. `aggregator.rs::finalize` sorts + dedups before
    // writing into TraceIntegrity.kprobe_attach_failures[]. Compute
    // `drops.total()` BEFORE moving `drops.attach_failures` out —
    // otherwise `drops` is partially moved and can no longer be borrowed.
    let overflows_total = drops.total();
    let mut counter_attach_failures = drops.attach_failures;
    counter_attach_failures.extend(filter_hits.attach_failures);

    // #1252 — open requests the kernel discarded because the syscall
    // failed. Counted unconditionally so a zero is readable: a compiler
    // attempts failed opens constantly across its search paths, so zero
    // on a real build says something about the capture, not the build.
    // An unreadable counter is recorded as an attach failure rather than
    // reported as zero.
    let (open_failures_discarded, open_counter_failure) =
        crate::trace::counters::read_open_failures_discarded(&mut handle.bpf);
    if let Some(name) = open_counter_failure {
        counter_attach_failures.push(name);
    }

    // #618 / #1185 — merge the kernel-side drop slots into the userspace
    // counts. An unreadable map is reported rather than read as zero.
    if let Some(name) =
        crate::trace::counters::merge_kernel_event_drops(
            &mut handle.bpf,
            &event_drop_baseline,
            &mut event_drops,
        )
    {
        counter_attach_failures.push(name);
    }

    // #1185 — the other half of `kernel_pending_insert_failed`: how full
    // the map that refuses a request had become. A count alone does not
    // say whether the bound was the constraint.
    let (pending_open_entries_at_close, pending_open_failure) =
        crate::trace::counters::read_pending_open_entries(&mut handle.bpf);
    if let Some(name) = pending_open_failure {
        counter_attach_failures.push(name);
    }

    // The compiler aggregator's own silent drops. Note what is NOT here:
    // an op whose process is not compiler-named belongs to no invocation
    // by construction, and it still reaches the document-level set, so it
    // is not a loss. Counting it warned that every healthy capture was "a
    // floor, not a total", which is the kind of false alarm that teaches
    // people to ignore the real one.
    event_drops.record_n(
        waybill_common::events::EventDropReason::UserHeldUnreplayed,
        compiler_agg.held_unreplayed() as u64,
    );
    event_drops.record_n(
        waybill_common::events::EventDropReason::UserHeldAtCapacity,
        compiler_agg.dropped_at_held_cap(),
    );
    // #1185 — the dry-run slot is reported separately from loss, because
    // during the dry run it is not loss. See `total_lost`.
    let outside_subtree = event_drops.get(
        waybill_common::events::EventDropReason::KernelOutsideTracedSubtree,
    );
    let (inside_subtree, subtree_counter_failure) =
        crate::trace::counters::read_traced_subtree_hits(&mut handle.bpf);
    let subtree_counter_unreadable = subtree_counter_failure.is_some();
    if let Some(name) = subtree_counter_failure {
        counter_attach_failures.push(name);
    }
    // One line, both halves, a stable prefix the harness greps for. Printed
    // unconditionally — a zero `inside` with a seeded root means propagation
    // is broken, and that is exactly the case a conditional print would
    // hide.
    tracing::info!(
        inside = inside_subtree,
        outside = outside_subtree,
        "#1185 traced subtree: opens admitted from the build and declined \
         from everything else. `outside` is other processes' opens the gate \
         dropped — not loss by this capture — and `inside` is whether \
         propagation past the root worked"
    );
    let event_drop_total = event_drops.total_lost();
    if event_drop_total > 0 {
        tracing::warn!(
            reasons = ?event_drops,
            "#618 observed events did not reach the recorded sets; the \
             capture is a floor, not a total"
        );
    }
    let event_drop_reasons = event_drops.into_wire();

    let mut trace = agg.finalize(&TraceStats {
        network_events: net_count,
        file_events: file_count,
        ring_buffer_overflows: overflows_total,
        // Not measured. See the field's doc on `TraceStats`: unlike
        // `overflows_total` above, nothing counts this, and #618 is where
        // measuring it lives.
        events_dropped: 0,
        counter_attach_failures,
        filter_categories_applied,
        probe_attach_failures,
        open_failures_discarded,
        event_drop_reasons,
        pending_open_entries_at_close,
        // #1185 — `None` only when the counter could not be read, which
        // `counter_attach_failures` records. A zero is a measurement.
        traced_subtree_opens: if subtree_counter_unreadable {
            None
        } else {
            Some(inside_subtree)
        },
    });

    if trace.network_trace.connections.is_empty()
        && trace.file_access.operations.is_empty()
        && compiler_count == 0
    {
        // Milestone 210 — the "no dependency activity" bail-out is
        // now a three-way OR: bail only when the network trace, the
        // file-op trace, AND the compiler-pipeline observation ALL
        // captured zero events. A hermetic offline build (cargo build
        // against a vendored fixture) legitimately has zero network
        // activity + zero file-op activity if the `vfs_open` kprobe
        // failed to attach; when the compiler pipeline still recorded
        // rustc/cc invocations we have real source→binary attribution
        // and should proceed to emit the attestation.
        tracing::error!(
            net = net_count,
            file = file_count,
            compiler = compiler_count,
            "Zero aggregated"
        );
        return Err(MikebomError::NoDependencyActivity.into());
    }

    let cmd_str = if args.command.is_empty() {
        format!("pid:{target_pid}")
    } else {
        args.command.join(" ")
    };

    // Feature 006 US3 — build the subject resolver from operator
    // override + artifact-dir walk. Legacy `subject_name` /
    // `subject_digest` remain for backward-compat callers but are
    // overridden once `subject_resolver` is set.
    let subject_resolver = Some(crate::attestation::subject::SubjectResolver {
        operator_subjects: args.subject.clone(),
        artifact_dirs: args.artifact_dir.clone(),
        mtime_floor: Some(trace_start_wall),
        command: cmd_str.clone(),
        trace_start_rfc3339: trace_start.to_iso8601(),
    });

    // Feature 006 — dispatch on attestation format. Witness-v0.1 is
    // the default; waybill-v1 preserves the richer native predicate
    // for operators who want it.
    if args.attestation_format == "witness-v0.1" {
        use crate::attestation::witness_builder::{build_witness_statement, WitnessBuildConfig};
        let identity = args.build_signing_identity()?;
        let witness_cfg = WitnessBuildConfig {
            target_pid: child_pid,
            target_command: cmd_str.clone(),
            cgroup_id: 0,
            subject_resolver: subject_resolver.clone(),
            collection_name: default_collection_name(&cmd_str),
        };
        let witness_stmt =
            build_witness_statement(trace, &witness_cfg, trace_start, trace_end)?;
        serializer::write_witness_attestation_signed(&witness_stmt, &args.output, &identity)?;

        let nc = witness_stmt
            .predicate
            .attestations
            .iter()
            .find(|e| e.attestor_type.ends_with("/network-trace/v0.1"))
            .and_then(|e| {
                e.attestation
                    .get("network_trace")
                    .and_then(|nt| nt.get("connections"))
                    .and_then(|c| c.as_array())
                    .map(|a| a.len())
            })
            .unwrap_or(0);
        let fo = witness_stmt
            .predicate
            .attestations
            .iter()
            .find(|e| e.attestor_type.ends_with("/material/v0.1"))
            .and_then(|e| e.attestation.as_object().map(|o| o.len()))
            .unwrap_or(0);

        if args.json {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "attestation_file": args.output.to_string_lossy(),
                    "attestation_format": "witness-v0.1",
                    "raw_net": net_count, "raw_file": file_count,
                    "emitted_net": nc, "emitted_file": fo,
                }))?
            );
        }

        tracing::info!(
            file = %args.output.display(),
            format = "witness-v0.1",
            "attestation written"
        );
        return Ok(());
    }

    // Milestone 210 — finalize the compiler-pipeline aggregator.
    // Returns `None` for cases where the trace captured zero compiler
    // invocations (older kernels without sched_process_exec,
    // tracepoint attach failed, operator's command didn't invoke a
    // whitelisted compiler). None-elision preserves pre-m210
    // attestation byte-identity per research R6.
    let mut compiler_pipeline_data = if compiler_count > 0 {
        Some(compiler_agg.finalize())
    } else {
        None
    };
    if let Some(ref data) = compiler_pipeline_data {
        tracing::info!(
            invocations = data.invocations.len(),
            secrets_filtered = data.secrets_read_filtered,
            "compiler-pipeline data captured"
        );
    }

    // Milestone 1072 (#1144) — apply close-time facts to the compiler
    // read/write sets. Until this ran, every write-set entry claimed it
    // survived the trace window and every hash was 64 zeros.
    //
    // Only runs when the trace captured compiler invocations: a trace
    // with none emits no coverage record at all, so its `trace_integrity`
    // stays byte-identical to a pre-m1072 attestation (SC-007).
    if let Some(ref mut data) = compiler_pipeline_data {
        // C-1072-8's mapping lives in `close_time::budget_from_flag` so
        // that it is testable: everything in this function is behind the
        // Linux + ebpf-tracing gate, where no unprivileged test reaches.
        let budget = close_time::budget_from_flag(
            args.compiler_hash_budget_bytes,
            DEFAULT_COMPILER_HASH_BUDGET_BYTES,
        );
        let coverage = close_time::apply_close_time_facts(data, budget);
        trace.trace_integrity.compiler_hash_coverage = Some(coverage);
    }
    // Left `None` when the trace captured no compiler invocations, so
    // `trace_integrity` serializes byte-identically to a pre-m1072
    // attestation (FR-014, SC-007). That is the `if let` above doing
    // nothing, not a separate branch.

    let stmt = builder::build_attestation(
        trace,
        &AttestationConfig {
            target_pid: child_pid,
            target_command: cmd_str,
            cgroup_id: 0,
            subject_name: "build-output".to_string(),
            subject_digest: None,
            subject_resolver,
        },
        trace_start,
        trace_end,
        compiler_pipeline_data,
    )?;

    // Feature 006 — write signed DSSE envelope when a signing identity
    // is configured; fall through to legacy raw shape otherwise.
    let identity = args.build_signing_identity()?;
    serializer::write_attestation_signed(&stmt, &args.output, &identity)?;

    let nc = stmt.predicate.network_trace.summary.total_connections;
    let fo = stmt.predicate.file_access.summary.total_operations;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&serde_json::json!({
            "attestation_file": args.output.to_string_lossy(),
            "raw_net": net_count, "raw_file": file_count,
            "connections": nc, "file_operations": fo,
        }))?);
    }

    tracing::info!(output = %args.output.display(), connections = nc, file_ops = fo, "Done");
    Ok(())
}

/// Walk each `artifact_dir` recursively, find files whose mtime is at or
/// after `since`, stream-hash each one, and push a synthetic
/// `FileOperation` into the aggregator. Returns the count added.
///
/// The underlying directory walk + hash logic lives in
/// [`crate::scan_fs::walker::walk_and_hash`] and is shared with the
/// standalone `sbom scan` subcommand.
#[cfg(all(target_os = "linux", feature = "ebpf-tracing"))]
fn scan_artifact_dirs(
    dirs: &[PathBuf],
    since: std::time::SystemTime,
    agg: &mut crate::trace::aggregator::EventAggregator,
) -> usize {
    use crate::scan_fs::walker::{walk_and_hash, DEFAULT_SIZE_CAP_BYTES};

    let mut added = 0;
    for dir in dirs {
        if !dir.is_dir() {
            tracing::warn!(dir = %dir.display(), "--artifact-dir is not a directory, skipping");
            continue;
        }
        let artifacts = walk_and_hash(dir, Some(since), DEFAULT_SIZE_CAP_BYTES);
        for a in artifacts {
            let ts = chrono::DateTime::<chrono::Utc>::from(a.mtime);
            agg.record_synthetic_file_op(
                a.path.to_string_lossy().into_owned(),
                a.size,
                Some(a.hash),
                ts,
            );
            added += 1;
        }
    }
    added
}

/// Stream-hash every captured path that (a) ends in a package-artifact
/// suffix, (b) still exists on disk, and (c) is under the size cap. Each
/// hash is keyed by the exact path string the aggregator saw so
/// `apply_file_hashes` can match it back.
#[cfg(all(target_os = "linux", feature = "ebpf-tracing"))]
fn hash_captured_artifacts(
    agg: &crate::trace::aggregator::EventAggregator,
) -> std::collections::HashMap<String, waybill_common::types::hash::ContentHash> {
    use crate::scan_fs::walker::{ARTIFACT_SUFFIXES, DEFAULT_SIZE_CAP_BYTES};
    use crate::trace::hasher::sha256_file_hex;
    use waybill_common::types::hash::ContentHash;

    let mut out = std::collections::HashMap::new();
    for path in agg.captured_paths() {
        let lc = path.to_ascii_lowercase();
        if !ARTIFACT_SUFFIXES.iter().any(|s| lc.ends_with(s)) {
            continue;
        }
        let p = std::path::Path::new(path);
        if !p.is_file() {
            continue;
        }
        match sha256_file_hex(p, DEFAULT_SIZE_CAP_BYTES) {
            Ok(hex) => match ContentHash::sha256(&hex) {
                Ok(h) => {
                    out.insert(path.to_string(), h);
                }
                Err(e) => tracing::warn!(path, error = %e, "invalid sha256 hex"),
            },
            Err(e) => tracing::debug!(path, error = %e, "could not hash artifact"),
        }
    }
    out
}

/// Compute the offset (in nanoseconds) that converts a CLOCK_BOOTTIME
/// nanosecond timestamp (what `bpf_ktime_get_ns` returns) into a
/// CLOCK_REALTIME Unix-epoch nanosecond timestamp. Returns 0 on error so
/// callers still get a best-effort wall-clock rather than panicking.
#[cfg(all(target_os = "linux", feature = "ebpf-tracing"))]
fn compute_boot_offset_ns() -> u64 {
    fn sample(clock: libc::clockid_t) -> Option<u64> {
        let mut ts = libc::timespec { tv_sec: 0, tv_nsec: 0 };
        // SAFETY: clock_gettime is a syscall wrapper; `ts` is a writable,
        // properly-aligned timespec on the stack.
        let rc = unsafe { libc::clock_gettime(clock, &mut ts) };
        if rc != 0 {
            return None;
        }
        Some((ts.tv_sec as u64).saturating_mul(1_000_000_000)
            + ts.tv_nsec as u64)
    }

    match (sample(libc::CLOCK_REALTIME), sample(libc::CLOCK_BOOTTIME)) {
        (Some(real), Some(boot)) => real.saturating_sub(boot),
        _ => 0,
    }
}

#[cfg(not(all(target_os = "linux", feature = "ebpf-tracing")))]
async fn execute_scan(_args: ScanArgs) -> anyhow::Result<()> {
    anyhow::bail!(
        "this build was compiled without eBPF support; rebuild with \
         --features ebpf-tracing on a Linux host to enable trace capture.\n\
         (macOS users: spin up a Lima VM and rebuild there. Non-tracing \
         commands — sbom scan/verify, attestation generate/verify, policy — \
         work on any platform without the feature.)"
    )
}
