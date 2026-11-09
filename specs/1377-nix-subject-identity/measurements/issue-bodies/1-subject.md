A `--nix-closure` scan of a flake describes the scan, not the package the flake builds. The document's subject is a placeholder with a placeholder CPE, and the built package appears as an ordinary component carrying no CPE. A CPE-matching scanner therefore cannot connect any vulnerability to the thing that was scanned.

## Measurement

nixpkgs `a799d3e3886da994fa307f817a6bc705ae538eeb`, aarch64-darwin, waybill `main@72b455e2`. Target: a wrapper flake exposing `nixpkgs#openssh` as `packages.<system>.default` (repro below).

| | waybill | sbomnix 1.9.0 (`--buildtime`) |
| --- | --- | --- |
| `metadata.component.name` / `version` | `filesystem-scan` / `0.0.0` | `openssh` / `10.3p1` |
| `metadata.component.cpe` | `cpe:2.3:a:waybill:filesystem-scan:0.0.0:*:*:*:*:*:*:*` | `cpe:2.3:a:openbsd:openssh:10.3p1:*:*:*:*:*:*:*` |
| `openssh 10.3p1` as a component | present, **no CPE** | (it is the subject) |
| grype 0.120.0 findings on `openssh 10.3p1` | **0** | 28 |

None of the 28 is attributable to a CVE-named nixpkgs patch on openssh; they are otherwise not adjudicated. The CPE sbomnix uses is the one nixpkgs itself declares (`openssh.meta.identifiers.cpe` at that revision).

## Expected

When the scan target is a flake whose evaluated package waybill resolves (`--nix-closure`, `--nix-closure-attr`), that package is the document's subject, with its name, version and identity. A synthetic root should not carry a CPE at all, since `waybill:filesystem-scan` names no product a feed could hold.

#1177 made the equivalent change for an explicitly named image.

## Reproduce

```sh
mkdir openssh-wrap && cd openssh-wrap
cat > flake.nix <<'EOF'
{
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/a799d3e3886da994fa307f817a6bc705ae538eeb";
  outputs = { self, nixpkgs }:
    let systems = [ "aarch64-darwin" "x86_64-linux" "aarch64-linux" ];
    in { packages = nixpkgs.lib.genAttrs systems (s: { default = nixpkgs.legacyPackages.${s}.openssh; }); };
}
EOF
git init -q && git add flake.nix && nix flake lock && git add flake.lock && git commit -qm wrap
waybill sbom scan --nix-closure --no-deep-hash --no-clearly-defined --path . \
  --format cyclonedx-json --output cyclonedx-json=waybill.cdx.json
jq '.metadata.component | {name, version, cpe}' waybill.cdx.json
jq '[.components[] | select(.name=="openssh") | {name, version, cpe}]' waybill.cdx.json
```

Refs #431, #1034

---
*Drafted with AI assistance (Claude), reviewed and filed by me. Measurements quoted were executed and verified.*
