// Loader types are only constructed inside the Linux-only
// `cli/scan.rs::execute_scan` flow; on macOS the file compiles but is
// unreachable.
#![allow(dead_code)]

use std::path::PathBuf;

/// Probes that did not attach on this kernel (#1252).
///
/// Every best-effort attach in `load_and_attach` records its program name
/// here instead of only logging a WARN. The names flow into
/// `TraceIntegrity.kprobe_attach_failures[]`, which existed and was
/// documented for exactly this since milestone 212 but was only ever fed
/// from counter-map failures — so an attestation produced with two probes
/// missing still serialized `kprobe_attach_failures: []`.
#[derive(Clone, Debug, Default)]
pub struct ProbeAttachFailures {
    /// Program names of kprobes and tracepoints that failed to attach.
    pub kprobe: Vec<String>,
}

impl ProbeAttachFailures {
    pub fn is_empty(&self) -> bool {
        self.kprobe.is_empty()
    }
}

pub struct LoaderConfig {
    pub target_pid: u32,
    pub libssl_path: Option<PathBuf>,
    pub ring_buffer_size: u32,
    pub trace_children: bool,
    pub ebpf_object: Option<PathBuf>,
    /// Milestone 213 (issue #616) — plumbs `ScanArgs.include_system_reads`
    /// through to the kernel-side `FILTER_WIDEN[0]` slot per FR-010.
    /// When `true`, the kernel-side classifier's System-category compare
    /// short-circuits to `None` — System paths flow through to the ring
    /// buffer instead of being dropped. UserCache/Ephemeral/CargoFingerprint
    /// remain active regardless of this flag.
    pub include_system_reads: bool,
    /// #1185 — per-trace handshake nonce, or `None` for a mode that cannot
    /// seed a traced root.
    ///
    /// `--target-pid` attaches to a process the tracer did not spawn, so
    /// there is no `pre_exec` in which to announce a root and the capture
    /// stays unscoped. `None` says so explicitly rather than arming a
    /// handshake nothing will answer.
    pub handshake_nonce: Option<u32>,
}

#[cfg(all(target_os = "linux", feature = "ebpf-tracing"))]
mod inner {
    use std::path::{Path, PathBuf};

    use anyhow::{Context, Result};
    use aya::programs::{TracePoint, UProbe};
    use aya::Ebpf;
    use tracing::{debug, info, warn};

    use waybill_common::maps::TraceConfig;

    use super::{LoaderConfig, ProbeAttachFailures};

    pub struct EbpfHandle {
        pub bpf: Ebpf,
        /// #1252 — which probes did not attach. The caller threads these
        /// into the attestation so a degraded capture says so on the wire.
        pub attach_failures: ProbeAttachFailures,
    }

