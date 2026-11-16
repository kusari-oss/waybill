# T019: corpus golden review (SC-002)

**Run:** `public-corpus.yml`, run 43890487022, `regen_goldens=true`, dispatched with `--ref 1536-python-native-binaries -f branch=1536-python-native-binaries`.
- Scanned commit: `a052a6212dc3852de23202df405b2ee724658e8a`. That is this branch, and it includes every code change (`29c34734`, `c690b8d8`).
- Workflow definition from `c690b8d8`.
- Every step passed, including "Assert the corpus actually ran" and "Assert every target regenerated its goldens".

**Result: 0 changed files.** The workflow's summary reports "None — every regenerated golden is byte-identical to the committed one". It was checked independently: the downloaded `corpus-goldens-regen` artifact holds 67 files, and each was compared with `cmp` against `waybill-cli/tests/fixtures/public_corpus/`.

| file | count | identical |
|---|---|---|
| `cdx.json` | 21 | 21 |
| `spdx-2.3.json` | 21 | 21 |
| `spdx-3.json` | 21 | 21 |
| `recall-truth.json` | 3 | 3 |
| `README.md` | 1 | 1 |
| **total** | **67** | **67** (0 differ, 0 missing) |

The committed tree also holds 67 files, so nothing exists on one side only. With no changed file, there is no normalised diff to read: byte-identity is the stronger result.

**SC-002 holds, and research R9's prediction is confirmed.**
- No corpus target holds a `.dist-info` whose `RECORD` lists a native file that no OS database claims.
- In `image-postgres16`, the Debian image, the Python packages' files are dpkg-owned. Under FR-005 they are unchanged, and the golden is byte-identical.
- No OS-package component changed in any target.

The corpus therefore does not exercise the new path. That path is covered by:
- the 22 `python_owned_binaries` tests;
- the mutations M1 to M13;
- the real wheels (T021, `wheels.md`);
- the ROCm SDK fixture (T020, `rocm-sdk.md`).
