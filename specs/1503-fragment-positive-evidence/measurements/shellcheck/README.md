# `nix-closure-shellcheck`: captured closure and baseline

- **Capture:** public-corpus run 43629708718, artifact `nix-closures-x86_64-linux`, file `shellcheck-closure.json`.
- **sha256:** `e2d8d769da1966075d38bd159f963887f9c417c2dcbf1916a59b2a1dcbcc38e7`
- **Derivations:** 2,209. **Pname-less members:** 1,393.
- **C206 roles at the base (ac2bd28), in derivations:** check 46, fragment 241, package 23, scaffolding 267, source 816.
- **Shipped edges at the base:** 10,224. This equals the committed golden's C184 `edges`, so the capture reproduces the input the golden was made from.

The capture itself is not committed (about 6–10 MB). The files below were all produced by the binary from it:
- `members-before.tsv`: every pname-less member at the base, as drv, name and role (`debug closure-dump`, ac2bd28).
- `fragments-before.tsv`: the base's fragment rows joined to `builder`/`stdenv`/main-program/executable attributes (`probe/attrs.py`).
- `edge-accounting-before.txt`: the `s1367` accounting at the base.
- The matching `*-after` files, plus `delta.md`, come from the change.