    pub fn load_and_attach(config: &LoaderConfig) -> Result<EbpfHandle> {
        info!(target_pid = config.target_pid, "Loading eBPF program");
        let mut attach_failures = ProbeAttachFailures::default();

        let obj_path = config
            .ebpf_object
            .clone()
            .unwrap_or_else(default_ebpf_path);

        let data = std::fs::read(&obj_path).with_context(|| {
            format!(
                "failed to read eBPF object at {}. Run `cargo xtask ebpf` first.",
                obj_path.display()
            )
        })?;

        let mut bpf = Ebpf::load(&data).context("failed to load eBPF bytecode")?;

        // Populate PID filter (even though kernel-side is disabled,
        // keep the map populated for future use). Insertion failure
        // is non-fatal here — the filter is defensive future-proofing —
        // but log so operators can investigate if a future kernel-side
        // change starts depending on it.
        {
            let mut pid_filter: aya::maps::HashMap<_, u32, u8> =
                aya::maps::HashMap::try_from(
                    bpf.map_mut("PID_FILTER").context("PID_FILTER map not found")?,
                )?;
            if let Err(e) = pid_filter.insert(config.target_pid, 1, 0) {
                warn!(error = %e, target_pid = config.target_pid,
                    "PID_FILTER insert failed; eBPF map populate is best-effort");
            }
        }

        // Set runtime configuration. Failure here IS load-bearing —
        // kernel-side eBPF reads tracer_pid / capture_file_content_hash /
        // trace_children from this map; a silent failure means the
        // kernel sees zeroed defaults and traces wrong / missing data.
        // We currently propagate-as-best-effort to avoid breaking
        // existing callers, but log loudly so the failure is visible.
        {
            let mut cfg_map: aya::maps::Array<_, TraceConfig> =
                aya::maps::Array::try_from(
                    bpf.map_mut("CONFIG").context("CONFIG map not found")?,
                )?;
            if let Err(e) = cfg_map.set(
                0,
                TraceConfig {
                    max_payload_capture: 512,
                    tracer_pid: std::process::id(),
                    capture_file_content_hash: 1,
                    trace_children: if config.trace_children { 1 } else { 0 },
                    _padding: [0; 2],
                },
                0,
            ) {
                warn!(error = %e,
                    "CONFIG map set failed; kernel-side will see zeroed TraceConfig defaults");
            }
        }

        // Milestone 213 (issue #616) — write the widen-flag config
        // (FILTER_WIDEN[0]) per FR-010. `1` disables the kernel-side
        // System-category filter; `0` (default) leaves it active.
        // On map-attach failure, log WARN + add to the caller's
        // eventual kprobe_attach_failures[] surface via R9. The trace
        // continues (fail-open) because the FILTER_WIDEN semantics
        // are "the kernel-side classifier interprets a missing/absent
        // FILTER_WIDEN as widen=false = filter active" — matches
        // pre-m213 System-category filtering behavior.
        {
            match aya::maps::PerCpuArray::try_from(
                bpf.map_mut("FILTER_WIDEN")
                    .context("FILTER_WIDEN map not found")?,
            ) {
                Ok(mut widen_map) => {
                    let widen_val: u8 = if config.include_system_reads { 1 } else { 0 };
                    // `aya::util::nr_cpus()` returns `Result<usize, (&str, io::Error)>` — a
                    // tuple error type, not something that implements `StdError`, so anyhow's
                    // `.context()` extension trait doesn't apply. Convert to anyhow explicitly.
                    let nr_cpus = aya::util::nr_cpus().map_err(|(msg, err)| {
                        anyhow::anyhow!(
                            "failed to detect online CPU count for FILTER_WIDEN write ({msg}): {err}"
                        )
                    })?;
                    let per_cpu_values: aya::maps::PerCpuValues<u8> =
                        aya::maps::PerCpuValues::try_from(vec![widen_val; nr_cpus])?;
                    if let Err(e) = widen_map.set(0, per_cpu_values, 0) {
                        warn!(error = %e,
                            include_system_reads = config.include_system_reads,
                            "FILTER_WIDEN set failed; kernel-side will see per-CPU-zero widen flag (System filter active)");
                    } else {
                        info!(
                            include_system_reads = config.include_system_reads,
                            "m213 widen flag written to FILTER_WIDEN[0]"
                        );
                    }
                }
                Err(e) => {
                    warn!(error = %e,
                        "FILTER_WIDEN map not usable on this kernel; System filter will always be active regardless of --include-system-reads");
                }
            }
        }

        // #1252 — the discarded-open diagnostic slot (OPEN_DIAG[0]).
        //
        // `1` makes the kernel report each open it discards as an
        // `OpenFailed` record naming the path and the errno; `0` (the
        // default) counts the discard and drops it. It exists so a read-set
        // entry that disappeared can be attributed to a failed request by
        // name, rather than accepted because a count happened to match.
        //
        // Env-var rather than a CLI flag: it changes nothing a consumer
        // sees, it is read when verifying a change to capture rather than
        // when producing an attestation, and FR-004c requires it not be
        // needed for a trace to run. An unusable map is a warning, not a
        // failure, for the same reason FILTER_WIDEN's is: the kernel reads
        // an absent slot as `0`, which is the default behaviour anyway.
        {
            let diag = std::env::var("WAYBILL_TRACE_OPEN_DIAG")
                .map(|v| v == "1")
                .unwrap_or(false);
            if diag {
                match bpf.map_mut("OPEN_DIAG").map(aya::maps::PerCpuArray::try_from) {
                    Some(Ok(mut diag_map)) => {
                        let nr_cpus = aya::util::nr_cpus().map_err(|(msg, err)| {
                            anyhow::anyhow!(
                                "failed to detect online CPU count for OPEN_DIAG write ({msg}): {err}"
                            )
                        })?;
                        let values: aya::maps::PerCpuValues<u8> =
                            aya::maps::PerCpuValues::try_from(vec![1u8; nr_cpus])?;
                        if let Err(e) = diag_map.set(0, values, 0) {
                            warn!(error = %e,
                                "OPEN_DIAG set failed; discarded opens will be counted but not named");
                        } else {
                            info!(
                                "#1252 discarded-open diagnostics ON: each failed open is \
                                 reported with its path and errno"
                            );
                        }
                    }
                    Some(Err(e)) => warn!(error = %e,
                        "OPEN_DIAG map not usable on this kernel; discarded opens will be counted but not named"),
                    None => warn!("OPEN_DIAG map not found; discarded opens will be counted but not named"),
                }
            }
        }

        // Attach uprobes to OpenSSL
        let libssl = config
            .libssl_path
            .clone()
            .or_else(find_libssl)
            .context("could not find libssl.so — pass --libssl-path")?;

        info!(path = %libssl.display(), "Attaching uprobes to libssl");
        attach_uprobe(&mut bpf, "ssl_read_entry", &libssl, "SSL_read")?;
        attach_uprobe(&mut bpf, "ssl_read_return", &libssl, "SSL_read")?;
        attach_uprobe(&mut bpf, "ssl_write_entry", &libssl, "SSL_write")?;

        // #1252 — file opens and network connects, observed at the
        // syscall boundary. Nothing here names an internal kernel
        // function: a syscall tracepoint's name and argument layout are
        // UAPI, and the kprobes these replaced were not. See
        // waybill-ebpf/src/programs/open_syscalls.rs for which interfaces
        // this project treats as stable and why.
        //
        // vfs_read/vfs_write remain declared and NOT attached. They fire
        // on every read/write syscall system-wide and emit empty-path
        // events, so each one costs ring-buffer space for no aggregator
        // benefit -- enough of them to push out the late opens. A kprobe
        // target is resolved at attach time, so leaving them unattached
        // costs nothing and keeps them out of #1252's scope.

        // Each open syscall is attached as an exit-then-entry pair, in
        // that order and never half of one. The entry tracepoint alone
        // produces a held request that nothing will ever collect; the exit
        // alone produces results with no request to match. Attaching the
        // exit first means a failure there stops the entry from being
        // attached at all, so the half-attached state the contract warns
        // about cannot arise by accident.
        //
        // `openat` is the exception that FR-004b exists for, handled
        // below: it is the interface every libc open wrapper reaches, so
        // losing its entry is fatal while losing only its exit degrades.

        // `sys_enter_openat` is fatal. Principle III — a capture with no
        // file visibility at all must not present itself as a trace.
        attach_tracepoint(&mut bpf, "sys_enter_openat", "syscalls", "sys_enter_openat")
            .with_context(|| {
                "could not attach the sys_enter_openat tracepoint — the trace \
                 would observe zero file opens and report them as an absence \
                 of file activity"
                    .to_string()
            })?;

        // FR-004b — the host can see the request but not its result.
        // Rather than abort, capture degrades to recording requests, which
        // is what it did before #1252, and says so: the name reaches
        // `kprobe_attach_failures[]` and the completeness statement
        // downgrades to `partial / probe_attach_failed`. Recording
        // requests *silently* is the failure this feature removes, so the
        // fallback is only acceptable with the gap attached to it.
        //
        // SC-005 asks for this verified by making an interface unavailable
        // deliberately rather than by waiting to observe it, so the
        // fallback is reachable on demand. Without that the branch below
        // would ship unexecuted — a fallback that has never run is a
        // fallback nobody knows works.
        let force_request_only = std::env::var("WAYBILL_TRACE_FORCE_REQUEST_ONLY")
            .map(|v| v == "1")
            .unwrap_or(false);
        if force_request_only {
            warn!(
                "WAYBILL_TRACE_FORCE_REQUEST_ONLY=1: pretending this host \
                 cannot report open results, to exercise the FR-004b fallback"
            );
            attach_failures.kprobe.push("sys_exit_openat".to_string());
            set_request_only(&mut bpf)?;
        } else if let Err(e) =
            attach_tracepoint(&mut bpf, "sys_exit_openat", "syscalls", "sys_exit_openat")
        {
            warn!(
                "could not attach the sys_exit_openat tracepoint: {e}; capture \
                 falls back to recording open requests, so a request that \
                 failed will appear in a read set and the capture is a floor \
                 rather than a total"
            );
            attach_failures.kprobe.push("sys_exit_openat".to_string());
            set_request_only(&mut bpf)?;
        }

        // #1185 — arm the handshake before the child can possibly run, so
        // the root's announcement cannot arrive at an unarmed kernel.
        if let Some(nonce) = config.handshake_nonce {
            set_handshake_nonce(&mut bpf, nonce)?;
        } else {
            warn!(
                "no handshake nonce: this mode cannot establish a traced \
                 subtree, so the capture admits every process on the machine \
                 (#1185)"
            );
        }

        // #1185 — the forced counterpart to the fallback above. Not an
        // attach failure and not recorded as one: the probes attached
        // fine, and what is being withheld is the map's willingness to
        // hold a request. Set here so it covers every open variant, since
        // they all stash through the same map.
        if std::env::var("WAYBILL_TRACE_FORCE_PENDING_INSERT_FAIL")
            .map(|v| v == "1")
            .unwrap_or(false)
        {
            set_pending_insert_force_fail(&mut bpf)?;
        }

        // openat2 is best-effort: it is the newer interface and a kernel
        // that lacks it still serves every open through `openat`. Exit
        // first, so its entry is never attached without it.
        if attach_tracepoint(&mut bpf, "sys_exit_openat2", "syscalls", "sys_exit_openat2")
            .map_err(|e| {
                warn!("could not attach the sys_exit_openat2 tracepoint: {e}");
                attach_failures.kprobe.push("sys_exit_openat2".to_string());
            })
            .is_ok()
        {
            if let Err(e) =
                attach_tracepoint(&mut bpf, "sys_enter_openat2", "syscalls", "sys_enter_openat2")
            {
                warn!("could not attach the sys_enter_openat2 tracepoint: {e}");
                attach_failures.kprobe.push("sys_enter_openat2".to_string());
            }
        }

        // `open` and `creat` exist on x86-64 and not on arm64, which has
        // only the directory-relative forms. Measured, not assumed:
        // specs/1270-stable-probe-tracepoints/measurements/findings.md.
        // Guarded at compile time rather than attached best-effort,
        // because an architecture's shape is not a fault and must not be
        // recorded as one (FR-006) — the m1070 `rename` precedent below.
        #[cfg(target_arch = "x86_64")]
        for (enter, exit) in [
            ("sys_enter_open", "sys_exit_open"),
            ("sys_enter_creat", "sys_exit_creat"),
        ] {
            // Exit first, same rule as openat2 above.
            if let Err(e) = attach_tracepoint(&mut bpf, exit, "syscalls", exit) {
                warn!("could not attach the {exit} tracepoint: {e}");
                attach_failures.kprobe.push(exit.to_string());
                continue;
            }
            if let Err(e) = attach_tracepoint(&mut bpf, enter, "syscalls", enter) {
                warn!("could not attach the {enter} tracepoint: {e}");
                attach_failures.kprobe.push(enter.to_string());
            }
        }

        // Network connects. Non-fatal: a build that fetches nothing over
        // the network is a normal build, so losing this degrades the
        // record rather than invalidating it (FR-008). Only the entry is
        // attached — see the module note in programs/tcp_connect.rs on why
        // connects keep attempt semantics while opens do not.
        if let Err(e) = attach_tracepoint(&mut bpf, "sys_enter_connect", "syscalls", "sys_enter_connect")
        {
            warn!("could not attach the sys_enter_connect tracepoint: {e}");
            attach_failures
                .kprobe
                .push("sys_enter_connect".to_string());
        }

        // Milestone 210 — compiler-pipeline tracepoints. Best-effort:
        // if any of the three fail to attach (missing symbol, kernel
        // config nostops), warn + continue. The compiler-pipeline
        // observation gracefully degrades to "no data" — the rest of
        // the trace pipeline (network, file-ops for non-compiler
        // pids) keeps working.
        if let Err(e) =
            attach_tracepoint(&mut bpf, "sched_process_exec", "sched", "sched_process_exec")
        {
            warn!("could not attach sched_process_exec tracepoint: {e}");
            attach_failures.kprobe.push("sched_process_exec".to_string());
        }
        if let Err(e) =
            attach_tracepoint(&mut bpf, "sched_process_fork", "sched", "sched_process_fork")
        {
            warn!("could not attach sched_process_fork tracepoint: {e}");
            attach_failures.kprobe.push("sched_process_fork".to_string());
        }
        if let Err(e) =
            attach_tracepoint(&mut bpf, "sched_process_exit", "sched", "sched_process_exit")
        {
            warn!("could not attach sched_process_exit tracepoint: {e}");
            attach_failures.kprobe.push("sched_process_exit".to_string());
        }
        // These three stay best-effort, as milestone 210 left them, but
        // since #1252 they are no longer silent: each records its name in
        // `attach_failures`, which reaches
        // `TraceIntegrity.kprobe_attach_failures[]` and downgrades the
        // compiler-pipeline completeness to `partial/probe_attach_failed`.
        // Fork records also drive working-directory inheritance (milestone
        // 1070), where a missing fork tracepoint additionally shows up as
        // unresolved relative opens.

        // Milestone 1070 (#614) — directory changes and renames. Unlike the
        // tracepoints above, a failure here is fatal (Principle III, analysis
        // C1): without them every relative source read, or every library the
        // compiler renames into place, would silently go missing from the
        // attestation, which is the failure this milestone exists to remove.
        for (prog, event) in [
            ("sys_enter_chdir", "sys_enter_chdir"),
            ("sys_exit_chdir", "sys_exit_chdir"),
            ("sys_exit_fchdir", "sys_exit_fchdir"),
            ("sys_enter_renameat", "sys_enter_renameat"),
            ("sys_exit_renameat", "sys_exit_renameat"),
            ("sys_enter_renameat2", "sys_enter_renameat2"),
            ("sys_exit_renameat2", "sys_exit_renameat2"),
        ] {
            attach_tracepoint(&mut bpf, prog, "syscalls", event)
                .with_context(|| format!("could not attach the {event} tracepoint"))?;
        }
        // `rename(2)` exists on x86-64 but not on arm64, which only has the
        // `renameat` family. Its absence there is not a failure.
        #[cfg(target_arch = "x86_64")]
        for event in ["sys_enter_rename", "sys_exit_rename"] {
            attach_tracepoint(&mut bpf, event, "syscalls", event)
                .with_context(|| format!("could not attach the {event} tracepoint"))?;
        }

        // #1252 — this used to read `All probes attached` unconditionally,
        // and printed it on a run where do_filp_open had just failed. Say
        // what happened instead of what was hoped for.
        if attach_failures.is_empty() {
            info!("All probes attached");
        } else {
            warn!(
                failed = ?attach_failures.kprobe,
                "some probes did not attach; capture is a floor, not a total"
            );
        }
        Ok(EbpfHandle {
            bpf,
            attach_failures,
        })
    }

