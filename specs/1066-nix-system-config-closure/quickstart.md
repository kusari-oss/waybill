# Quickstart: verify system-configuration closures

Needs `nix`. Build first: `cargo build -p waybill`; `B=target/debug/waybill`.
Fixtures: `waybill-cli/tests/fixtures/nix_config_closure/`. They are synthetic and have no inputs, so they need no network.

## 1. One configuration, no flags (US1)

```sh
F=waybill-cli/tests/fixtures/nix_config_closure/one_darwin
$B sbom scan --path $F --nix-closure --no-deep-hash --format cyclonedx-json --output cyclonedx-json=/tmp/one.cdx.json
jq -r '.metadata.properties[] | select(.name=="waybill:nix-closure") | .value | fromjson | .attribute' /tmp/one.cdx.json
# darwinConfigurations.laptop.system
```

## 2. Several configurations (US2)

```sh
F=waybill-cli/tests/fixtures/nix_config_closure/two_configs
$B sbom scan --path $F --nix-closure --no-deep-hash --format cyclonedx-json --output cyclonedx-json=/tmp/two.cdx.json 2>&1 | grep several-system-configurations
# ... detail="darwinConfigurations.laptop, nixosConfigurations.web01; choose one with --nix-closure-attr ..."
$B sbom scan --path $F --nix-closure --nix-closure-attr nixosConfigurations.web01.config.system.build.toplevel \
  --no-deep-hash --format cyclonedx-json --output cyclonedx-json=/tmp/web01.cdx.json
jq -r '.metadata.properties[] | select(.name=="waybill:nix-closure") | .value | fromjson | .attribute' /tmp/web01.cdx.json
# nixosConfigurations.web01.config.system.build.toplevel
```

## 3. Package flakes unchanged (US3)

```sh
F=waybill-cli/tests/fixtures/nix_config_closure/package_and_config
# C184 attribute = "default": the package wins over the configuration.
```

The corpus run shows `no semantic change` for every target, including `nix-closure-moat`.

## 4. Real configurations (SC-002)

`specs/1066-nix-system-config-closure/measurements/probe_config_closure.sh`: a minimal NixOS and a minimal nix-darwin configuration, cold and warm.
