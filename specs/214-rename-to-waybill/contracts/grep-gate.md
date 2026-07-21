# Contract: CI grep-gate for SC-001 (rename bug detection)

**Feature**: 214-rename-to-waybill
**Kind**: CI-level enforcement contract
**Consumers**: `.github/workflows/ci.yml` — new step run alongside the existing walker-audit-allowlist check.

## Purpose

Enforce SC-001: `grep -rE '\bmikebom\b' <in-scope paths>` returns **zero hits** on the post-rename tree. Any hit outside the allowlist is a rename bug.

## In-scope paths (grep MUST find zero hits)

```
waybill-cli/src/**
waybill-common/src/**
waybill-ebpf/src/**
xtask/src/**
Cargo.toml
waybill-cli/Cargo.toml
waybill-common/Cargo.toml
waybill-ebpf/Cargo.toml
xtask/Cargo.toml
.github/workflows/*.yml
Dockerfile*
scripts/*.sh
waybill-cli/tests/**/*.rs
waybill-common/tests/**/*.rs
waybill-cli/build.rs (if exists)
waybill-ebpf/build.rs (if exists)
```

## Allowlisted paths (grep hits acceptable)

```
specs/**              # historical spec directories (specs/001-* through specs/213-*)
docs/**               # narrative docs, including one heritage sentence per doc
README.md             # one heritage sentence: "Waybill (previously known as mikebom)…"
CHANGELOG.md          # historical entries preserve original names
.git/**               # implicit; grep never traverses .git
MEMORY.md             # user-personal
.specify/**           # spec-kit templates + old constitution history in SYNC IMPACT REPORT
docs/migration/mikebom-to-waybill.md  # migration guide literally names the old name
docs/audits/**        # historical audit reports
specs/214-rename-to-waybill/**  # this feature's own artifacts (spec/plan/research/etc. reference mikebom by name)
Cargo.lock            # transitive package-metadata may reference historical package names
waybill-cli/tests/fixtures/**  # test fixtures may embed mikebom as external-input data (e.g., a rpm fixture whose upstream calls itself mikebom is legitimately preserved)
```

## Gate implementation

```yaml
# .github/workflows/ci.yml — new step, added after walker-audit check
- name: "m214 rename-completeness grep gate (SC-001)"
  shell: bash
  run: |
    set -u
    IN_SCOPE=(
      waybill-cli/src
      waybill-common/src
      waybill-ebpf/src
      xtask/src
      Cargo.toml
      waybill-cli/Cargo.toml
      waybill-common/Cargo.toml
      waybill-ebpf/Cargo.toml
      xtask/Cargo.toml
      Dockerfile.ebpf-test
      scripts
    )
    for wf in .github/workflows/*.yml; do IN_SCOPE+=("$wf"); done
    BADHITS=$(grep -rE '\bmikebom\b' "${IN_SCOPE[@]}" 2>/dev/null | \
      grep -v '^Binary file' | \
      grep -vE '\.git/' || true)
    if [[ -n "$BADHITS" ]]; then
      echo "::error::m214 rename gate — mikebom found in functional-identifier positions:"
      echo "$BADHITS" | head -30
      COUNT=$(echo "$BADHITS" | wc -l)
      if [[ "$COUNT" -gt 30 ]]; then
        echo "::error::… and $((COUNT - 30)) more hits (showing first 30)"
      fi
      echo "::error::Fix by running rename_pass.py or via /speckit.tasks T### for m214, or add path to allowlist in this workflow step if legitimately preserved."
      exit 1
    fi
    echo "m214 rename gate: zero mikebom hits in functional-identifier paths (SC-001 satisfied)"
```

## Contract stability

- The in-scope list is the source of truth. Adding a new source-tree directory (e.g., a fourth crate someday) requires updating both `Cargo.toml` `[workspace].members` AND this gate's in-scope list.
- The allowlist should be EXPANDED, not modified retroactively. If a new pre-existing path emerges that legitimately preserves `mikebom` (e.g., a new heritage doc), add it to the allowlist rather than editing existing preserved paths.
- The gate runs alongside every CI invocation — clippy + tests + this gate all must pass to merge.

## Failure semantics

- Exit code 1 fails the CI job → PR cannot merge.
- Emits GitHub Actions error annotation with first 30 offending lines for quick diagnosis.
- Points to `rename_pass.py` (m214's local script) OR to specific task IDs from `tasks.md` for follow-up.
- If the gate flags a false positive (e.g., a new file at a path that should be allowlisted), the fix is to update this workflow step's allowlist, not to rewrite the offending file.
