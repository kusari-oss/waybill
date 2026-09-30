# `requirements_txt_only`

Regression fixture for #1045.

A Python project whose only manifest is `requirements.txt`, so no reader
emits a root main-module. That is the shape that triggers the FR-008
"zero root-level manifests" fallback under `--project-discovery=root-only`.

The fallback claimed in its WARN to be emitting full scope while actually
emitting the already-filtered slices, so every dependency was dropped and
the SBOM carried only the file-tier `requirements.txt` entry.

Package names are synthetic (`waybill-fixture-*`) per the repository rule:
real coordinates in a fixture trip advisory scanning. The versions are the
ones from the issue's repro, kept so the shape matches what was reported.
