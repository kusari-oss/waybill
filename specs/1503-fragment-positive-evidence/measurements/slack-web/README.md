# `nix-closure-slack-web`: captured closure and baseline

- **Capture:** public-corpus run 43629708718, artifact `nix-closures-x86_64-linux`, file `slack-web-closure.json`.
- **sha256:** `7732d430615d7c7eefb44d4baee4bd6166433e32acc2bc7e4b6c2e9f634fc084`
- **Derivations:** 1,293. **Pname-less members:** 727.
- **C206 roles at the base (ac2bd28), in derivations:** fragment 66, package 3, scaffolding 8, source 650.
- **Shipped edges at the base:** 44,426. This equals the committed golden's C184 `edges`, so the capture reproduces the input the golden was made from.

The capture itself is not committed (about 6–10 MB). The files below were all produced by the binary from it:
- `members-before.tsv`: every pname-less member at the base, as drv, name and role (`debug closure-dump`, ac2bd28).
- `fragments-before.tsv`: the base's fragment rows joined to `builder`/`stdenv`/main-program/executable attributes (`probe/attrs.py`).
- `edge-accounting-before.txt`: the `s1367` accounting at the base.
- The matching `*-after` files, plus `delta.md`, come from the change.