    /// FR-004b — tell the kernel it must emit at entry, because this host
    /// cannot report the result of an open.
    ///
    /// Writing the slot is fatal if it fails. Everywhere else in this
    /// loader an unusable config map is a warning, because the kernel
    /// reads an absent slot as the default and the default is the intended
    /// behaviour. Here the default is the *wrong* behaviour: without the
    /// flag the entry tracepoint holds requests that no exit will collect,
    /// so the trace would record almost no opens while reporting only a
    /// probe-attach gap. Failing is the honest outcome.
    fn set_request_only(bpf: &mut Ebpf) -> Result<()> {
        let map = bpf
            .map_mut("OPEN_REQUEST_ONLY")
            .context("OPEN_REQUEST_ONLY map not found, so the FR-004b fallback cannot be enabled")?;
        let mut slot = aya::maps::PerCpuArray::<_, u8>::try_from(map)
            .context("OPEN_REQUEST_ONLY is not a usable per-CPU array")?;
        let nr_cpus = aya::util::nr_cpus().map_err(|(msg, err)| {
            anyhow::anyhow!("failed to detect online CPU count for OPEN_REQUEST_ONLY ({msg}): {err}")
        })?;
        let values: aya::maps::PerCpuValues<u8> =
            aya::maps::PerCpuValues::try_from(vec![1u8; nr_cpus])?;
        slot.set(0, values, 0)
            .context("could not write OPEN_REQUEST_ONLY; capture would hold requests nothing collects")?;
        warn!(
            "FR-004b fallback ON: this host reports no open results, so capture \
             records requests. A request that failed will appear in a read set, \
             and the attestation says the capture is a floor rather than a total."
        );
        Ok(())
    }

