use std::path::PathBuf;

use clap::Args;

#[derive(Args)]
pub struct ScanArgs {
    #[arg(long)]
    pub target_pid: Option<u32>,
    #[arg(long, default_value = "mikebom.attestation.json")]
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
    /// auto-detection is suppressed — mikebom signs exactly what you
    /// told it to (FR-009).
    #[arg(long = "subject", value_name = "PATH")]
    pub subject: Vec<PathBuf>,

    /// Attestation output format. `witness-v0.1` emits an in-toto
    /// Statement v0.1 wrapped around a witness attestation-collection
    /// (`material` + `command-run` + `product` + `network-trace`
    /// inner attestors), directly consumable by `sbomit generate` and
    /// any go-witness-aware verifier. `mikebom-v1` emits mikebom's
    /// native `BuildTracePredicate` Statement v1 — richer network-
    /// trace semantics but only mikebom understands it.
    #[arg(long = "attestation-format", value_name = "FORMAT", default_value = "witness-v0.1")]
    pub attestation_format: String,

    /// Milestone 210 (FR-016) — bypass the compiler-pipeline aggregator's
    /// default denylist for `/etc/`, `/proc/`, `/sys/`, `/dev/`, user
    /// cache directories, `/tmp/`, and secret-adjacent paths. Off by
    /// default so per-component `mikebom:source-read-set` annotations
    /// stay signal-heavy (system reads dominate a build's read syscalls
    /// but almost never disambiguate which SOURCE input produced a
    /// binary). Turn on for kernel/toolchain SBOM audits where reads
    /// under `/usr/include`, `/usr/lib`, or `/dev/urandom` are relevant.
    #[arg(long)]
    pub include_system_reads: bool,

