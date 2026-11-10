# Research: Nix closure scope from the runtime closure

Code citations come from a read-only map of `be8924f7` taken on 2026-11-10. Measurements of external behaviour are in `measurements/`; each one names its probe.

## R1 — Where the new pass runs

**Decision:** Extend `resolve/closure_role_scope.rs::assign_role_scope`, called at `scan_cmd.rs:5111-5114`, so it also takes `Option<&ClassifiedClosure>`, the scan's `relationships`, and a new `RuntimeEvidence` value. Do not add a separate pass.

**Rationale:**
- The pass already owns C203 and the two exemptions (FR-006).
- It runs after the closure merge, identity, contents and edges steps (`scan_cmd.rs:4715-4877`) and before `NixClosureSummary::build` (:5129). That means every input it needs exists by then, and the summary can still report the method.
- Two passes writing `artifact_presence` would have to agree on precedence. One pass with one decision function cannot disagree with itself.

**Alternatives considered:**
- A new pass after the existing one. Rejected: it would overwrite marks the first pass made, and C203 would need a merge rule.
- Deciding presence in `emit.rs`. Rejected: the contents and identity steps run later and can rewrite PURLs.

## R2 — Obtaining the runtime closure

**Decision (revised during US1, `measurements/nix-behaviour/q9-*`):** query the evaluated derivation's **default output path** (the head of its `outputs`), not the installable. Passing `<flakeref>#<attr>` failed three ways: with `--store <cache>` nix tries to upload the `.drv` and gets a 404; curl's installable does not evaluate at `a799d3e3`; and an installable selects `meta.outputsToInstall` (shellcheck: bin, man, doc), not `outPath`, which the committed truth used. The default output matched the evaluated `outPath` on 4 of 4 packages and needs no second evaluation. Untested inference: an attribute that explicitly selects a non-default output (`pkgs.curl.out`) is not seen this way. Calls go through `eval/invoke.rs::run_bounded` with the tier's budget and `guard()`.

*Original plan, superseded:* run `nix path-info -r <flakeref>#<attr>`.

- **Order of stores:** the local store first. If the output isn't local, query the user's configured substituters, read via `nix config show` (already invoked by the tier), in order, one `--store <url>` call per substituter, and take the first complete answer.
- **Which output:** pass the installable, not an output path. The closure does not record which output the attribute selects (`outputName`): shellcheck's and curl's `outPath` is their `bin` output (`measurements/nix-behaviour/outpaths.tsv`). Passing the installable lets nix pick the same output a user's `nix build` would.

**Measured** (`measurements/nix-behaviour/`, Determinate Nix 3.20.0 / nix 2.34.6, aarch64-darwin):

- **Without `--store`:** path-info consults substituters, then fails with `path … is not valid` (exit 1). Every remote query therefore needs an explicit `--store <url>`, one call per substituter. Two `--store` flags means the last one wins.
- **Reading the configuration:** `nix config show substituters` prints a single space-separated line, with `extra-*` already merged in. The tier already runs `config show`, so this adds one argument.
- **Miss:** exit 1, empty stdout, stderr `don't know how to build these paths` then `path … is not valid`. Treat it as "not here; try the next store".
- **Signatures:** path-info does not verify them; a bogus trusted key still gives exit 0. `nix store verify --no-contents -r --store <url> <installable>` does (exit 2, `untrusted`). **Decision:** after a remote hit, verify, and treat a failure as a miss. The verification cost is unmeasured; see R11.
- **Timing:**
  - warm narinfo cache: 0.05–0.09 s;
  - cold: shellcheck 0.27 s, curl 0.76 s, hlint (322 paths) 11.0 s median, about 34 ms per path and apparently serial;
  - local store: 0.08–0.14 s.
- **Unreachable cache:** nix's defaults take 79.3 s with a blackholed proxy. `--option connect-timeout 2 --option download-attempts 1` cuts that to 2.1 s. **Decision:** always pass both on remote queries.
- **Several unreachable caches (T041):** tried in turn, three would cost ~6.3 s (inference: 3 × the measured 2.1 s). **Decision:** a cumulative 5 s remote budget (`runtime::REMOTE_BUDGET`). Once the substituter calls have spent it, no further substituter is started and the reason is `query-timed-out` (unless an earlier `verification-failed` outranks it). A call already running is not cut short, because a reachable cache answering a cold query is slow by nature (hlint 11 s) and cutting it would trade a correct answer for the walk. The worst case is therefore 5 s plus one bounded call, not a strict 5 s.
- **Offline:** nix's own `--offline` still tries an explicit `--store` once. Moot here, because the closure tier doesn't run under waybill's `--offline` (R7).
- **Output format:** `--json` shape varies by version, and help text says `--json-format` will become required. **Decision:** use plain `-r` output, one path per line, which is stable. Signatures come from the separate verify step, not from path-info's JSON.
- **Multi-output:** `curl.outPath` is `bin`, and path-info on one output gives only that output's closure (bin 20 paths, all five outputs 48). Passing the installable gives the attribute's default output, which is what `nix build` produces.

**Rationale:** this goes only to stores the user already trusts (FR-004), uses nix's own output selection, and builds nothing.