    /// #1185 — arm the handshake the traced root uses to announce itself.
    ///
    /// Fails loudly. Without the nonce in the map the kernel recognises no
    /// handshake, nothing is seeded, and the traced subtree stays empty —
    /// which, once the gate lands, is a capture that drops every open while
    /// reporting itself complete. That is the failure this whole line of
    /// work exists to remove, so it must not be reachable by a warning.
    fn set_handshake_nonce(bpf: &mut Ebpf, nonce: u32) -> Result<()> {
        debug_assert!(nonce != 0, "a zero nonce arms nothing");
        let map = bpf.map_mut("TRACE_HANDSHAKE_NONCE").context(
            "TRACE_HANDSHAKE_NONCE map not found, so the traced root cannot \
             announce itself and the subtree would stay empty",
        )?;
        let mut arr = aya::maps::Array::<_, u32>::try_from(map)
            .context("TRACE_HANDSHAKE_NONCE is not a usable array")?;
        arr.set(waybill_common::maps::TRACE_HANDSHAKE_NONCE_SLOT, nonce, 0)
            .context("could not write TRACE_HANDSHAKE_NONCE")?;
        Ok(())
    }

    /// #1185 — make the entry tracepoint refuse to stash requests, so the
    /// `kernel_pending_insert_failed` counter can be seen to fire.
    ///
    /// Fails loudly rather than warning, for `set_request_only`'s reason
    /// inverted: an operator who asked for the forced failure and silently
    /// did not get it would read the resulting zero as the counter being
    /// quiet, which is the ambiguity the counter exists to remove.
    fn set_pending_insert_force_fail(bpf: &mut Ebpf) -> Result<()> {
        let map = bpf.map_mut("PENDING_INSERT_FORCE_FAIL").context(
            "PENDING_INSERT_FORCE_FAIL map not found, so the #1185 forced \
             insert failure cannot be enabled",
        )?;
        let mut slot = aya::maps::PerCpuArray::<_, u8>::try_from(map)
            .context("PENDING_INSERT_FORCE_FAIL is not a usable per-CPU array")?;
        let nr_cpus = aya::util::nr_cpus().map_err(|(msg, err)| {
            anyhow::anyhow!(
                "failed to detect online CPU count for PENDING_INSERT_FORCE_FAIL ({msg}): {err}"
            )
        })?;
        let values: aya::maps::PerCpuValues<u8> =
            aya::maps::PerCpuValues::try_from(vec![1u8; nr_cpus])?;
        slot.set(0, values, 0).context(
            "could not write PENDING_INSERT_FORCE_FAIL; the forced failure \
             would not happen and its absence would read as a quiet counter",
        )?;
        warn!(
            "WAYBILL_TRACE_FORCE_PENDING_INSERT_FAIL=1: pretending every open \
             arrives at a full PENDING_OPEN map. Every open will be lost and \
             counted as kernel_pending_insert_failed — this is #1185's symptom \
             on purpose, not a usable capture."
        );
        Ok(())
    }

