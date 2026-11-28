# Data model

## C172 value (unchanged keys)

| Form | `emitted` | `nixpkgs` | `requirement-ranges` | `note` |
|---|---|---|---|---|
| Lock-sourced (R2) | lock version | nixpkgs version | absent | `the emitted version came from the Stack lock file <files> and takes precedence` (or `the freeze file <files>`) |
| Unresolved (#1457), not merged | `null` | nixpkgs version | declared ranges | unchanged |
| Merged, built version differs (R4) | built version | nixpkgs version | absent | `the closure built this version; the pinned revision's version does not satisfy the project's declared requirement and a flake overlay pinned another` |
| Merged, built version equals `nixpkgs` (R4) | — | — | — | C172 removed; C221 records the range violation |
| Main module (R3) | — | — | — | not emitted |

`<files>` is the component's source paths, comma-separated and sorted.

## Orphan reason

`hackage-stack-lock` or `hackage-freeze`, when unreachable, gives `edgeless-lock-pin`.

## C174 `disagreements`

Equals the number of components that carry C172 in the emitted document.