**Alternatives considered:**
- `nix eval .outPath` followed by `path-info <path>`. Rejected: an extra evaluation, which the installable form does anyway.
- Hard-coding cache.nixos.org. Rejected by the 2026-11-10 clarification.

## R3 — Joining store paths to components

**Decision:** For each runtime store path, take `store_basename`, then `RawClosure::output_index()` (`derivation.rs:456`, output basename to `.drv` key), then that derivation's `MemberKey` (`contents.rs:298-324`: `Named(pname, version)` or `Hashed(purl)`), then the component. The same join already serves `contents::read` and the license join, which is keyed by store path.

- **Unmatched paths:** count them for FR-007. Examples are darwin's `libSystem`, `apple-sdk`, and members `emits_as_component` drops.
- **Name-and-version fallback** (FR-003): only for a runtime path absent from `output_index`. That should not occur, because `derivation show -r` covers every output of every closure member. Count it if it does.

**Rationale:** components do not carry their `.drv` (`ResolvedComponent` has no drv field), but every existing closure join goes through `output_index`, so this introduces no new identity.

## R4 — Statically linked members (FR-002)

**Go and Rust.** `contents::read` already records host → module edges, emitted as `DependsOn` with `provenance { source: "nix-closure-contents", data_type: "embedded-build-info" }` (`contents.rs:266-288`, pushed at `scan_cmd.rs:4840`). A module is runtime when any of its hosts is runtime.

Today the contents pass decides module presence from the host's *role* (`contents.rs:557`) and stamps C203 `closure-role` without the role-scope pass ever running on the module. The new pass takes that decision over.

**Haskell (measured; `measurements/nix-behaviour/q6-*`).**

How shellcheck's derivations are wired:
- The top-level `shellcheck` derivation (`justStaticExecutables`) has empty `buildInputs`.
- It reaches the static `ShellCheck` build through `inputs.drvs` and an `installPhase` path, and that build *is* in the runtime closure.
- The linked libraries sit in that build's `buildInputs`. Haskell libraries carry their own dependencies in `propagatedBuildInputs`.

Two sources of edges:

1. **Preferred:** milestone 1275's `nix-closure-declared` edges, built from `libraryHaskellDepends + executableHaskellDepends`, with no test or benchmark dependencies (`haskell_packages/closure_identity.rs:195-205, 728-772`). They are present when the pinned package set is cached. This is equivalent to `getCabalDeps`, which measured exact on ormolu (29/29).
2. **Fallback, from derivation fields:** at each runtime member built with the Haskell generic builder, take `buildInputs + propagatedBuildInputs`, then follow `propagatedBuildInputs` transitively through Haskell derivations, matching by output path.

Scores for the fallback (cabal metadata as truth; it also finds every package whose code is in the binary, read from symbol tables):

| target | rule finds | cabal metadata | code in binary | all binary packages found? |
|---|---|---|---|---|
| shellcheck | 50 | 50 | 28 | yes |
| pandoc | 220 | 218 | 177 | yes |
| ormolu | 75 | 29 | 22 | yes |

- pandoc's two extra libraries: aeson-pretty propagates its executable-only dependencies.
- ormolu's 46 extra libraries: `doCheck=1` puts test-only dependencies in the same `buildInputs`.

**Decision:** source (1) when available, source (2) otherwise. Source (2) over-includes test dependencies, which is the safe direction: linked code is never dropped. C216 records which source was used.

Dynamically linked Haskell executables (hlint, hadolint) need neither source, because every library they declare is already in the runtime closure.

## R5 — Recording the method (FR-007)

**Native-construct audit (Principle V):**

| Format | Candidate | Carries "how were scope marks decided"? |
|---|---|---|
| CycloneDX 1.6 | `metadata.lifecycles` | No. It names the SBOM's lifecycle phase, not a classification method. |
| CycloneDX 1.6 | `compositions[].aggregate` | No. It states completeness of a set, not how scope was assigned. |
| CycloneDX 1.6 | per-component `evidence` | Per component, not document scope. C203 already covers per-component. |
| SPDX 2.3 | document `annotations` | Generic container. This is what waybill uses for every `waybill:*` document row. |
| SPDX 3.0.1 | `Annotation` on the SpdxDocument | Same as SPDX 2.3. |

No native field carries it. **Decision:** add the method to `NixClosureSummary` as a new object, following the C204 `nix_closure_read_wire` pattern (`summary.rs:284`; emitted at `cyclonedx/metadata.rs:1148-1153`, `spdx/annotations.rs:1084`, `spdx/v3_annotations.rs:1029`). It becomes catalog row **C216** `waybill:closure-scope-method`: a document-scope JSON object with a parity extractor in all three formats.

**Rationale:** the summary is built after the scope pass (`scan_cmd.rs:5129`), so this needs no new `ScanArtifacts` plumbing.

**Alternative considered:** extending C204 `nix-closure-read`. Rejected: C204 records what the reader *saw*, and the scope method is a later decision. Mixing them would make C204's meaning depend on the pass order.

## R6 — Per-component method (FR-008)

