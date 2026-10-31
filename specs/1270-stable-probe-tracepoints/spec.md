# Feature Specification: Capture that survives a kernel change

**Feature Branch**: `1270-stable-probe-tracepoints`
**Created**: 2026-10-29
**Status**: Draft
**Input**: User description: "Move file-open and TCP-connect capture off kprobes on internal kernel symbols and onto ABI-stable syscall tracepoints, so a GitHub runner-image kernel change cannot silently remove trace coverage. Restores the file-open events lost when do_filp_open stopped being kprobe-able on ubuntu-24.04 image 20261025.353 (#1252)."

## Why this exists

`waybill trace` observes a build by attaching probes to the running kernel.
Some of those probes name **internal kernel functions**, which are not a
stable interface: a kernel build may inline them, rename them, or drop them
from the symbol table, and the probe then cannot attach.

That is not hypothetical. Measured across the last 100 `ci.yml` runs on
`main` (2026-10-04 .. 10-29: 84 passed, 11 failed, 3 cancelled, 2 without
the lane). The runner image version was read from the job log for **all 11
failures and for the 14 most recent passes** — a sample of the passes, not
all 84 — and within that sample the outcome separates on the image with no
overlap:

| runner image | lane outcome in the sample |
| --- | --- |
| `ubuntu-24.04` `20261012.334.1` | 14 of 14 passed |
| `ubuntu-24.04` `20261018.339.1` | 2 of 2 failed |
| `ubuntu-24.04` `20261021.345.10` | 1 of 1 failed |
| `ubuntu-24.04` `20261025.353.1` | 7 of 7 failed |

On the newer images the `do_filp_open` probe cannot attach, and the same
fixture build yields **443 file-open events instead of 902** — a 51% loss
that removed a crate's source file from the compiler's recorded read set.

Two properties of that failure shape this feature:

1. **It is deterministic, not flaky.** Given the image, the result is fixed.
   The appearance of intermittency came from the hosting platform serving
   several image versions concurrently during a rollout: the 14 sampled
   passes all ran on the older image and all fall between 2026-10-23 and
   10-28, straddling failures on 10-21/22 and 10-28 onward.
2. **It does not recover on its own.** It becomes permanent as the pool
   finishes rolling over. The remaining passing runs are a dwindling supply,
   not evidence of health.

