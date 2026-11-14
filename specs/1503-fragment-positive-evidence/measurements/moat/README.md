# `nix-closure-moat`: captured closure and baseline

- **Capture:** public-corpus run 43629708718, artifact `nix-closures-x86_64-linux`, file `moat-closure.json`. moat was also captured by run 43535531153, with the same sha256.
- **sha256:** `898fcdd38aa235840cc2a76b9ee4048ef826a853ce8abe19efdfb3076c8744ab`
- **Derivations:** 1,749. **Pname-less members:** 1,134.
- **C206 roles at the base (ac2bd28), in derivations:** check 46, fragment 240, package 23, scaffolding 267, source 558.
- **Shipped edges at the base:** 6,891. This equals the committed golden's C184 `edges`, so the capture reproduces the input the golden was made from.

The capture itself is not committed (about 6–10 MB). The files below were all produced by the binary from it:
- `members-before.tsv`: every pname-less member at the base, as drv, name and role (`debug closure-dump`, ac2bd28).
- `fragments-before.tsv`: the base's fragment rows joined to `builder`/`stdenv`/main-program/executable attributes (`probe/attrs.py`).
- `edge-accounting-before.txt`: the `s1367` accounting at the base.
- The matching `*-after` files, plus `delta.md`, come from the change.
