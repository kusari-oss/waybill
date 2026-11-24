# T001: census of `design` components in the closure goldens (FR-001)

Source: the committed corpus goldens at `main` `18691d8` (CI Linux), CycloneDX, with components counted recursively.

**Origin** is read from each component's evidence:
- **reader-declared:** a `package.yaml` or `.cabal` occurrence, or C-evidence-kind;
- **transitive walk:** none of that, plus the walk's synthetic identity evidence (technique `instrumentation`, confidence 0.95).

| Target | `design` | C203 evaluated package | C218 (compiler) | neither | C218 origin: reader-declared | C218 origin: transitive walk (m985) | bootstrap-versioned `design` |
|---|---|---|---|---|---|---|---|
| nix-closure-moat | 21 | 1 (moat) | 19 (ghc-9.10.3) | 1 ['readme'] | 7 | 12 | 0 |
| nix-closure-openai | 22 | 1 (openai) | 21 (ghc-9.6.6) | 0 [] | 6 | 15 | 0 |
| nix-closure-shellcheck | 0 | 0 () | 0 () | 0 [] | 0 | 0 | 0 |
| nix-closure-slack-web | 24 | 1 (slack-web) | 23 (ghc-9.10.3) | 0 [] | 9 | 14 | 0 |
| nix-closure-slacklinker | 24 | 1 (slacklinker) | 23 (ghc-9.10.3) | 0 [] | 10 | 13 | 0 |

## Findings

- **Research R7's counts hold:** 21, 22, 24, 24 and 0. Every `design` component is in one of the two groups, except moat's `readme`, which has neither C203 nor C218: it is a repository package the closure does not evaluate.
- **No `design` component is versioned from a bootstrap `ghc-binary`.** Every C218 on a `design` component names the closure's own compiler.
- **C218 and C203 `exempt-evaluated-package` appear only in the five closure goldens.** Checked across every committed golden, none of the 30-odd others carries either. So the re-tier cannot reach a manifest-only document (R4, FR-007).

## R1 corrected: three paths, not two, leave a boot library `design`

The plan found two: the reader's declaration, and the closure's `held_purls` check. The census finds a third, and it is the larger: 12, 15, 14 and 13 of the C218 components per target. Moat's evidence-free `transformers` and `unix` are among them.

**Milestone 985's transitive runtime walk** over the nixpkgs package set (`nix/haskell_packages/mod.rs`, the "transitive runtime closure" block) builds each reached member with `new_closure_component`, which sets `design`:
- a non-boot member then goes through `apply_resolved`, which sets `source`;
- a boot member goes through `apply_compiler_version`, which leaves the tier, so it stays `design` with C218.

When the closure pass later reaches the same library, its PURL is already held, so no `build` copy is synthesised.

**FR-003's rule keys on C218, not on origin,** so it covers all three paths unchanged. The census changes the research, not the design.
