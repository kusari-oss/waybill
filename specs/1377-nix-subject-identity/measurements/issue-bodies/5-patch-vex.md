#1040 §2 set aside VEX for backported patches because patch filenames sampled then rarely named a CVE, and closed with: *"count how many reported vulnerabilities are false positives attributable to a nixpkgs patch. That number decides whether this is worth building, and nobody has it."* This is that count, for six targets.

## Measurement

nixpkgs `a799d3e3886da994fa307f817a6bc705ae538eeb`, aarch64-darwin, grype 0.120.0. Targets: wrapper flakes for `nixpkgs#{curl,openssh,httpie,gh,ripgrep}` plus the `moat` corpus pin, each scanned by waybill (`--nix-closure`) and sbomnix 1.9.0 (`--buildtime`).

A finding counts as **already patched** when the matched component's own derivation carries a patch whose filename, or whose contents where the file is in the store, names that CVE. Positive control: `unzip` 6.0 carries 17 patches, 13 named for CVEs, and every one is detected.

| deduplicated across the six targets | distinct grype findings | already patched |
| --- | --- | --- |
| sbomnix | 382 | **20** (5.2%) — unzip 13, jq 5, libssh2 1, fontforge 1 |
| waybill | 307 | **6** (2.0%) — jq 5, libssh2 1 |

Only **one** lands on a component that ships: `libssh2 1.11.1`, CVE-2026-7598, in curl's runtime closure, reported from both documents.

## Against #1040's open questions

- *Do security backports exist without naming the CVE in the filename?* Not tested. This method only sees CVE-named patches, so the counts above are lower bounds.
- *Does nixpkgs ship a patched build whose version still matches a vulnerable range?* **Yes.** libssh2 1.11.1 in curl's runtime closure is one: grype matches CVE-2026-7598 on version, and the derivation carries a patch naming it.
- *Is the justification strong enough to act on?* Unchanged. The judgment is still open.

## Reading of the number (judgment, not measurement)

The effect is real but small at runtime scope: one finding across six targets. At buildtime scope it is six findings for waybill. An existing slot could carry it with no new mechanism: waybill already emits `pedigree.patches[]` with `type: backport` on closure components and leaves `resolves` empty. Whether that, an OpenVEX sidecar, or nothing is worth it is the decision #1040 deferred.

Refs #1040, #1034

---
*Drafted with AI assistance (Claude), reviewed and filed by me. Measurements quoted were executed and verified.*
