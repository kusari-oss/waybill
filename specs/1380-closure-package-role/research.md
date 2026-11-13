# Research: spec 1380 (#1374)

Sources:
- `measurements/package-role-census-2026-11-13.txt`: the five closure goldens.
- `measurements/member-evidence-probe-2026-11-13.txt`: nixpkgs `a799d3e3` (moat's pin) and `5b2c2d84` (slacklinker's pin), `x86_64-linux`, evaluated without building. No corpus flake was evaluated.

## R1. No derivation evidence separates a check from a genuine package

**Finding (measured).** The checks come from two builder families, and each matches a genuine package exactly:
- **`kaem`-built checks** (`mes-get-version-0.27.1`, `tinycc-boot-mes-get-version-…`) have `env` = `{PATH}`. So do `mes-libc-0.27.1` and `tinycc-boot0-libs-…`.
- **`bash`-built checks** (`bash-get-version-5.3`, `gcc-simple-program-15.2.0`) are `runCommand`: `{PATH, SHELL, buildCommand, passAsFile}`. `glibc-iconv-2.42` also has a `buildCommand`, and it is a package.

Nothing in the derivation marks a `passthru.tests` output.

**Decision.** `check` is assigned by name: a `-get-version-` or `-simple-program-` segment, followed by a version (`[0-9]…`) or `unstable-…`. The clarified scope (Q2) is these two groups only.

**Alternatives.**
- **Inspect `buildCommand` text** for `--version`: that is fragile, and the `kaem` checks have no `buildCommand`.
- **Detect `passthru.tests`:** not recorded in a derivation.

## R2. E1: a member listed in another derivation's `patches` is a patch

**Finding (measured).** Across the three probed closures, the non-fixed-output members whose output another derivation lists in `patches` are:
- `001-fix-rpath.patch` ×3
- `catalog-legacy-uris.patch` ×6
- `clang-at-least-16-LLVMgold-path.patch`
- `fix-build-time-run-tcl.patch` ×3
- `remove-hatch-plugins.patch` ×2

No genuine package is listed.

**Decision.** E1 is evidence for `source`. The patch set is built once per closure, from `paths_in("patches")` (both carriers), keyed by store basename.

## R3. E2: nixpkgs `srcOnly` is a source tree

**Finding (measured).** `phases` = `unpackPhase patchPhase installPhase` identifies `ghc-9.10.3-source`, and nothing else in the probed closures.

**Decision.** E2 is evidence for `source`.

## R4. The source-tree name rule, and false positives

**Finding (measured).** Some source trees have no evidence:
- `clang-src-21.1.8`, `llvm-src-…` and `llvm-tblgen-src-…` are `runCommand` copies out of a fetched monorepo;
- `cargo-auditable-0.7.2-vendor` is a `fetch-cargo-vendor-util` stage;
- `mes-src-0.27.1` and `tinycc-*-source` are `kaem` builds.

Over every non-fixed-output, non-`text` derivation in the probed closures, the name rules match only:
- **Source-tree rule:** the 9 source trees.
- **Check rule:** 54 check derivations.

Every census misfit is caught, and no census genuine package is.

**Decision.** A name ending `-source`, `-src-<v>` or `-vendor` is `source`, where no evidence applies.

## R5. The new rules refine the `package` rung only (FR-006)

**Finding (measured).** Four locally built patches have no `-<digit>` token, so they are `fragment` today, a role the graph walks through. On moat:
- `001-fix-rpath.patch`
- `catalog-legacy-uris.patch` ×2
- `fix-build-time-run-tcl.patch`
- `remove-hatch-plugins.patch`

Applied to every member, E1 would move them to `source`, an endpoint, and add edges, breaking FR-006.

**Decision.** E1, E2 and the name rules are consulted only for a member the version-token rung already claims. They split today's `package` population into `source`, `check` and `package`.
- Every move is between endpoint roles, so the graph is unchanged by construction.
- A member outside today's `package` population is untouched.
- The four fragment patches are a separate defect (fragment means "live-bootstrap C function"). It is filed as a follow-up and out of scope here.

**Alternative.** Fix all patches now, and accept the edge change. Rejected: the spec fixes the edges, and a graph change deserves its own measured review.

## R6. Rung order inside the refined `package` rung

**Decision.** In order:
1. E1;
2. E2;
3. the source-name rule;
4. the check-name rule;
5. otherwise `package`.

Evidence comes before names (FR-005). No probed name matches both name rules, so their relative order is not load-bearing; a unit test pins it.

## R7. Wire and counts

**Decision.**
- **Wire value:** `MemberRole::Check`, wire `check`, `is_edge_endpoint() == true`, appended to `ALL`.
- **C184:** `member-roles` gains a `check` key on every closure document, because counts are seeded from `ALL` with zeros (`summary.rs`).
- **C206:** stays `SymmetricEqual`. It is the same annotation, with one more value.
- **Docs:** the catalog row and `nix-evaluation.md` list five values, and say which rules are evidence and which are names.

## Expected golden change (prediction, to verify at regeneration)

| target | `package` | `check` | `source` gains | edges |
|---|---|---|---|---|
| moat | 73 → 23 | 46 | +4 | unchanged |
| shellcheck | 73 → 23 | 46 | +4 | unchanged |
| slacklinker | 93 → 34 | 47 | +12 | unchanged |
| openai | 4 → 3 | 0 | +1 | unchanged |
| slack-web | 4 → 3 | 0 | +1 | unchanged |

C184's `member-roles` changes to match on all five. Every other target is byte-identical.
