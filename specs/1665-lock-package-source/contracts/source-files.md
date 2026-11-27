# Contract: source files of Haskell lock-pinned components (C18)

A component with `waybill:source-type` of `hackage-stack-lock` or `hackage-freeze` has `waybill:source-files` equal to the sorted, unique, rootfs-relative list of every `stack.yaml.lock` or `cabal.project.freeze` that pinned exactly its name and version. It is never `[""]`.

Identical in CycloneDX, SPDX 2.3 and SPDX 3 (C18 `SymmetricEqual`). The orphan reason of these components is unchanged by this contract.
