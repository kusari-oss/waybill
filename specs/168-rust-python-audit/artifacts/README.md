# Milestone 168 audit intermediate artifacts

This directory holds regenerable intermediate artifacts from the m168
Round-4 audit against Tauri + Apache Airflow. **The contents are
gitignored** except for this README — everything under
`tauri-src/`, `airflow-src/`, `tauri/`, and `airflow/` is
regenerable from the pinned commit SHAs recorded in the audit report
header (`docs/audits/2026-07-06-tauri-airflow.md`).

Mirrors m165's `specs/165-k8s-argocd-audit/artifacts/` treatment —
same gitignore contract per m090 fixture-stayset guidance + m168
plan.md structure decision.

## Directory layout

```text
artifacts/
├── README.md                # this file — the only tracked entry
├── tauri-src/               # Tauri clone (gitignored)
├── airflow-src/             # Airflow clone (gitignored)
├── tauri/
│   ├── mikebom.cdx.json     # mikebom CycloneDX 1.6 SBOM
│   ├── mikebom.spdx23.json  # mikebom SPDX 2.3 SBOM
│   ├── mikebom.spdx3.json   # mikebom SPDX 3.0.1 SBOM
│   ├── trivy.cdx.json       # Trivy CycloneDX SBOM
│   ├── syft.cdx.json        # Syft CycloneDX SBOM
│   ├── analysis.json        # analyze.py parsed metrics
│   └── *.log                # per-tool invocation logs (wall-clock, warnings)
└── airflow/                 # same shape
```

## Reproducing the artifacts

Run the audit harness (T010's `run-audit.sh`):

```bash
bash specs/168-rust-python-audit/scripts/run-audit.sh
```

Or manually per the Reproduction Appendix in the audit report.
