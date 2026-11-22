# T005: census at the log-only commit (SC-001)

## Runs

- **Census:** run 44990809831 of `census-versions.yml` in the TheRock SDK fixture repository (commit `26d82dd`), dispatched `--ref main -f waybill_ref=d3dcd676961b92e2c7f6d0a008ea17cc5459c2bf`.
  - **All 19 jobs succeeded:** `sdk` and 18 `corpus` jobs.
  - **Every job's `refs.txt` names waybill `d3dcd676`.**
  - **SDK tarball:** sha256 `efeb6b07211030d0dea0f89e15017fe269f80c8811e22b61799bfb32b0c8bc5a`, which matches the fixture's `MANIFEST.json`.
- **Corpus trees.** 9 of the 18 report a commit other than their `quality-corpus.toml` pin: cmake-nlohmann-json, cpp-mongo, go-cobra, go-kubernetes, gradle-apache-solr, npm-express, python-flask, ruby-rails and rust-ripgrep. Each of those pins is an annotated tag object, and the reported commit is that tag's target. This was checked for all 9 with `gh api repos/<repo>/git/tags/<pin>`; for example ripgrep's `0e8390a6` is tag `14.1.1`, pointing at `4649aa97`. They are the pinned trees.
- **Public corpus, first attempt.** Run 44990811702 was dispatched from the branch without `-f branch=`, and its summary says `Scanned commit: 83657af2`, which is `main`. Its 66 of 66 identical goldens therefore say nothing about this branch. Run 44992685827 re-dispatches with `-f branch=1643-version-string-boundary` (see below).
- **Positive controls:**
  - C-1 and C-2 pass at the log-only commit (`control.md`);
  - in the CI matrix, `pants-backend-ai` reports 4 events with the same job template as the 16 targets reporting 0. The zeros therefore mean no curated banner occurs in those source trees, not that the census did not run.

`census/versions.py` reads only the events. It strips ANSI codes, because the workflow does not set `NO_COLOR`.

## Results

| target | events | binaries | verdicts that change | gain or change a version | lose | newly two versions |
|---|---|---|---|---|---|---|
| therock-sdk | 16 | 13 | 8 | 8 | 0 | 0 |
| pants-backend-ai | 4 | 4 | 0 | 0 | 0 | 0 |
| the other 17 | 0 | 0 | 0 | 0 | 0 | 0 |

The 17 targets with no events: cmake-nlohmann-json, cpp-mongo, go-cobra, go-kubernetes, gradle-apache-solr, gradle-bitwarden-android, maven-guice, npm-express, pnpm-vue-core, python-ansible, python-flask, python-pytorch, ruby-rails, rust-ripgrep, rust-zizmor, uv-meilisearch-python and yarn-guac-visualizer.

### By preceding byte

| target | library | start | nul | whitespace | other non-identifier | identifier |
|---|---|---|---|---|---|---|
| therock-sdk | expat | 0 | 2 | 0 | 0 | 0 |
| therock-sdk | llvm | 0 | 3 | 3 | 0 | 0 |
| therock-sdk | ncurses | 0 | 0 | 0 | 2 | 0 |
| therock-sdk | zlib | 0 | 0 | 0 | 6 | 0 |
| pants-backend-ai | openssl | 0 | 2 | 0 | 0 | 0 |
| pants-backend-ai | zlib | 0 | 2 | 0 | 0 | 0 |

### The 8 new matches, judged (TheRock SDK)

| binary (under the SDK root) | match | byte before | judgement |
|---|---|---|---|
| `lib/rocm_sysdeps/lib/libz.so` | `zlib@1.3.2` | `-` (`0x2d`) after binary data | own banner: ` deflate 1.3.2 Copyright 1995-2026 Jean-loup Gailly and Mark Adl…` |
| `lib/rocm_sysdeps/lib/librocm_sysdeps_z.so.1` | same | same | same |
| `lib/rocm_sysdeps/lib/librocm_sysdeps_z.so.1.3.2` | same | same | same |
| `lib/rdc/grpc/lib/rocm_sysdeps/lib/libz.so` | same | same | same |
| `lib/rdc/grpc/lib/rocm_sysdeps/lib/librocm_sysdeps_z.so.1` | same | same | same |
| `lib/rdc/grpc/lib/rocm_sysdeps/lib/librocm_sysdeps_z.so.1.3.2` | same | same | same |
| `lib/rocm_sysdeps/lib/libncursesw.so` | `ncurses@6.6` | `0xff`, after `…0123456789\0\0\xff\xff\xfe` | own banner: `ncurses 6.6.20251230\0`, a whole string after a constant table |
| `lib/rocm_sysdeps/lib/librocm_sysdeps_ncursesw.so.6` | same | same | same |

This is exactly #1641's layout (zlib `- deflate 1.3.2 …`, ncurses after `0xff`) on the pinned ROCm 7.14 SDK. Each zlib file is a separate copy or name for the same library: the scan reads each path, as it does on `main`.

**No false positive.** Every new match is the library's own banner, and none is a mention inside other text.

### Whitespace

No anchored pattern is preceded by whitespace in any of the 19 targets. The only whitespace-preceded hits are 3 LLVM ones, `AMD LLVM version 23.0.0git (52226beb…`, in `libLLVM.so` and its two other names. That is a version inside a longer string, which is exactly what the NUL rule is for. LLVM is NUL-only, so these stay rejected under both rules, and the same binaries already carry a NUL-preceded `LLVM version 23.0.0git`.

## Decisions the census settles

- **FR-005: no change.** No anchored pattern produced a false positive, so zlib, expat and ncurses stay anchored.
- **No binary newly carries two versions** of one library, and none loses a version.
- **The two-space zlib case.** The existing #1525 test expects `\0  deflate 1.3.2 Copyright` to be rejected, but FR-002 accepts a space before zlib's leading space. No target has a whitespace-preceded zlib banner, so there is no evidence of harm, and the ` Copyright` suffix still fixes the match's end. The test's expectation is updated to FR-002's at T007, with this reason. `\0see deflate …`, an identifier before the space, stays rejected.
- **What T018 must trace.** 8 SDK binaries gain a curated version match. In the log-only document (`scan-sdk.cdx.json` of this run, the same as `main`'s output), these libraries are already partly identified by spec 1531's installed-library reader:
  - **zlib:** the 6 files are carried by an unversioned `pkg:generic/zlib` (`waybill:evidence-kind = symbol-fingerprint`). `pkg:generic/zlib@1.3.2` already exists, from `zlib.pc` and the CMake package configs.
  - **ncurses:** `libncursesw.so` is named by `pkg:generic/ncursesw@6.6.20251230`, from `ncursesw.pc`. The curated slug is `ncurses` and its version is `6.6`, so the embedded match is not the same PURL.

  How the embedded matches combine with these records is measured at T018, not predicted here. A second, differently named ncurses component on the same file would be a finding for T018. No quality-corpus document should change, since no corpus binary changes verdict.
