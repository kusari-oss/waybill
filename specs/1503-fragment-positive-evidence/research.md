# Research: `fragment` requires evidence of a live-bootstrap function

Measurements on moat use the captured x86_64-linux closure from public-corpus run 43535531153 (sha256 `898fcdd3…44ab`, 1,749 derivations). Roles come from `waybill debug closure-dump` at ac2bd28. Derivation attributes the reader does not parse (`builder`, `NIX_MAIN_PROGRAM`, `isExecutable`) were read from the closure document. The probe is `measurements/probe/attrs.py`.

The other four closures are captured by public-corpus run 43629708718, on the branch-only `1503-closure-capture`. R1 and R2 are re-checked on them in R7.

## R1 — The live-bootstrap evidence (FR-001)

**Decision.** A member is `fragment` only if the final path component of its derivation's `builder` is `kaem`.

**Measured on moat.**

| fragment members (240) | builder | `stdenv` attribute |
|---|---|---|
| 215 | `<store>-kaem-1.9.1/bin/kaem` | none |
| 16 | `<store>-bash-static-5.3/bin/bash` | present |
| 9 | `<store>-bash-5.3p9/bin/bash` | present |

**Rationale.**
- `builder` is positive evidence: it names the program that built the member.
- The alternative, "no `stdenv` attribute", is evidence of absence. FR-001 rules that kind of evidence out.
- The alternative also fails on the older bootstrap, which openai and slack-web pin. There `bootstrap-tools` and other raw `derivation` calls carry no `stdenv` attribute, so they would become `fragment`. That repeats the defect.
- The two signals agree on all 240 of moat's members, so moat cannot tell them apart. The older bootstrap can.

**The stage0 seeds.** Across the whole closure, 518 derivations are built by `kaem`. Others are built directly by stage0-posix tools such as `catm`, `hex2`, `M2` and `kaem-unwrapped`.
- None of the latter is among moat's 240 fragments. They carry version-shaped names, so the version rung classifies them first.
- R7 confirms this on all five closures. A stage0-built member reaching the last rung would become `package` under this rule.
- If R7 finds any such member, the rule widens to name those builders explicitly. It does not widen to "anything not bash".

**Alternatives considered.**
- **No `stdenv` attribute.** Rejected: evidence of absence, and it breaks on the older bootstrap (above).
- **Builder is not bash.** Rejected for the same reason: it is negative evidence.
- **Name lists** (`strncpy`, `memcpy`, …). Rejected: open-ended, and the builder already answers the question.

## R2 — The script evidence (FR-004 `scaffolding`)

**Decision (final after R7, with the user's answers recorded in the spec's Clarifications).** A member that reaches the last rung without `kaem` evidence is taken in this order:
1. **`package`** if its `buildCommand` writes `nix-support/setup-hook`. This marks a `makeSetupHook` output, a setup-hook package (rung `setup-hook-package`).
2. **`scaffolding`** if its name ends in `.sh`, `.bash` or `.pl` (rung `script-name`).
3. **`package`** otherwise (rung `default`).

The rest of this section is the moat analysis that produced the name rule. R7 shows why the setup-hook rung must come before it.

For a single-file output, the derivation's `name` is the output's file name. `substituteAll` and `replaceVars` name the output after the template. The suffix is therefore a property of the artifact, not a guess about it.

**Measured on moat**, over the 25 stdenv-built members that leave `fragment`:

| candidate | selects the 10 scripts | selects any of the 15 others |
|---|---|---|
| name ends in `.sh` or `.bash` | 10 | **0** |
| no `NIX_MAIN_PROGRAM` and no `isExecutable` | 10 | 2 (`mirrors-list`, the `glibc-iconv` stage) |
| `dontUnpack` + `src` (substitution shape) | 7 (misses `python-setup-hook.sh` ×3, built by `buildCommand`) | 5 (`nuke-refs` ×5) |
| a consumer references it under `setupHook` | 3 | 0 |
| a consumer references it under `buildCommand` / `postFixup` | 7 | 2 (`nuke-refs` in a `postFixup`) |

Only the name suffix separates the populations on moat.

**What R7 must answer before this is final.** The names below are read from the committed goldens and are not yet measured:
- **slacklinker** has `pythoncheck.sh` ×4 (suffix: `scaffolding`) and `builder.pl` (a Perl script; no `.sh` suffix: `package` under the rule as stated).
- **openai and slack-web** have `makeSetupHook` outputs that declare no `pname`, such as `update-autotools-gnu-config-scripts-hook` ×7.
  - On moat the same hooks declare a `pname`. They are package components and graph endpoints there, not closure members.
  - Under the suffix rule they become `package` on openai as well. That keeps one hook's treatment the same across closures.
  - FR-004's wording ("a hook or helper script that a build sources") reads on them as well. R7 records the count, and the review decides whether that reading or cross-closure consistency governs.

**Alternatives considered.** The four rejected rows in the table, for the counts shown. A consumer-reference rung like spec 1492's `patches` rung was also considered. It needs the closure threaded into `classify`, which spec 1492 did for `patches`. Here it selects only 3 of the 10 scripts, so it is not worth that cost.

## R3 — `package` is the default, and what it means for the projection

**Decision.** A member is `package` if it reaches the last rung with neither `kaem` nor script evidence (FR-004).

**Consequence.** `package` is an edge endpoint (`MemberRole::is_edge_endpoint`), so the projection stops at these members instead of walking through them.
- **Moat:** 15 members, among them `nuke-refs` ×5 and `expand-response-params` ×6.
- **openai and slack-web:** `stdenv-linux` ×2 and the bootstrap stages. Stopping there is what removes the toolchain fan-out (SC-004).

