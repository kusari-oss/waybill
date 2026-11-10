# Quickstart: verifying milestone 1377

Requires Nix, and grype 0.120.0 (`nix build --no-link --print-out-paths nixpkgs#grype`).
sbomnix 1.9.0 is needed only for the reference column.

```sh
cargo build -p waybill --bin waybill
M=specs/1377-nix-subject-identity/measurements

# Copy the pinned targets out of the repo: each must be its own git flake,
# and evaluating a flake in place can re-lock it.
W=$(mktemp -d); cp -R $M/wrapper-flakes/* $W/
for t in $W/*; do (cd $t && git init -q && git add . && git -c user.name=q -c user.email=q@q commit -qm q); done

# Scan, then judge (paths come from the environment, research R9)
TARGETS_DIR=$W OUT_DIR=$W/out WAYBILL=$PWD/target/debug/waybill bash $M/gen-sboms.sh waybill
OUT_DIR=$W/out bash $M/scan.sh
```

| check | how | expected |
| --- | --- | --- |
| SC-001 | `jq '.metadata.component \| {name,version,cpe}'` on openssh's CycloneDX document; SPDX 2.3 `documentDescribes`; SPDX 3 `rootElement` | `openssh` `10.3p1` in all three |
| SC-002 | `analyze.py`, openssh, grype, waybill column | ≥ 28 findings on `openssh 10.3p1` |
| SC-003 | the 16 declared-CPE members (`measurements/meta-eval-cost/README.md`) against the document | all 16 carry the declared CPE; bash `gnu:bash:5.3:9`; gnugrep `gnu:grep` |
| SC-004 | `measurements/bash-cpe-encoding/` with the emitted bash CPE | CVE-2019-18276 matched |
| SC-005 | every closure component with a CPE | a native provenance carrier in each format |
| SC-006 | `analyze.py` + `precision.py`, all six targets, against the spike baseline | no lost finding except by CPE source change; distinct patched FPs ≤ 6 |
| SC-007 | regenerate every golden, `git diff --stat` | only `nix-closure-moat` and closure-test fixtures change |
| SC-008 | moat from `gen-sboms.sh` (no override); then the `nix-closure-moat` golden diff | probe: subject `pkg:hackage/moat@0.1`, heuristic `nix-evaluated-package`, no `waybill:` CPE. Golden: subject still `nix-closure-moat` (harness `--root-name` wins), changes only in closure-member CPEs and provenance and C184 |
| SC-009 | `identifiers.wanted − identifiers.received` from C184, against `measurements/meta-eval-cost/reach.py` run on the same closure | equal |