    #[arg(last = true)]
    pub command: Vec<String>,
}

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
        "witness-v0.1" | "mikebom-v1" => {}
        other => anyhow::bail!(
            "unknown --attestation-format {other:?}; accepted: witness-v0.1, mikebom-v1"
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

    use aya::maps::RingBuf;

    use crate::attestation::builder::{self, AttestationConfig};
    use crate::attestation::serializer;
    use crate::error::MikebomError;
    use crate::trace::aggregator::EventAggregator;
    use crate::trace::compiler_pipeline::{
        CompilerPipelineAggregator, FilterConfig as CompilerFilterConfig,
    };
    use crate::trace::loader::{self, LoaderConfig};
    use crate::trace::processor::TraceStats;
    use mikebom_common::events::{CompilerExecEvent, FileEvent, NetworkEvent};
    use mikebom_common::types::timestamp::Timestamp;

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
    let mut handle = loader::load_and_attach(&LoaderConfig {
        target_pid,
        libssl_path: args.libssl_path.clone(),
        ring_buffer_size: args.ring_buffer_size,
        ebpf_object: None,
        trace_children: args.trace_children,
        // Milestone 213 (issue #616) — plumbs --include-system-reads
        // to the kernel-side FILTER_WIDEN[0] slot per FR-010.
        include_system_reads: args.include_system_reads,
    })?;
    tracing::info!("eBPF probes attached");

    // THEN spawn child
    let mut child = if args.target_pid.is_none() {
        let cmd = &args.command;
        tracing::info!(command = %cmd.join(" "), "Spawning traced command");
        let c = std::process::Command::new(&cmd[0])
            .args(&cmd[1..])
            .spawn()
            .map_err(|e| anyhow::anyhow!("failed to spawn: {e}"))?;
        tracing::info!(pid = c.id(), "Child started");
        Some(c)
    } else {
        None
    };

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
    // The "run a command and trace it" path (`mikebom trace run -- ...`)
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
        bpf: &mut aya::Ebpf,
        agg: &mut EventAggregator,
        count: &mut u64,
        max: usize,
        target_pids: &std::collections::HashSet<u32>,
    ) -> usize {
        let map = bpf
            .map_mut("NETWORK_EVENTS")
            .expect("NETWORK_EVENTS ring buffer is statically declared in the eBPF object");
        let mut rb = RingBuf::try_from(map)
            .expect("NETWORK_EVENTS map shape is BPF_MAP_TYPE_RINGBUF by construction");
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
        bpf: &mut aya::Ebpf,
        compiler_agg: &mut CompilerPipelineAggregator,
        count: &mut u64,
        max: usize,
    ) -> usize {
        let Some(map) = bpf.map_mut("COMPILER_EXEC_EVENTS") else {
            return 0;
        };
        let Ok(mut rb) = RingBuf::try_from(map) else {
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

    fn drain_file(
        bpf: &mut aya::Ebpf,
        agg: &mut EventAggregator,
        compiler_agg: &mut CompilerPipelineAggregator,
        count: &mut u64,
        max: usize,
        target_pids: &std::collections::HashSet<u32>,
    ) -> usize {
        let map = bpf
            .map_mut("FILE_EVENTS")
            .expect("FILE_EVENTS ring buffer is statically declared in the eBPF object");
        let mut rb = RingBuf::try_from(map)
            .expect("FILE_EVENTS map shape is BPF_MAP_TYPE_RINGBUF by construction");
        let mut n = 0;
        while n < max {
            match rb.next() {
                Some(item) => {
                    let data: &[u8] = item.as_ref();
                    if data.len() >= core::mem::size_of::<FileEvent>() {
                        let ev = unsafe {
                            core::ptr::read_unaligned(data.as_ptr() as *const FileEvent)
                        };
                        if target_pids.is_empty() || target_pids.contains(&ev.pid) {
                            agg.handle_file_event(&ev);
                            // Milestone 211 (post-#611 follow-up): also route
                            // to the m210 compiler-pipeline aggregator so
                            // per-invocation read_set/write_set populates.
                            // The compiler_agg keys on pid_to_invocation_id,
                            // so events from non-compiler pids no-op harmlessly.
                            // Without this dual-dispatch the compiler-pipeline
                            // invocation buckets stayed empty even with file
                            // events flowing through the doc-level file_access
                            // — which in turn kept m210's C130 always emitting
                            // empty payloads.
                            compiler_agg.handle_file_event(&ev);
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

        drain_network(&mut handle.bpf, &mut agg, &mut net_count, MAX_PER_ITER, active_filter);
        // Milestone 211 post-#611 follow-up: drain compiler exec events
        // BEFORE file events on each iteration. compiler exec/exit
        // events populate `compiler_agg.pid_to_invocation_id` which the
        // per-invocation read_set/write_set routing in
        // `compiler_agg.handle_file_event` reads. If file events drain
        // first, they arrive for pids not yet in the map → the
        // compiler_agg drops them silently → invocation buckets stay
        // empty even when both event streams are healthy.
        drain_compiler(
            &mut handle.bpf,
            &mut compiler_agg,
            &mut compiler_count,
            MAX_PER_ITER,
        );
        drain_file(&mut handle.bpf, &mut agg, &mut compiler_agg, &mut file_count, MAX_PER_ITER, active_filter);

        if done {
            // Settling drain: pull remaining events with a hard deadline so
            // we never loop forever if probes keep firing from unrelated PIDs.
            let deadline = Instant::now() + Duration::from_millis(250);
            while Instant::now() < deadline {
                let n = drain_network(&mut handle.bpf, &mut agg, &mut net_count, MAX_PER_ITER, active_filter)
                    + drain_compiler(
                        &mut handle.bpf,
                        &mut compiler_agg,
                        &mut compiler_count,
                        MAX_PER_ITER,
                    )
                    + drain_file(&mut handle.bpf, &mut agg, &mut compiler_agg, &mut file_count, MAX_PER_ITER, active_filter);
                if n == 0 {
                    break;
                }
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

    let trace = agg.finalize(&TraceStats {
        network_events: net_count,
        file_events: file_count,
        ring_buffer_overflows: overflows_total,
        events_dropped: 0,
        counter_attach_failures,
        filter_categories_applied,
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
    // the default; mikebom-v1 preserves the richer native predicate
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
    let compiler_pipeline_data = if compiler_count > 0 {
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
) -> std::collections::HashMap<String, mikebom_common::types::hash::ContentHash> {
    use crate::scan_fs::walker::{ARTIFACT_SUFFIXES, DEFAULT_SIZE_CAP_BYTES};
    use crate::trace::hasher::sha256_file_hex;
    use mikebom_common::types::hash::ContentHash;

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
