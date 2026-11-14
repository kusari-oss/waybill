# `nix-closure-slacklinker`: captured closure and baseline

- **Capture:** public-corpus run 43629708718, artifact `nix-closures-x86_64-linux`, file `slacklinker-closure.json`.
- **sha256:** `ec11ed0a25cb44bc92d4173c54c2c18d1321429b9b3c3fac9828f5b0c3c00772`
- **Derivations:** 2,518. **Pname-less members:** 1,567.
- **C206 roles at the base (ac2bd28), in derivations:** check 47, fragment 250, package 34, scaffolding 267, source 969.
- **Shipped edges at the base:** 12,169. This equals the committed golden's C184 `edges`, so the capture reproduces the input the golden was made from.

The capture itself is not committed (about 6–10 MB). The files below were all produced by the binary from it:
- `members-before.tsv`: every pname-less member at the base, as drv, name and role (`debug closure-dump`, ac2bd28).
- `fragments-before.tsv`: the base's fragment rows joined to `builder`/`stdenv`/main-program/executable attributes (`probe/attrs.py`).
- `edge-accounting-before.txt`: the `s1367` accounting at the base.
- The matching `*-after` files, plus `delta.md`, come from the change.
