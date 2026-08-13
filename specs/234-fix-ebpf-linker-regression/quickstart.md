# Quickstart: Durable eBPF Build Resilience

**Feature**: `234-fix-ebpf-linker-regression`
**Audience**: waybill release lead, waybill maintainers, contributors
who want to verify eBPF-toolchain changes locally.

---

## Scenario A — I'm the release lead. Un-pin bpf-linker.

The interim pin is `bpf-linker@0.10.4`. Upstream has published a fixed
version (e.g., `0.12.1`). You want to move to it.

```bash
# 1. Verify locally that the candidate version works.
scripts/verify-ebpf.sh --version 0.12.1
# Expect: "verify-ebpf: PASS — bpf-linker v0.12.1 works end-to-end."

# 2. Update the single source of truth.
sed -i 's/^BPF_LINKER_VERSION=.*/BPF_LINKER_VERSION=0.12.1/' \
    .github/env/bpf-linker.env

# 3. Open the bump PR.
git checkout -b bump/bpf-linker-0.12.1
git add .github/env/bpf-linker.env
git commit -m "bpf-linker: bump 0.10.4 → 0.12.1 (upstream fix landed)"
git push -u origin bump/bpf-linker-0.12.1
gh pr create --title "chore(deps): bump bpf-linker to 0.12.1" \
             --body "Verified locally via scripts/verify-ebpf.sh.
Canary has been green for the past N runs against 0.12.x."

# 4. Wait for CI. The ebpf-tracing lane MUST pass.
# 5. Merge. Next nightly + release will use 0.12.1.
```

**Acceptance for Scenario A**: PR merges on first push (no fixup
commits); next nightly release completes without touching install
steps.

---

## Scenario B — I'm a maintainer. Canary just opened an issue.

You see a new GitHub issue titled
`[canary] bpf-linker eBPF build regression` labeled `canary`, `ebpf`,
`regression`. The body cites the version, failing step, and log tail.

```bash
# 1. Reproduce locally with the reported version.
scripts/verify-ebpf.sh --version <version-from-issue>
# Expect: matching FAIL output that names bpf-linker.

# 2. If reproduced, file upstream.
#    Open https://github.com/aya-rs/bpf-linker/issues/new
#    Paste: the FAIL output from step 1 + a link to the canary run URL.

# 3. Update the canary tracking file with the upstream issue link.
#    Edit .github/env/bpf-linker.env:
#    BPF_LINKER_UPSTREAM_ISSUE=https://github.com/aya-rs/bpf-linker/issues/<N>
#    BPF_LINKER_FALLBACK_DEADLINE=<today + 30 days, ISO-8601>

# 4. Comment on the waybill canary issue with the upstream link.
#    The canary will keep posting comments on subsequent runs until
#    the regression is resolved.

# 5. Wait for upstream fix OR the fallback deadline.
```

**Acceptance for Scenario B**: within 15 minutes of first seeing the
canary issue, the maintainer has an upstream bug report filed.

---

## Scenario C — I'm the release lead. Upstream fallback window expired.

30 days have passed since the canary issue was opened; upstream hasn't
fixed the regression. Execute the downstream mitigation.

The downstream mitigation shape is deferred to the tasks phase — it
will be one of:

- Explicit LLVM path setup step in the composite action
  (`sudo apt-get install -y llvm-<version>` + `LD_LIBRARY_PATH` export).
- Alternative install method (pre-built binary from GH releases,
  cargo-binstall, etc.).
- Fork bpf-linker + patch + install from git.

The choice is made in `/speckit.tasks` based on what the specific
regression demands. Whatever the choice, this scenario looks like:

```bash
# 1. Update the composite action + env file.
# 2. Run scripts/verify-ebpf.sh against latest bpf-linker with mitigation.
# 3. Open PR. Merge. Canary un-pins (goes green again).
```

**Acceptance for Scenario C**: `BPF_LINKER_VERSION=latest` works
end-to-end with the mitigation applied; canary auto-closes the
tracking issue on first green run post-merge.

---

## Scenario D — I'm a contributor. I want to test my bpf-linker change locally.

You have a patched bpf-linker branch and want to verify it against
waybill before opening an upstream PR.

```bash
# 1. Point to your local bpf-linker via cargo install --path.
cargo install --path /path/to/your/bpf-linker/checkout

# 2. Run the verify script with --version latest (skips the pin check).
BPF_LINKER_VERSION=latest scripts/verify-ebpf.sh --version latest
# Note: --version latest tells cargo install to use HEAD;
#       since you already installed from --path, the install step
#       is a no-op (cache hit) and verify proceeds to build steps.

# 3. Alternatively, force container path with your patched image.
docker build -f Dockerfile.ebpf-test \
    --build-arg BPF_LINKER_VERSION=latest \
    -t waybill-ebpf-mypatch .
```

**Acceptance for Scenario D**: contributor gets pass/fail signal
without needing to open a PR against waybill.

---

## Scenario E — I want to smoke-test the canary manually.

Force a canary run with a known-broken version to make sure the
issue-creation path works.

```bash
gh workflow run ebpf-canary.yml \
    -f version=0.11.0 \
    -f dry_run=false \
    --ref main \
    --repo kusari-oss/waybill

# Watch the run; when it fails, verify:
# 1. A new issue titled "[canary] bpf-linker eBPF build regression"
#    appeared with labels canary/ebpf/regression.
# 2. Body cites bpf-linker@0.11.0 explicitly.
# 3. Body has runnable next-step commands.

# Re-run with the same broken version:
gh workflow run ebpf-canary.yml -f version=0.11.0 -f dry_run=false --ref main --repo kusari-oss/waybill

# Verify: same issue got a NEW COMMENT (not a new issue).

# Verify recovery:
gh workflow run ebpf-canary.yml -f version=0.10.4 -f dry_run=false --ref main --repo kusari-oss/waybill

# Verify: the issue was auto-closed with a "canary green" comment.
```

**Acceptance for Scenario E**: dedup works, close-on-recovery works,
body content matches contract.
