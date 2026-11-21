# Quickstart: Strip credentials from declared URLs

## See the defect on `main`

```sh
mkdir -p /tmp/wbcred && cd /tmp/wbcred
cat > CMakeLists.txt <<'EOF'
cmake_minimum_required(VERSION 3.24)
project(waybill-fixture-app)
include(FetchContent)
FetchContent_Declare(waybill-fixture-cred
  GIT_REPOSITORY https://wbfxuser:wbfxtoken7c41@git.example/org/waybill-fixture-cred.git
  GIT_TAG v1.0.0)
FetchContent_MakeAvailable(waybill-fixture-cred)
EOF
waybill --offline sbom scan --path . --format cyclonedx-json --output out.cdx.json
grep -o wbfxtoken7c41 out.cdx.json | wc -l   # measured on main's build (86bce89): 1, in waybill:download-url; this branch: 0
```

## Run this feature's tests

```sh
cargo test -p waybill --test declared_url_credentials
cargo test -p waybill --lib identifiers::sanitize
```

## Re-run the URL normalisation probe

```sh
sh specs/1640-strip-url-credentials/measurements/url-probe/run.sh
```

## Gate

```sh
./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh
```
