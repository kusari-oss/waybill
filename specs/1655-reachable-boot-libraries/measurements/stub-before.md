# Stub fixture before-state (2026-11-25, `main` a9a1393 + spec only)

Captured through the project's own harness: a throwaway copy of
`tests/boot_library_edges.rs` whose extra test wrote the two scans' CycloneDX
(`project_with_ghc_prim()` and the plain `project/`, both with `--nix-closure`
and `--root-name waybill-fixture-root`). The copy was deleted after the run.

## Boot libraries and C247

| PURL | C218 | C247 absent (with ghc-prim) | C247 absent (plain) |
|---|---|---|---|
| base@4.20.2.0 | ghc-9.10.3 | ghc-internal | ghc-internal, ghc-prim |
| ghc-prim@0.12.0 | ghc-9.10.3 | rts | (not a component) |
| process@1.6.26.1 | ghc-9.10.3 | deepseq, directory, filepath, os-string, unix | same |
| stm@2.5.3.1 | ghc-9.10.3 | array | same |
| base@4.19.2.0 | ghc-binary-9.8.4 | ghc-bignum, ghc-prim, rts | same |

`Win32` is a component without C218: Linux's table does not ship it.

C104 `partial`. C105: `transitive-edges-unresolvable: hackage;
boot-library-edges-incomplete: 5` (with ghc-prim) / `4` (plain). C219: both
compilers `in-table`, project `ghc-9.10.3`, system `x86_64-linux`.

## Expected additions (`shared_purls.py` over the two dumps)

    nix-closure-stub-plain	ghc-9.10.3	reachable-absent=18	held-under-other-label=0	no-component=18
    		held: 
    		new:  array@0.5.8.0 bytestring@0.12.2.0 deepseq@1.5.0.0 directory@1.3.8.5 exceptions@0.10.9 filepath@1.5.4.0 ghc-bignum@1.3 ghc-boot-th@9.10.3 ghc-internal@9.1003.0 ghc-prim@0.12.0 mtl@2.3.1 os-string@2.0.7 pretty@1.1.3.6 rts@1.0.2 template-haskell@2.22.0.0 time@1.12.2 transformers@0.6.1.1 unix@2.8.7.0
    nix-closure-stub-plain	ghc-binary-9.8.4	reachable-absent=3	held-under-other-label=0	no-component=3
    		held: 
    		new:  ghc-bignum@1.3 ghc-prim@0.11.0 rts@1.0.2
    nix-closure-stub-with-prim	ghc-9.10.3	reachable-absent=17	held-under-other-label=0	no-component=17
    		held: 
    		new:  array@0.5.8.0 bytestring@0.12.2.0 deepseq@1.5.0.0 directory@1.3.8.5 exceptions@0.10.9 filepath@1.5.4.0 ghc-bignum@1.3 ghc-boot-th@9.10.3 ghc-internal@9.1003.0 mtl@2.3.1 os-string@2.0.7 pretty@1.1.3.6 rts@1.0.2 template-haskell@2.22.0.0 time@1.12.2 transformers@0.6.1.1 unix@2.8.7.0
    nix-closure-stub-with-prim	ghc-binary-9.8.4	reachable-absent=3	held-under-other-label=0	no-component=3
    		held: 
    		new:  ghc-bignum@1.3 ghc-prim@0.11.0 rts@1.0.2

Distinct new PURLs: **18** with ghc-prim (17 for 9.10.3, plus `ghc-prim@0.11.0`;
`ghc-bignum@1.3` and `rts@1.0.2` are shared) and **19** plain (18 + `ghc-prim@0.11.0`).
The bootstrap `base@4.19.2.0` must reach `ghc-prim@0.11.0`, never `@0.12.0`.

After the change, C105 keeps `transitive-edges-unresolvable: hackage` and loses
`boot-library-edges-incomplete`, so the stub stays `partial`. Neither compiler
is out of the table and every library is in it, so the reason has no
integration-level positive control on this fixture (research R6).