**No code outside the classifier needs to change.**
- `is_edge_endpoint(drv, patches)` calls `classify`, and so do the emitter's `emitted_drv_keys` and the projection's `stops`.
- The new evidence is on the derivation itself, so no new closure-wide set has to be threaded through, unlike spec 1492's `PatchOutputs`.
- The existing invariant test `edge_endpoint_keys_are_a_subset_of_emitted` still holds.

**Expected side effects (as in spec 1492's golden review).**
- **Orphan reason.** A member that becomes `package` stops carrying `closure-member-not-an-edge-endpoint` (`orphan_reason.rs:634`).
- **Coverage population.** It joins the nixpkgs-metadata coverage population, so C184 `identifiers.wanted` and `licenses.wanted` rise by the number of such components.
- **Scaffolding.** A member that becomes `scaffolding` stays a non-endpoint, so nothing changes for it except its role.

## R4 — Identity is unchanged

`member::identity` reads only the content hash and the `.drv` key, never the role. No `bom-ref` or PURL changes. The golden review confirms this: a changed identity is a defect (FR-010).

## R5 — Parse `builder`

`RawDerivation` (`closure/derivation.rs`) does not deserialize `builder` today. It gains `builder: Option<String>` with `#[serde(default)]`.
- `nix derivation show` emits `builder` at the derivation's top level, beside `env`.
- A missing `builder` deserializes to `None`. A member without one has no `kaem` evidence and falls to FR-004.
- No `structuredAttrs` lookup is needed: structured attributes move `env` values, not the derivation's `builder`.

## R6 — Which rung decided, exposed by the binary

**Decision.** `waybill debug closure-dump` rows gain a `member_role_rung` field. Its values are `content-hash`, `patches`, `env-text`, `version-name`, `live-bootstrap-builder`, `script-name` and `default`.

**Rationale.**
- FR-007, and the edge case for scripts without evidence, require listing every member that took the `package` default. That list has to come from the binary, not from a re-model of it.
- `classify` returns its role from an inner function that also reports the rung, so the two cannot disagree.

The rung is debug output only and is not emitted into any SBOM.

## R7 — The four captures (FR-006)

**Captured** by public-corpus run 43629708718 on the branch-only `1503-closure-capture` (commit `99f9e0e4`). The step used the reader's own argv: `derivation show -r --option allow-import-from-derivation <false|true> <copy>#packages.x86_64-linux.default`, with `true` for the three IFD targets.

**Control.** moat's recapture has sha256 `898fcdd3…44ab`, identical to run 43535531153, so the capture method is unchanged.

| target | derivations | sha256 | fragments before | `kaem`-built | bash, `stdenv` | bash, no `stdenv` | other builder |
|---|---|---|---|---|---|---|---|
| moat | 1,749 | `898fcdd3…44ab` | 240 | 215 | 25 | 0 | 0 |
| shellcheck | 2,209 | `e2d8d769…38e7` | 241 | 215 | 26 | 0 | 0 |
| slack-web | 1,293 | `7732d430…c084` | 66 | 0 | 52 | 13 | 1 (busybox: `bootstrap-tools`) |
| openai | 1,448 | `a810dc53…d01e` | 67 | 0 | 53 | 13 | 1 (busybox: `bootstrap-tools`) |
| slacklinker | 2,518 | `ec11ed0a…0772` | 250 | 215 | 35 | 0 | 0 |

**R1 holds.**
- On the three closures that contain live-bootstrap, exactly 215 fragments each are `kaem`-built. On openai and slack-web, none is.
- No last-rung member on any closure has a stage0 seed builder.
- The rejected "no `stdenv` attribute" signal would have kept **14 fragments** on each of openai and slack-web: the 13 `stdenv-linux` and bootstrap-stage derivations, and `bootstrap-tools`. This is the failure R1 predicted.

**R2 needed the setup-hook rung.** Non-`kaem` fragments are split below by a `buildCommand` that writes `nix-support/setup-hook`, and by the `.sh`/`.bash` suffix. "On moat" means a same-named member declares a `pname` on moat, where it is a package component.

| target | setup-hook package, `.sh` name | setup-hook package, other name | `.sh`/`.bash` single file | other |
|---|---|---|---|---|
| moat | 0 | 0 | 10 | 15 |
| shellcheck | 0 | 0 | 11 (adds `utils.sh`) | 15 |
| slack-web | 6 (all on moat) | 30 (all on moat) | 3 (`python-setup-hook.sh`) | 27 |
| openai | 6 (all on moat) | 31 (all on moat) | 3 (`python-setup-hook.sh`) | 27 |
| slacklinker | 0 | 0 | 14 | 21 |

- **Setup-hook packages.** The suffix rule alone would have made 6 setup-hook packages `scaffolding` (`pypa-build-hook.sh` ×2, `python-runtime-deps-check-hook.sh` ×2, `python-imports-check-hook.sh`, `python-namespaces-hook.sh`). It would have made the 30 or 31 others `package`. That splits one kind of artifact by its name. The setup-hook rung makes all of them `package`, which matches moat (user decision).
- **Other script extensions.** The only one observed on any closure is slacklinker's `builder.pl`. `.pl` joins the list (user decision).
- **Every closure, per member:** `measurements/<target>/fragments-before.tsv`.

For each of the five closures, record:
- run ID, derivation count and sha256;
- the fragment count before the change;
- how many fragments have a `kaem` builder;
- how many members on the last rung have a stage0 builder that is neither kaem nor bash (R1);
- how many members the suffix rule selects, and how many members carry a script extension it does not list (R2);
- how many `makeSetupHook`-shaped members end up `package` (R2).

moat's recapture must reproduce sha256 `898fcdd3…44ab`. That is the control showing the capture method is unchanged.
