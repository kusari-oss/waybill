# Quickstart: Go Build-Inclusion Clarity

**Feature**: 112-go-build-inclusion

## Try it (post-implementation)

```bash
# Part B + C together (toolchain on PATH):
mikebom sbom scan --path ~/Projects/kusari-cli \
  --format cyclonedx-json --output cyclonedx-json=/tmp/kc.cdx.json

# Inspect classifications:
jq '[.components[] | select(.purl|startswith("pkg:golang"))
     | {purl, scope, props: [.properties[]?
        | select(.name|startswith("mikebom:build-inclusion")
                 or startswith("mikebom:lifecycle-scope"))]}]' /tmp/kc.cdx.json

# Part B only (analysis disabled):
MIKEBOM_NO_GO_MOD_WHY=1 mikebom sbom scan --path ~/Projects/kusari-cli ...
# → fallback-discovered modules carry mikebom:build-inclusion: unknown
```

## Validation walkthrough (maps to Success Criteria)

1. **SC-001 (unknown markers, no toolchain)**: run with
   `MIKEBOM_NO_GO_MOD_WHY=1` against the kusari-cli tree; every
   component outside cyclonedx-gomod's build list (20 at spec time)
   carries `mikebom:build-inclusion: unknown`; no others do.
2. **SC-002 (toolchain parity)**: run default against the same tree;
   `jq` for `unknown` → empty; the outside-build-list components are
   `not-needed` (scope excluded) or test-scoped; cross-check no
   build-list module is excluded:
   `cyclonedx-gomod mod -json | jq` vs the mikebom output.
3. **SC-003 (never fails)**: `PATH=/tmp/broken-go-stub:$PATH mikebom ...`
   where the stub exits 1 → scan exits 0, warn log
   `subprocess-error`, SBOM valid.
4. **SC-004 (byte identity)**: `./scripts/pre-pr.sh` — golden suites run
   with `MIKEBOM_NO_GO_MOD_WHY=1`; only Go goldens drift, only by the
   documented annotations (one-time regeneration:
   `MIKEBOM_UPDATE_CDX_GOLDENS=1 cargo test --test cdx_regression`,
   `MIKEBOM_UPDATE_SPDX_GOLDENS=1 cargo test --test spdx_regression`).
5. **SC-005 (consumer determinability)**: for each golang component,
   exactly one status is readable from scope + properties per the
   contracts/annotations.md table.

## Hermetic test pattern (stub toolchain)

Part C integration tests do not require a real Go toolchain. They
prepend a temp dir to PATH containing a `go` shell script
(`#[cfg(unix)]`) that emits canned `go mod why` output:

```sh
#!/bin/sh
# stub: classify fixture modules
cat <<'EOF'
# example.com/needed
fixturemod/cmd
example.com/needed/pkg

# example.com/testonly
fixturemod/cmd
fixturemod/cmd.test
example.com/testonly/assert

# example.com/orphan
(main module does not need to vendor module example.com/orphan)
EOF
```

The stub must branch on its subcommand: `go list all` (the reliability
preflight) → exit 0; `go mod why` → the canned output above. The
not-needed phrasing includes "to vendor" because the real invocation
passes `-vendor` (the parser matches the `(main module does not need`
prefix either way).

Variants: exit-1 (subprocess-error), `sleep 120` (budget exhaustion —
shorten via `MIKEBOM_GO_MOD_WHY_BUDGET_MS`), preflight `list` exiting 1
(unresolvable-packages — no verdicts accepted),
partial output (per-module Unresolved). A real-toolchain end-to-end
test is env-gated (`MIKEBOM_GO_TOOLCHAIN_E2E=1`), mirroring the
docker-daemon test pattern.

## Key implementation files

| Concern | Path |
|---|---|
| `BuildInclusion` enum + field | `mikebom-common/src/resolution.rs` |
| `go mod why` runner + parser | `mikebom-cli/src/scan_fs/package_db/golang/mod_why.rs` (new) |
| Classification + unknown-marker passes | `mikebom-cli/src/scan_fs/package_db/mod.rs` (`read_all`, after the existing G3/G4 filters) |
| CDX scope/property emission | `mikebom-cli/src/generate/cyclonedx/builder.rs` (~599, ~928) |
| SPDX 2.3 / SPDX 3 annotations | `mikebom-cli/src/generate/spdx/annotations.rs`, `v3_annotations.rs` |
| CLI flag | `mikebom-cli/src/main.rs` |
| Parity catalog | `docs/reference/sbom-format-mapping.md` + `mikebom-cli/src/parity/catalog.rs` |
| Goldens | `mikebom-cli/tests/fixtures/golden/{cyclonedx,spdx-2.3,spdx-3}/golang.*` |

## Pre-PR gate (unchanged, mandatory)

```bash
./scripts/pre-pr.sh           # clippy --workspace --all-targets + test --workspace
# host with broken docker DNS:
MIKEBOM_SKIP_DOCKER_INTEGRATION=1 ./scripts/pre-pr.sh
```
