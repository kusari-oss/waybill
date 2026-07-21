# m210 compiler-pipeline fixtures

Vendored fixture projects driving the milestone 210 integration
tests. See `specs/210-compiler-pipeline-trace/tasks.md` T015..T017
for the source-of-truth task descriptions.

## `two_binaries_diverge/` (T015 — SC-001 coverage)

A Cargo workspace with:

- `libsafe/` — hypothetical safe library, one function.
- `libvuln/` — hypothetical vulnerable library, one function.
- `binaries/safe-only/` — binary depending on **libsafe only**.
- `binaries/vuln-included/` — binary depending on **libsafe + libvuln**.

**Test assertions** (per T035):

1. `safe-only`'s `mikebom:source-read-set` MUST NOT contain any
   `libvuln` source path.
2. `vuln-included`'s `mikebom:source-read-set` MUST contain paths
   from BOTH `libsafe` AND `libvuln`.
3. Both binaries' source-read-sets MUST contain `libsafe` paths.

**Regeneration**: this is a hand-authored fixture, not generated.
Edit files in place; commit changes.

## `secrets_touch/` (T016 — FR-016a coverage)

**Status**: not yet vendored (needs the shell-script + Cargo
scaffold — deferred to a Linux implementation session where
`sudo mikebom trace run` can actually exercise it).

Intended shape: a shell script that reads from a synthetic secret
path THEN compiles a trivial C program. The trace attaches to the
shell; the fake secret's path lands in the denylist filter, and the
resulting SBOM carries `mikebom:secrets-read-filtered = "1"` at
document scope.

## `stdin_input/` (T017 — FR-018 coverage)

**Status**: not yet vendored (needs the invocation script — deferred
to a Linux implementation session).

Intended shape: `gcc -x c - -o /tmp/stdin_output` with the C source
piped in via stdin. Verifies the `mikebom:stdin-input` marker fires
correctly.

## Why the deferrals

The eBPF programs that populate the compiler-pipeline data
(`sched_process_exec` tracepoint, kprobe extensions) are Linux-only
kernel-side code that requires nightly Rust + `bpf-linker` +
`CAP_BPF` to compile-verify and test. This session (macOS host) can
land the user-space Rust types + the `two_binaries_diverge` fixture
which are OS-agnostic; the `secrets_touch` + `stdin_input` fixtures
depend on `sudo mikebom trace run` actually working, so they land
alongside their tests in the Linux session.
