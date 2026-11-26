# Tasks: Keep test output paths reserved (#1727)

**Input**: `specs/1660-held-temp-paths/` (spec, plan, research, contracts/check.md, data-model, quickstart).

**Rules (CLAUDE.md):**
- Never pipe a check through `head`.
- Revert mutations with `cp` and check with `cmp`.
- Run Python as `python3 -I -B`.
- Don't run `cargo fmt`.
- zsh doesn't word-split, so use bash.

## Phase 1: Setup

- [x] T001 Create `specs/1660-held-temp-paths/measurements/`.

## Phase 2: User Story 1, measure the mechanism before any change (P1)

**Goal**: know whether concurrent processes are ever handed the same temporary-file name on this host.

**Independent test**: the control run shows duplicates; the real runs report their counts, including 0.

- [x] T002 [US1] Create the standalone crate `specs/1660-held-temp-paths/measurements/collide/`:
  - **`Cargo.toml`:** package `collide`, an empty `[workspace]` table, and dependencies `tempfile = "=3.27.0"` and `fastrand = "=2.4.1"` (the lockfile's versions; check with `grep -A1 '^name = "tempfile"' Cargo.lock`). Use its own `target` directory by building with `--manifest-path`.
  - **`src/main.rs`:**
    - with no argument, create `tempfile::NamedTempFile::new()`, print `.path()`, then drop it (the tests' shape);
    - with `--fixed-seed`, print a name built from `fastrand::Rng::with_seed(42)` the way `tempfile` builds one (`.tmp` + 6 alphanumerics), as the control.
- [x] T003 [US1] Write `specs/1660-held-temp-paths/measurements/collide.sh` (bash):
  - build the crate in release mode;
  - for each mode (`real`, `fixed-seed`) and each N in 18, 36 and 72, run 50 batches, each starting N processes at once (`&` then `wait`), collecting stdout per batch;
  - count duplicates per batch with `sort | uniq -d | wc -l`, and sum them;
  - write `mode`, `N`, `batches`, `names`, `duplicates` to `collide-results.tsv`.
- [x] T004 [US1] Run `collide.sh` 3 times, appending a `run` column, and write `measurements/collide.md`:
  - **The host:** model and core count.
  - **The control result,** which must be greater than 0, or the harness is broken.
  - **The real results:** if every real count is 0, say plainly that the collision did not reproduce on this host, and that the fix rests on the code facts.
  - **Scope:** state that the harness tests the naming mechanism under concurrent process starts, not nextest's own launcher. A count of 0 here says nothing about whether nextest could still collide.
  - **Cleanup:** remove `collide/target` afterwards.

**Checkpoint**: the mechanism is measured before any code changes.

## Phase 3: User Story 3, the check, with its positive control on the unfixed tree (P2, done first because it finds the sites)

**Goal**: the dropped shape is rejected locally and in CI.

**Independent test**: `--self-test` passes, and the check lists all 23 sites on the unfixed tree.

- [ ] T005 [US3] Write `scripts/check-held-temp-paths.py` per `contracts/check.md`:
  - **Scan:** every `*.rs` file in every workspace member (`waybill-cli/`, `waybill-common/`, `waybill-ebpf/`, `xtask/`), excluding `target/`.
  - **Constructors:** `NamedTempFile::new(`, `NamedTempFile::new_in(`, `Builder::new()` followed by `.tempfile(`/`.tempdir(`, `tempfile::tempdir(`, bare `tempdir(` and `TempDir::new(`/`TempDir::new_in(`.
  - **The statement:** from the constructor's start to the first `;` at bracket depth 0, skipping string literals.
  - **The chain:** parse the constructor's own parenthesised arguments, then look at what is chained onto it. The statement is flagged when that chain reaches `.path()` and the statement isn't of the form `let <ident> = <ctor>…;` with nothing after `.unwrap()`/`.expect(...)`/`?`.
  - **Allowed:** `.keep()`, `.persist(`, and a `// held-temp-path: <reason>` marker on the preceding line.
  - **`--self-test`:** built-in must-flag snippets (the tests' exact shape, a `TempDir::new().unwrap().path().join("x")` variant and a `Builder` variant) and must-pass snippets (bound and then used, `new_in(tmp.path())` bound, `.keep()`, the marker).
- [ ] T006 [US3] Positive control. On the unfixed tree, run the check and save its output to `measurements/check-before.txt`. It must exit 1 and list exactly the 23 sites in research R1. Any difference is investigated and explained in `check-controls.md` before going on. Also record the `--self-test` output there.

## Phase 4: User Story 2, every output path stays reserved (P1)

**Goal**: no site drops its temporary guard before the scan output has been read.

**Independent test**: the check exits 0, and the suite passes 3 times under nextest.

- [ ] T007 [US2] Review all 23 sites, plus anything T006 found, by hand. Write `measurements/sites.md` with a table: file, line, helper, shape, verdict (`dropped` or `held`), reason, and the output-name extension, if the helper or scan cares about it (check whether `--format` or the file name decides the format).
- [ ] T008 [US2] Fix every `dropped` site per research R4:

  ```rust
  let out_dir = tempfile::tempdir().expect("tempdir");
  let out = out_dir.path().join("out.<ext>");
  ```

  `out_dir` must be bound and live until after the output is read. Keep the original extension where `sites.md` says it matters, and use `out.json` otherwise. Don't change any assertion. Edit each file with an exact replacement and check the diff per file.
- [ ] T009 [US2] Run the check: it must exit 0. Then run `cargo clippy --workspace --all-targets -- -D warnings`, because unused imports such as `NamedTempFile` will need removing, and fix whatever it names.
- [ ] T010 [US2] Run every touched binary once with `cargo nextest run -p waybill --test <bin>` for each file, and confirm the pass counts are unchanged from `main`. Get `main`'s counts from `cargo nextest list` before and after, by name.

## Phase 5: Wire the check in (US3, continued)

- [ ] T011 [US3] Add `python3 -I -B scripts/check-held-temp-paths.py` to `scripts/pre-pr.sh`'s steps, after `check-walker-audit.sh`, and extend the comment above the steps that explains why the fast checks run first. Add a CI step beside the walker-audit step in `.github/workflows/ci.yml` (Linux lint job), named `Held temp-path check`, running the same command. Then run `--self-test` and the check from the repository root once more.

## Phase 6: Verify and ship

- [ ] T012 Run the full suite 3 times: `cargo nextest run --workspace --no-fail-fast --color never`. Record each summary line in `measurements/after.md`.
- [ ] T013 Run the gate: `./scripts/gate.sh` with both `--expect` patterns from CLAUDE.md, around `prepr -- ./scripts/pre-pr.sh`. Check `GATE_RESULT=pass`, the summary line, and that the test count is unchanged (9,299), and that the new check's step ran.
- [ ] T014 Write `measurements/after.md`, commit, and open the PR (Refs #1719, #1727). Check every CI lane once, enumerating every job's conclusion. After merge, close #1727 and tick it in #1719.

## Dependencies

- **Order:** T001 → US1 (T002 → T003 → T004) → US3 check (T005 → T006) → US2 (T007 → T008 → T009 → T010) → T011 → T012 → T013 → T014.
- **US1 must finish before any code change** (FR-001).
- **T006 runs on the unfixed tree,** so it comes before T008.

## Implementation strategy

Measure first, then write the check, since it doubles as the site finder. Then fix, then enforce. One gate, one PR.