**Decision:** `ArtifactPresence` (`waybill-common/src/resolution.rs:658-686`) is the source of C203's exempt values. C203 itself comes from `closure_role_scope::classification_source` (:194-215). Add a `ScopeEvidence` enum, carried beside `artifact_presence`, with variants:
- `RuntimeClosure`
- `StaticallyLinked`
- `ReachabilityWalk`
- `ClosureRole` (today's behaviour, kept for `method = none`)

C203 gains `runtime-closure`, `statically-linked` and `reachability-walk`. Both `ArtifactPresence::as_str` and `ScopeEvidence` are matched exhaustively (the #892 lesson).

**I10** (`tests/corpus_harness_195/invariants/checks/closure_role_scope.rs`): its population is "role is `build-tooling` or `unreferenced`". Under runtime truth, a `build-tooling` member inside the closure (openssh's `dns-root-data`) is legitimately unmarked. I10 must accept C203 `runtime-closure` / `statically-linked` as evidence that the member was decided, and add the converse check: a member C203 says is runtime must not carry `scope: excluded`.

**Unmarked members:** runtime members also get a C203 value. Today an unmarked `artifact-input` member carries none. Without one, "decided runtime" and "never reached" cannot be told apart, which is the same reason `ExemptCompilerSupplied` exists.

## R7 — `--offline`

`closure::admission(nix_closure, offline)` (`closure/mod.rs:351-360`) refuses the whole closure tier under `--offline` and emits C190 `offline-requested`. No closure means no marks, so FR-014's "`--offline` suppresses the substituter query" holds trivially.

**Decision:** keep admission as it is. Correct the spec edge case that said "the local store is still used" under `--offline`: there is no closure at all. Changing admission is out of scope.

## R8 — `sbom edit` selection (FR-010)

Today:
- **CDX** reads `scope: excluded` with no lifecycle property as `Scope::Development` (`edit/cdx.rs:131-138`).
- **SPDX 2.3** ignores `BUILD_TOOL_OF` (`edit/spdx23.rs`).
- **SPDX 3** ignores `usesTool` + `scope: build` (`edit/spdx3.rs:106-132`).
- #1428 doesn't touch scope reading.

**Decision:**
- Add a selector value **`scope=excluded`**, parsed as a new `Scope::NotInArtifact`. It matches CDX `scope: excluded`, SPDX 2.3 `BUILD_TOOL_OF` (source member), and SPDX 3 `usesTool` with `scope: build` (the used member).
- Remove the CDX fallback that read `excluded` as `Development`. Real lifecycle scopes are unaffected, because they arrive through `waybill:lifecycle-scope` / `optional-derivation` first.
- The FR-011 recipe is `waybill sbom edit <doc> --drop 'scope=excluded'`.

**Alternative considered:** a separate `presence=` key. Rejected: `scope=excluded` is CycloneDX's own word for the concept, and the SPDX mappings are one-to-one.

**Compatibility:** a Go `build_inclusion: NotNeeded` component also emits `scope: excluded` (`builder.rs:1528-1532`). It will now match `scope=excluded` instead of `scope=development`. That is a behaviour change for Go documents, and it must be called out in the CHANGELOG and checked against SC-006.

## R9 — Stub `nix` in existing tests

Every stub (`tests/common/nix_stub.rs:86-101` and 13 private copies) ends in `*) … exit 1`. A new `path-info` call would therefore fail in all of them and send them down the walk path, which changes their marks.

**Decision:** teach each stub a `"path-info"*` branch.
- The shared stub takes an optional `runtime: Option<&str>` listing, falling back to the walk when it is `None`.
- The private copies get a `"path-info"*) exit 1` branch with a comment that the walk is intended. Then each test's expected marks can be checked against the walk deliberately rather than by accident.
- Any test whose assertions change is reviewed one by one, never by golden regeneration alone.

## R10 — Corpus target for the live path (FR-016)

`SourceKind` is `Git` or `OciImage` only, and the allowlist admits github.com, docker.io and ghcr.io (`manifest.rs:139-142, 590`).

**Decision:** create a wrapper flake for shellcheck in a public repository under `github.com/kusari-sandbox/`, which `public_only_audit` exempts (`manifest.rs:562`). Its `flake.lock` pins nixpkgs `a799d3e3`. Add a `nix-closure-shellcheck` `CorpusTarget` with `ScanMode::NixClosure { attr: "default" }` and an expectations function carrying SC-007. Goldens come from the CI `regen_goldens` dispatch.

**Caveat:** creating a repository under the organization is outward-facing and needs the maintainer's go-ahead. Shellcheck has no test-only dependencies (QuickCheck is a library dependency, per cabal line 61), so SC-007's test-exclusion half is tested hermetically on an ormolu-shaped fixture instead.

## R11 — Signature verification cost

Unmeasured. Implementation task: time `nix store verify --no-contents -r --store https://cache.nixos.org` for shellcheck, curl and hlint, cold and warm, 5 runs each, before SC-005 is asserted. If verification makes hlint's query unusably slow (more than about 0.2 s per path), stop and bring it back as a decision. Do not drop verification on our own authority.
