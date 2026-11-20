# Quickstart

```sh
# Two projects, one finds the other.
mkdir -p /tmp/q/a /tmp/q/b
printf 'project(waybill-fixture-a VERSION 1.0.0)\nfind_package(waybill-fixture-b REQUIRED)\n' > /tmp/q/a/CMakeLists.txt
printf 'project(waybill-fixture-b VERSION 2.0.0)\n' > /tmp/q/b/CMakeLists.txt
waybill sbom scan --path /tmp/q --format cyclonedx-json --offline --output /tmp/q/o.json
jq -r '.components[] | select(.properties[]? | .name=="waybill:source-mechanism" and .value=="cmake-project") | .purl' /tmp/q/o.json
jq -r '.dependencies[] | select(.ref|test("waybill-fixture-a")) | .dependsOn' /tmp/q/o.json
```

Expected:
- `pkg:generic/waybill-fixture-a@1.0.0` and `pkg:generic/waybill-fixture-b@2.0.0`;
- `a` depends on `b`;
- C237 shows `"paired":{…,"project":1,…}`.

Test entry points:
- `cargo test -p waybill --test cmake_project_components`
- `cargo test -p waybill --bins cmake_search_pair::`
- `cargo test -p waybill --bins declared_attach::`
- `cargo test -p waybill --bins root_selector::`
