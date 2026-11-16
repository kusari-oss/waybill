# SC-008: what a root-only `complete` would change (T021)

**Probe:** `root_only.py`, run with `python3 -I -B` on run 43961229275's `corpus-goldens-regen` artifact. It takes two definitions from the classifier, not a guess at it:
- file-tier components are excluded (m194 US3);
- a main module is a component carrying `waybill:demoted-from-main-module`.

**Validation:**
- On every document marked `complete`, the probe and waybill's verdict agree.
- An earlier draft counted `waybill:workspace-member` as a main-module marker and included file-tier entries. That draft reported 10 flask "main modules" and an SC-003 "failure" in rust-ripgrep made of 7 file-tier blobs. Neither was real, and both are fixed.

| target | components | verdict | unreachable from root alone | reached by a main module | reached by nothing |
|---|---|---|---|---|---|
| go-cobra | 8 | partial | 2 | 0 | 2 |
| go-opentelemetry | 333 | partial | 264 | 0 | 264 |
| haskell-aeson | 61 | partial | 0 | 0 | 0 |
| haskell-language-server | 370 | partial | 2 | 0 | 2 |
| haskell-security-advisories | 251 | partial | 3 | 0 | 3 |
| image-postgres16 | 146 | partial | 0 | 0 | 0 |
| maven-guice | 48 | partial | 5 | 0 | 5 |
| nix-closure-moat | 1628 | partial | 493 | 0 | 493 |
| nix-closure-openai | 1374 | partial | 12 | 0 | 12 |
| nix-closure-shellcheck | 2085 | complete | 0 | 0 | 0 |
| nix-closure-slack-web | 1247 | partial | 12 | 0 | 12 |
| nix-closure-slacklinker | 2940 | partial | 498 | 0 | 498 |
| npm-express | 44 | partial | 0 | 0 | 0 |
| pants-clojure-polyglot | 55 | complete | 0 | 0 | 0 |
| pants-example-django | 46 | complete | 0 | 0 | 0 |
| pants-example-golang | 4 | partial | 0 | 0 | 0 |
| pants-example-javascript | 303 | complete | 0 | 0 | 0 |
| pants-example-jvm | 28 | complete | 0 | 0 | 0 |
| pants-example-python | 11 | complete | 0 | 0 | 0 |
| python-flask | 108 | partial | 10 | 0 | 10 |
| rust-ripgrep | 61 | complete | 0 | 0 | 0 |

**SC-003 (I-2):** 0 documents are marked `complete` with a component that nothing reaches. T025 is not blocked.

**SC-008:** a root-only definition would change **0** verdicts on this corpus. No component in any document is reached only through a main module.

**Why the answer is zero.**
- Every corpus target is scanned with `--root-name`, which keeps each main module as a direct root edge (milestone 860).
- So the root alone reaches everything a main module reaches, and the two definitions coincide by construction.
- They can differ only on a scan without `--root-name`, and the corpus has none. The answer is therefore "no effect where measured", not "no effect".

**No follow-up issue is filed.** A zero produced by construction gives a follow-up nothing to act on, which departs from T021's text. A scan without `--root-name` would be needed to measure the question, and that is a corpus-design decision, raised in the PR rather than filed.