    fn attach_tracepoint(
        bpf: &mut Ebpf,
        prog: &str,
        category: &str,
        event: &str,
    ) -> Result<()> {
        let p: &mut TracePoint = bpf
            .program_mut(prog)
            .and_then(|p| p.try_into().ok())
            .with_context(|| format!("program '{prog}' not found"))?;
        p.load()?;
        p.attach(category, event)?;
        debug!(prog, category, event, "tracepoint attached");
        Ok(())
    }

    fn attach_uprobe(bpf: &mut Ebpf, prog: &str, lib: &Path, fn_name: &str) -> Result<()> {
        let p: &mut UProbe = bpf
            .program_mut(prog)
            .and_then(|p| p.try_into().ok())
            .with_context(|| format!("program '{prog}' not found"))?;
        p.load()?;
        p.attach(Some(fn_name), 0, lib, None)?;
        debug!(prog, fn_name, "uprobe attached");
        Ok(())
    }

    // #1252 — `attach_kprobe` is deliberately absent.
    //
    // Nothing in this loader may attach a kprobe, because a kprobe's
    // target is an internal kernel function and that is the dependency
    // this feature removed. The helper's absence is the enforcement:
    // reintroducing one means writing the attach code from scratch, which
    // a reviewer notices, and `loader_attaches_no_kprobes` in
    // `waybill-cli/tests/` fails on the attempt. A list of permitted
    // attach sites would not do the same job — a list can grow an entry.

