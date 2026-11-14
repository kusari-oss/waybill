# `nix-closure-openai`: captured closure and baseline

- **Capture:** public-corpus run 43629708718, artifact `nix-closures-x86_64-linux`, file `openai-closure.json`.
- **sha256:** `a810dc53c2d670084cdd65263222e7d890371568c99fc183771ad8259a32d01e`
- **Derivations:** 1,448. **Pname-less members:** 816.
- **C206 roles at the base (ac2bd28), in derivations:** fragment 67, package 3, scaffolding 8, source 738.
- **Shipped edges at the base:** 48,336. This equals the committed golden's C184 `edges`, so the capture reproduces the input the golden was made from.

The capture itself is not committed (about 6–10 MB). The files below were all produced by the binary from it:
- `members-before.tsv`: every pname-less member at the base, as drv, name and role (`debug closure-dump`, ac2bd28).
- `fragments-before.tsv`: the base's fragment rows joined to `builder`/`stdenv`/main-program/executable attributes (`probe/attrs.py`).
- `edge-accounting-before.txt`: the `s1367` accounting at the base.
- The matching `*-after` files, plus `delta.md`, come from the change.
