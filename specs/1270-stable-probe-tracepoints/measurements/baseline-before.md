# Baseline: what capture records before the change

Taken 2026-10-29 with the container harness built from the pristine tree
at `2f9be2e6` (the commit before this feature), so nothing in it is
contaminated by work in progress. Reproduce with `probe.sh` below.

**Host**: aarch64, Linux 6.8.0-136-generic (Colima VM), Docker
`--privileged` with `-v /sys/kernel/debug:/sys/kernel/debug`. Capture is
**intact** on this host: the harness passes, `kprobe_attach_failures` is
empty and `completeness` is `complete`, which is what T002 requires of a
baseline host. It is not the x86_64 runner image where the defect fires;
run 2 of the inventory and a second baseline are still owed from the
lane.

## The numbers

| Quantity | Value |
|---|---|
| File operations recorded | 573 (500 read, 73 write) |
| Distinct read paths | 137 |
| Distinct write paths | 38 |
| `ring_buffer_overflows` | 0 |
| `events_dropped` | 0 |
| `kprobe_attach_failures` | `[]` |
| `unresolved_relative_opens` | 10 |
| `completeness` | `complete` |
| Compiler invocations | 9 (7 rustc) |

573 is the **SC-008 ceiling**: the event count after the change must not
exceed it.

Two runs of the same fixture on the same host produced 573 and 575
operations. The count is not bit-stable, so SC-008 must be read as a
ceiling with that much slack, not as an equality.

## How much of the read set is an overstatement

Every recorded read is an open *request*, so a request that failed is in
the set. Classifying the 137 distinct read paths by whether they exist
at the end of the build — **in the same container that produced them**,
see the trap below:

| Class | Distinct paths | What it is |
|---|---|---|
| Present at build end | 25 | Opened and still there |
| Absent, search-path shape | ~84 | `libc.so.6`, `libgcc_s.so.1`, `libbfd-*.so`, `librustc_driver-*.so`, `libutil.{so,a}`, `librt.{so,a}` and friends **inside the fixture's `target/release/deps/`**, plus `benches`/`examples`/`tests`/`src/bin` directory probes and the `tls/aarch64/atomics` hwcap variants. These never existed. They are failed opens, recorded today as "the build read this file" |
| Absent, build temporary | ~52 | `rustcXXXXXX/raw-dylibs`, `.tmpXXXXXX.temp-archive/tmp.a`, `rmetaXXXXXX/full.rmeta`, `*.rcgu.o`. Opened **successfully**, then removed or renamed into place |

The classes overlap slightly, so the counts do not sum to 110. The
honest statement is bounded, not exact: **110 of 137 distinct read paths
are absent at build end, of which roughly 84 have the shape of a failed
search-path probe and roughly 52 are successful opens of files that no
longer exist.** The precise split is what the discard record (FR-004c)
will establish; this is the magnitude it should be checked against, so a
discard count of three or of three thousand both read as wrong before
anyone inspects the diff.

All 38 distinct write paths were checked the same way: 19 present, 19
absent. The absent ones are the compiler's temporary archives and
renamed-into-place objects — m1070's rationale, not failures.

## The trap this measurement fell into first

The first attempt classified the paths in a **fresh** container. Every
file the traced build had produced was therefore missing, and the result
read "130 of 138 read paths absent, 38 of 38 write paths absent" — which
looks like a far larger overstatement than the real one, and would have
been quoted as evidence. A `docker run --rm` per step measures a
filesystem that never saw the build.

The fix is in `probe.sh`: trace and classify in one container
invocation. Anything that re-enters the container has already lost the
state it is asking about.

## SC-001a is not executable as written, and why

SC-001a asks an implementation task to "establish the successful-open
baseline **before** the change". It cannot be done: the pre-change build
observes requests and has no idea which succeeded. That is the defect,
so it cannot also be the instrument.

The executable form, which is strictly stronger:

- the baseline is the **request** set, recorded above;
- after the change, the recorded set plus the discard record MUST
  reconstitute it — `after ∪ discarded == before`, with
  `after ∩ discarded == ∅`;
- "no lower than the baseline" then means no entry vanished without
  appearing in the discard record, which is SC-001b stated as an
  identity rather than as a comparison of two counts.

Recorded here rather than silently reinterpreted. The spec should be
amended to this form.

## probe.sh

```bash
#!/bin/bash
set -u
F=/waybill/waybill-cli/tests/fixtures/compiler_pipeline/two_binaries_diverge
/waybill/target/release/waybill trace capture --attestation-format waybill-v1 \
  --output /out/baseline.json \
  -- cargo build --release --manifest-path $F/Cargo.toml --target-dir $F/target \
  > /dev/null 2>&1
echo "trace exit=$?"
python3 - <<'PY'
import json, os, collections
d = json.load(open('/out/baseline.json'))
p = d.get('predicate', d)
ops = p['file_access']['operations']
print('operations:', len(ops), dict(collections.Counter(o['operation'] for o in ops)))
for kind in ('read', 'write'):
    paths = sorted({o['path'] for o in ops if o['operation'] == kind})
    rel = [x for x in paths if not x.startswith('/')]
    absolute = [x for x in paths if x.startswith('/')]
    present = [x for x in absolute if os.path.lexists(x)]
    absent = [x for x in absolute if not os.path.lexists(x)]
    print(f'{kind}: distinct={len(paths)} present={len(present)} '
          f'absent={len(absent)} unresolved_relative={len(rel)}')
    open(f'/out/{kind}-absent.txt', 'w').write('\n'.join(absent) + '\n')
    open(f'/out/{kind}-all.txt', 'w').write('\n'.join(paths) + '\n')
ti = p['trace_integrity']
print('ring_buffer_overflows=%s events_dropped=%s kprobe_attach_failures=%s '
      'unresolved_relative_opens=%s' % (ti['ring_buffer_overflows'],
      ti['events_dropped'], ti['kprobe_attach_failures'],
      ti.get('unresolved_relative_opens')))
print('completeness=', p['compiler_pipeline']['completeness'])
PY
```

Run it as:

```sh
docker run --rm --privileged -v /sys/kernel/debug:/sys/kernel/debug \
    -v "$HOME/wb-1270-out:/out" --entrypoint /bin/bash \
    waybill-ebpf-test:baseline /out/probe.sh
```

The `-v /sys/kernel/debug` mount is required — without it every
tracepoint attach fails with `tracefs not found` and the trace aborts at
`sys_enter_chdir`. `scripts/ebpf-container-run.sh` is what CI uses and
it passes the mount; a bare `docker run --privileged` does not.
