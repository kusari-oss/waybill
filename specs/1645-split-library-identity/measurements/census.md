# T010: census at the log-only commit (FR-001, SC-002)

## Runs

- **Census**: run 45080107747 of `census-versions.yml` in the TheRock SDK fixture repository, at fixture commit `174360e`. It was dispatched with `--ref main -f waybill_ref=7db3e627064b3cb01876fb099e02e9fee25741c7`, the log-only commit.
  - **Jobs**: 20 of 20 succeeded: `control`, `sdk`, and 18 `corpus` jobs.
  - **Commit check**: all 20 `refs.txt` name waybill `7db3e627`. The SDK tarball is the pinned one, `efeb6b07…`.
- **Public corpus**: run 45080110678, dispatched with `-f branch=1645-split-library-identity`. Its summary reads `Scanned commit: 7db3e627…`, the workflow definition comes from the same commit, and it passed `166 passed; 0 failed`. The log-only commit therefore changes no public-corpus output.

## Positive controls

- **Spec 1644's control** is unchanged: `control ok: [('bzip2', 'versioned', '1.0.8'), ('gmp', 'versioned', '6.3.0'), ('liblzma', 'versioned', '5.8.1')]`.
- **Spec 1645's identity control** (`anchored_control.py OUT IDENTITY`): `identity control ok: [('llvm', 'version-disagrees'), ('ncurses', 'merge')] [('llvm', '23.0.0', '23.0.0git'), ('ncurses', '6.6', '6.6.20251230')] ['pkg:generic/ncurses@6.4.20221231']`.

  Each of the three event kinds fires on the case it is meant to count. A zero in a corpus job therefore means the event is absent, not that the probe is broken.

## Per target

| Target | `shared_file` | `curated_version_change` | `cpe_form_change` |
|---|---|---|---|
| therock-sdk | 3 | 5 | 0 |
| the 18 quality-corpus targets | 0 | 0 | 0 |

TheRock, by file (under `lib/rocm_sysdeps/lib/` for ncurses, `lib/llvm/lib/` for LLVM):

| Event | Library | File | Detail |
|---|---|---|---|
| `shared_file` | ncurses `6.6` | `libncursesw.so` | **merge**: pkg-config `ncursesw@6.6.20251230`, name qualifies, `6.6` a dotted prefix |
| `shared_file` | llvm `23.0.0` | `libLLVM.so` | **version-disagrees**: cmake-package-version `llvm@23.0.0git` |
| `shared_file` | llvm `23.0.0` | `libLLVM.so.23.0git` | **version-disagrees**: the same record |
| `curated_version_change` | llvm | `libLLVM-23git.so`, `libLLVM.so`, `libLLVM.so.23.0git` | `23.0.0` → `23.0.0git` |
| `curated_version_change` | ncurses | `libncursesw.so`, `librocm_sysdeps_ncursesw.so.6` | `6.6` → `6.6.20251230` |

No `ambiguous` or `name-unrelated` verdict, and no curated version that would stop matching (`new: null`).

## Checkpoint

| Requirement | Expected (tasks T010) | Measured |
|---|---|---|
| FR-003 to FR-006 (merge) | TheRock's `libncursesw.so` is `merge` | as expected |
| FR-002 (LLVM suffix) | LLVM changes on TheRock only, 3 files; its 2 shared files `version-disagrees` until FR-002 | as expected |
| FR-013 (ncurses date) | ncurses changes on TheRock only, 2 files | as expected |
| FR-008 (CPE form) | `nix-closure-openai` only; none on TheRock | none on TheRock or the quality corpus, as expected. **`nix-closure-openai` not observed here**: the public-corpus harness does not enable `waybill::version_census`, so its run carries no census events. That component's change is measured instead by T017's golden diff, which is exact. |

No count contradicts FR-002 to FR-013, so the spec is not amended. After wiring, the expected TheRock changes are:
- ncurses becomes one `ncurses@6.6.20251230`, with the `ncursesw` component merged into it;
- the two LLVM components become one `llvm@23.0.0git`.

This is a prediction for T018 to check.