A companion change (#1310) already made this loss *visible* — the capture
now records which probes failed and stops reporting itself as complete. This
feature removes the loss itself.

## Clarifications

### Session 2026-10-29

- Q: Network capture attaches to an IPv4-only internal function, so IPv6
  connections are not observed at all. The stable replacement reports both.
  Adopt that coverage, or preserve IPv4-only parity? → A: **adopt it.** IPv6
  connections become observable, which changes emitted documents for any
  build that uses them.
- Q: Capture observes open *requests*, so a request that failed is already
  recorded. Preserve that, or narrow to opens that succeeded? → A: **narrow
  to successful opens.** "The build read this file" becomes true as stated;
  read sets shrink by the failed requests they used to carry.
- Q: Recording only successful opens needs both the request and its result.
  Join them in the kernel, or send both to userspace and join there? → A:
  **join in the kernel**, holding the request per-thread and emitting one
  event once the result is known, so a failed open never reaches the ring
  buffer. Sending both would roughly double open-family volume on a path
  with a history of drops (#614, m212, m213), trading one silent loss for
  another.
- Q: Do the old kprobes on internal symbols stay attached as a best-effort
  fallback, or are they removed? → A: **removed entirely.** The replacement
  interfaces are a superset, so a fallback would buy coverage on no kernel
  anyone runs while keeping the unstable dependency, and its cross-interface
  deduplication, alive. Removal is also what lets a test enforce the absence
  of the dependency rather than relying on review.
- Q: After this feature the interfaces that fail to attach are tracepoints,
  not kprobes, so the existing `kprobe_attach_failures[]` field would carry
  names that are not kprobes. Rename it, add a neutral field, or keep it? →
  A: **keep the existing field and fix its documentation.** It is already a
  union — it has carried counter-map names since m212 — and the attestation
  is treated as a stability surface, so a rename breaks consumers for a
  cosmetic gain.
- Q: SC-001b requires every entry that leaves a read set to be attributable
  to a failed request. How is that attribution established? → A: **an
  instrumented run records each discarded request together with the result
  that caused the discard**, and the committed-expectation diff is checked
  against that list entry by entry. The kernel already holds the result at
  the moment it discards the event, so the information costs nothing to
  surface. Reconciling counts instead would pass equally for a correct
  implementation and for a join that also dropped successful opens.
- Q: SC-002 compares outcomes across the runner image versions in the
  measured set, but the failing image is being rolled out of the pool. Keep
  the criterion as written, or restate it? → A: **restate it as a property
  verified by making the interface unavailable deliberately**, and keep the
  14-against-10 split as the historical baseline that motivated the work. An
  image-dependent criterion stops being evaluated once the image is retired,
  and an unreachable criterion reports nothing rather than failing — the
  same shape as the defect this feature removes.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A build's recorded inputs do not depend on the host kernel (Priority: P1)

An operator traces the same build on two hosts whose kernels differ. Both
traces record the same set of files the compilers read. The operator does not
have to know, or ask, which kernel build each host was running in order to
trust that the attestation lists every input.

**Why this priority**: this is the whole value of the artifact. A read set
that silently depends on the host kernel cannot support the claim the
attestation exists to make — that these are the inputs the build consumed.
Today a consumer comparing two attestations of the same build sees a
difference caused by the observer, with nothing in either document
attributing it there.

**Independent Test**: trace the fixture build on a host where the current
build loses coverage and on one where it does not, and compare the recorded
read sets. Delivers value on its own: it is the restoration of the lost
events.

**Acceptance Scenarios**:

1. **Given** a host whose kernel does not expose the internal functions the
   current probes name, **When** an operator traces the fixture build,
   **Then** every compiler's recorded read set contains each crate root it
   compiled.
2. **Given** two hosts with different kernel builds, **When** the same
   fixture build is traced on each, **Then** the recorded read sets contain
   the same files.
3. **Given** a host where capture is fully available, **When** the fixture
   build is traced, **Then** the recorded read sets differ from today's only
   by the removal of opens that did not succeed. The raw event count is not
   the test here: the clarification above narrows capture to successful
   opens, so a count below today's 902 is the correct outcome and comparing
   against that figure would fail a working implementation.

---

### User Story 2 - A kernel change degrades capture instead of ending it (Priority: P2)

An operator runs a trace on a kernel where one capture interface is
unavailable. The trace still produces an attestation covering everything it
could observe, and says plainly what it could not. It does not abort.

**Why this priority**: the network-connect capture is currently the one place
where an unavailable interface **ends the trace entirely** rather than
reducing it, because its probe is required. It names an internal kernel
function, so it carries the same risk that just materialised for file opens
— but with a worse failure mode. The same change closes a live blind spot:
that function handles IPv4 only, so IPv6 connections are not observed at all
today. Second priority because the fragility is a latent risk rather than a
live outage, and because the blind spot, while real, is silent rather than
breaking anything.

**Independent Test**: make the interface unavailable and confirm a trace is
still produced, carries its own record of what was missing, and does not
describe itself as complete.

**Acceptance Scenarios**:

1. **Given** a kernel on which the network-connect capture interface cannot
   be used, **When** an operator runs a trace, **Then** an attestation is
   produced, it records that the interface was unavailable, and it does not
   report its capture as complete.
2. **Given** a kernel on which no file-open capture interface at all can be
   used, **When** an operator runs a trace, **Then** the trace stops with an
   error naming the missing interfaces, rather than emitting a document that
   reports no file activity.
3. **Given** a build that reaches the network only over IPv6, **When** an
   operator traces it, **Then** the connection is recorded. Today the same
   build is recorded as having made none, which a consumer cannot tell apart
   from an offline build.

---

### User Story 3 - A red lane means waybill changed, not that the pool rolled (Priority: P3)

A maintainer reading a failed eBPF lane learns something about the commit
under test. They do not have to first establish which runner image the job
drew in order to know whether the failure is theirs.

**Why this priority**: it is the maintainer-facing consequence of stories 1
and 2 rather than a separate capability, and the surrounding plumbing (the
failure report, the alarm on a red default branch) already landed in #1310.
Still worth stating, because "the lane is green again" is the signal anyone
will actually check, and because a lane that fails for reasons outside the
repository trains people to ignore it — which is how a 51% coverage loss
went unexamined across two releases.

**Independent Test**: run the lane with each capture interface made
unavailable in turn, and confirm the outcome does not depend on which one
is missing. Stated this way rather than against the two image versions that
separate today's outcomes, because the failing image is leaving the pool and
a test pinned to it would stop running rather than start failing.

**Acceptance Scenarios**:

1. **Given** a host missing an interface the capture used to need, **When**
   the lane runs, **Then** its outcome is the same as on a host that has it.
   The image versions that split 14 passes against 10 failures are the
   historical case this generalises.
2. **Given** a commit that genuinely breaks capture, **When** the lane runs,
   **Then** it fails — the lane retains the ability to fail.

---

### Edge Cases

- **A kernel offers neither the old nor the new file-open interface.** The
  trace must refuse rather than emit a document whose empty file list reads
  as "this build opened nothing". Covered by story 2 scenario 2.
- **Hardware architectures differ in which open operations exist.** Some
  architectures offer only the newer directory-relative forms. Capture must
  not require an operation the architecture does not have, and must not
  report its absence as a failure.
- **The same open observed twice.** No longer possible across interfaces,
  because the old probes are removed and one open is one syscall. Still
  possible across the request and the result of that one syscall, which must
  produce a single record. See FR-005.
- **An open that fails.** Per the clarification, a failed request must not
  reach a read set. This needs both sides of the operation — the request and
  its result — so a host that offers one without the other must degrade to
  requests and say so (FR-004b) rather than quietly guessing either way.
- **A build connects only over IPv6.** Today this is recorded as no network
  activity at all, which is indistinguishable from an offline build. After
  this feature it is recorded as a connection, so a document that previously
  claimed no connections for such a build will change.
- **Capture observes work outside the traced build.** The new interfaces fire
  for every process on the host, as the current ones do. The existing
  attribution and noise-filtering behaviour must be preserved. Volume is no
  longer a growth risk, because the request and result are joined in the
  kernel and failures are discarded there — a build's successful opens are
  strictly fewer than its open requests, so the ring buffer sees less than
  it does today, not more.
- **A held request whose result never arrives.** A thread killed inside the
  syscall, or a result the capture never sees, leaves a pending entry. It
  must not accumulate, and it must not be mistaken for the request belonging
  to a later open on the same thread. See FR-005c.
- **A probe attaches but never fires.** Indistinguishable from a quiet build
  unless something asserts a positive signal; the fixture build is known to
  produce compiler activity, so a zero count there is a failure, not a pass.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Capture of file-open activity MUST NOT depend on the presence
  of any internal kernel function name.
- **FR-002**: Capture of network-connect activity MUST NOT depend on the
  presence of any internal kernel function name.
- **FR-002a**: Network-connect capture MUST observe connections over both
  IPv4 and IPv6. Today only IPv4 is observed, so a build that connects over
  IPv6 is recorded as having made no connection at all.
- **FR-003**: On a kernel where the current build loses file-open coverage,
  the recorded read sets MUST contain every crate root compiled by the
  fixture build.
- **FR-004**: Recorded read and write sets MUST NOT lose any entry that the
  current build records on a kernel where the current build is fully
  functional, **except** entries for open requests that did not succeed,
  which FR-004a removes deliberately.
- **FR-004a**: An open request that did not succeed MUST NOT appear in a
  recorded read or write set. The set's claim is that the build read these
  files; a request that failed did not read anything.
- **FR-004b**: Where the information needed to tell a successful open from a
  failed one is unavailable on a host, capture MUST fall back to recording
  requests, MUST record that fallback as a capture gap, and MUST NOT report
  the capture as complete. Silently recording requests as though they were
  reads is the failure mode this feature exists to remove, in a smaller
  form.
- **FR-004c**: Capture MUST be able to report, on request, each open request
  it discarded together with the result that caused the discard. This is the
  evidence SC-001b is verified against, so it MUST name the path, not only
  count the discards. It MUST NOT change what a run records when it is not
  requested, and MUST NOT be required for a trace to run.
- **FR-005**: A single file-open MUST be recorded once. With the old probes
  removed, the overlap this originally guarded — two interfaces observing
  the same open — no longer exists, since one open is one syscall and so is
  seen by one interface. It now guards the remaining way to double-count:
  an open is observed at request and again at result, and those two
  observations are one event, not two.
- **FR-005b**: The request and its result MUST be joined before the event
  reaches userspace, and an open that did not succeed MUST NOT occupy
  ring-buffer space at all. Per-build event volume MUST NOT increase as a
  result of this feature.
- **FR-005c**: A held request whose result never arrives MUST NOT accumulate.
  The hold is per-thread and bounded; a thread that dies mid-syscall, or a
  result that is never observed, MUST NOT leave an entry behind or cause a
  later open on the same thread to inherit it.
- **FR-005a**: The removed capture MUST NOT remain reachable — not attached,
  not behind a flag, not as a fallback. A dependency that can be switched
  back on is a dependency, and FR-012's test must have something
  unambiguous to assert.
- **FR-006**: Where an architecture does not provide a given open operation,
  its absence MUST NOT be recorded as a capture failure.
- **FR-007**: Loss of all file-open capture MUST stop the trace with an error
  that names what could not be attached.
- **FR-008**: Loss of network-connect capture MUST NOT stop the trace; it
  MUST be recorded as a capture gap and MUST prevent the trace from
  reporting itself complete.
- **FR-009**: Every capture interface that fails to become usable MUST be
  named in the emitted attestation, and the attestation's completeness
  statement MUST be consistent with that list.
- **FR-010**: The attributed set of events belonging to the traced build MUST
  be unchanged in membership by this feature.
- **FR-011**: The project MUST record, next to the capture code, which
  interfaces it treats as stable and why, so a future change does not
  reintroduce a dependency on an unstable one.
- **FR-011a**: The attach-failure list MUST keep its existing field name and
  position, so documents produced before and after this feature stay
  comparable and no consumer has to be changed. Its documentation MUST state
  that it carries any capture interface that failed to become usable,
  whatever kind it is, because the field name names only one kind and a
  reader who trusts the name will misread the contents.
- **FR-012**: A test MUST fail if capture of file-open or network-connect
  activity regains a dependency on an internal kernel function name.
- **FR-013**: The integration harness MUST assert a positive signal — a
  non-zero count of observed compiler activity and the presence of named
  inputs — rather than the absence of an error.

### Key Entities

- **Capture interface**: a named way of observing one kind of kernel
  activity, which on a given host is either usable or not. Carries one
  attribute this feature turns on: whether the interface is a stable
  contract or an implementation detail of a particular kernel build.
- **Capture gap**: the record that a named interface was unusable on this
  host, carried in the attestation and bearing on whether the capture may
  describe itself as complete. Carried in the existing attach-failure list,
  whose name is historical: it already holds entries that are not kprobes,
  and this feature adds more. The name is kept for wire stability and the
  documentation states what it actually holds.
- **Recorded input set**: the files a traced compiler read, as observed. Its
  correctness claim is that it is complete for the traced build; a capture
  gap reduces that claim to a floor.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On the runner image where the current build records 443
  file-open events for the fixture build, every crate root the fixture
  compiles appears in the corresponding recorded read set. This is the
  restoration claim, stated as content rather than as a count, because the
  902-event figure was measured under the old request-based semantics and
  the clarification above narrows capture to successful opens — a lower
  count is now the correct outcome, so comparing against 902 would fail a
  working implementation.
- **SC-001a**: An implementation task MUST establish the successful-open
  baseline before the change, on a host where capture is intact, and the
  count after the change MUST be no lower on that same semantics. Expressed
  as a ratio against a baseline a task will establish rather than as an
  absolute, because no measurement of successful-opens-only exists yet.
- **SC-001b**: On a host where capture is intact, the recorded read sets
  after the change MUST differ from before only by the removal of entries
  whose open did not succeed. Every removal MUST be attributable to a failed
  request; any other difference is a regression. Attribution is established
  per entry, against the discard record FR-004c produces, and every removed
  entry MUST appear in it. Accepting the diff because its size matches a
  discard count is not verification: the same count is consistent with a
  join that dropped successful opens too.
- **SC-002**: The lane's outcome does not depend on which runner image it
  lands on. Verified by making each capture interface unavailable on purpose
  — the SC-005 mechanism — rather than by waiting for a particular image to
  be scheduled: the measured set splits 14 passes against 10 failures, but
  the failing image is leaving the pool, and a criterion pinned to it would
  go quiet instead of red once it is gone. The split stays here as the
  historical baseline that motivated the work, not as the test.
- **SC-003**: Across 30 consecutive default-branch runs after the change, the
  number of lane failures attributable to the host kernel or runner image is
  zero. Baseline: 11 failures in the 100 runs measured before the change, 10
  of them attributable to one unusable interface.
- **SC-004**: Zero capture of file-open or network-connect activity depends
  on an internal kernel function name, enforced by a test rather than by
  review.
- **SC-008**: `ring_buffer_overflows` remains 0 for the fixture build, the
  value the integration harness already asserts, and the count of events the
  ring buffer carries for that build does not exceed today's. Measured
  rather than argued, because the kernel-side join is the reason to expect
  it and an implementation that quietly sent both observations to userspace
  would still pass every content-based criterion above.
- **SC-005**: A trace on a host missing one capture interface still produces
  an attestation, and that attestation names the gap and does not claim
  completeness — verified by making an interface unavailable deliberately
  rather than by waiting to observe it.
- **SC-006**: On a host where the current build is fully functional, the
  recorded input set for the fixture build changes only as SC-001b permits,
  and the network-connect record gains IPv6 connections where the build
  makes them. No other difference appears, so the change adds coverage and
  removes a known overstatement without disturbing what was already correct.
- **SC-007**: A build that connects only over IPv6 is recorded as having
  made that connection. Today the same build is recorded as having made
  none, so this is verified against a deliberately IPv6-only case rather
  than inferred from a build that happens to use both.

## Assumptions

- **The host kernel is not under the project's control, and pinning it is not
  available.** A hosted runner's image version cannot be pinned, and a
  container shares the host kernel, so containerising the test provides no
  insulation. Running the test under a virtual machine with a fixed kernel
  would, and is deliberately **out of scope** here: this feature removes the
  dependency on unstable interfaces, which is worth doing whether or not the
  test environment is later pinned as well.
- **Failed opens leave the read set, by decision.** The current capture
  observes open *requests*, so a request that failed is already recorded.
  The clarification narrows this to opens that succeeded. Two consequences
  are accepted: existing read sets shrink, so committed expectations change;
  and capture now needs the result of an open as well as the request, which
  is a second interface that can be unavailable — handled by FR-004b rather
  than assumed present.
- **IPv6 connections become visible, by decision.** The clarification adopts
  the address-family coverage the stable interface provides. This is a
  coverage increase, not a refactor: documents for IPv6-connecting builds
  change, and any consumer that read "no connections" as "offline" was
  already being misled for those builds.
- **Read and write capture of already-open files stays out of scope.** Two
  interfaces for that exist in the code and are deliberately not used; this
  feature neither enables nor removes them. Note the asymmetry with the
  decision above: those two were never attached, so they are not a live
  dependency on an unstable name, whereas the ones being removed are.
- **The replacement interfaces are a superset, pending confirmation.** The
  decision to delete rather than keep a fallback rests on the syscall-level
  interfaces covering every open the removed probes caught, and on their
  being available on every kernel this project supports. Both are stated
  here as the basis for the decision and MUST be confirmed by an
  implementation task, not assumed: if either turns out false, the removal
  is what needs revisiting, not the tracepoint migration.
- **The visibility work is already done.** #1310 landed the capture-gap
  record, the completeness downgrade, the refusal when no file-open
  interface is available, and the harness consistency check. This feature
  depends on those and does not redo them.
- **The fixture build is the measurement instrument.** All counts quoted here
  come from the existing two-binary fixture traced by the integration
  harness, so before/after numbers are comparable.
- **One unexplained failure is excluded.** Of the 11 measured failures, the
  one on 2026-10-14 has a different and still unidentified cause and is not
  claimed by this feature. SC-003 counts it against the baseline anyway
  rather than excusing it.
