# Contract: C172, C174, C45

- **C172** is present iff the component is Haskell, is not a boot library, is not a main module, and its emitted version differs from nixpkgs'. It is also present in the unresolved `"emitted": null` form until a twin merge gives the component a version; then R4 applies. The note names the version's source as in data-model.md, and never names a lock or freeze file unless the component's source type is that kind.
- **C174** `disagreements` equals the count of C172 in the document.
- **C45** gives `edgeless-lock-pin` for an unreachable component whose source type is `hackage-stack-lock` or `hackage-freeze`.
- **Parity**: the C172 value is equal in CycloneDX, SPDX 2.3 and SPDX 3. The keys are unchanged.