    fn default_ebpf_path() -> PathBuf {
        let p = PathBuf::from("waybill-ebpf/target/bpfel-unknown-none/release/waybill-ebpf");
        if p.exists() { return p; }
        PathBuf::from("target/bpfel-unknown-none/release/waybill-ebpf")
    }

    fn find_libssl() -> Option<PathBuf> {
        for p in &[
            "/usr/lib/x86_64-linux-gnu/libssl.so",
            "/usr/lib/aarch64-linux-gnu/libssl.so",
            "/usr/lib/libssl.so",
            "/usr/lib64/libssl.so",
        ] {
            let path = PathBuf::from(p);
            if path.exists() { return Some(path); }
        }
        None
    }
}

#[cfg(all(target_os = "linux", feature = "ebpf-tracing"))]
pub use inner::load_and_attach;
// `EbpfHandle` lives inside the inner Linux-only module and isn't used
// by any external caller; left non-re-exported to keep clippy clean.

#[cfg(not(all(target_os = "linux", feature = "ebpf-tracing")))]
pub struct EbpfHandle;

#[cfg(not(all(target_os = "linux", feature = "ebpf-tracing")))]
pub fn load_and_attach(_: &LoaderConfig) -> anyhow::Result<EbpfHandle> {
    anyhow::bail!("eBPF tracing requires Linux.")
}
