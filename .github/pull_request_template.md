## Summary

<!-- 1-3 sentences explaining what this PR does and why. -->

## Test plan

<!-- Bulleted list of the verification steps you ran locally. -->

- [ ]
- [ ]

## Pre-PR checklist

- [ ] I ran `./scripts/pre-pr.sh` and it exited clean (zero clippy warnings, all test suites `0 failed`).
- [ ] For non-trivial changes, I followed the speckit lifecycle (`specs/<NNN>-<short-name>/` exists with spec/plan/tasks); for drive-by fixes, this is a single-purpose change.
- [ ] If I touched SBOM emission or output formats, I regenerated the affected byte-identity goldens (`WAYBILL_UPDATE_{CDX,SPDX,SPDX3}_GOLDENS=1 cargo +stable test ...`).
- [ ] If I added a new `waybill:*` property / annotation / relationship type, I audited each target format for an existing native construct first (Constitution Principle V — see [`.specify/memory/constitution.md`](../.specify/memory/constitution.md)).
- [ ] If this is a release-bump PR, I ran the SPDX-3 conformance gate locally: `WAYBILL_REQUIRE_SPDX3_VALIDATOR=1 ./scripts/pre-pr.sh`.
- [ ] If this PR touches the scan pipeline, output dispatch, or per-format emission, I added the `perf` label to trigger the dedicated perf benchmarking lane ([`.github/workflows/perf.yml`](../.github/workflows/perf.yml)).

🤖 If this PR was AI-assisted, include the Co-Authored-By trailer in the commit message.
