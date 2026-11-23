# Quickstart: a statically linked liblzma is versioned from the version it embeds

## The defect, locally

```sh
cd specs/1647-static-liblzma-anchor/measurements/probe
tar -cf - a.c b.c probe.sh | docker run --rm -i --platform linux/amd64 ubuntu:24.04 \
  bash -c 'mkdir /w && tar -xf - -C /w && bash /w/probe.sh'
```

To scan a built program, copy it out of the container (`tar -cf - a.static` on the container side), put it alone in a directory, then run:

```sh
RUST_LOG=waybill::version_census=debug ./target/debug/waybill sbom scan --path <dir> --offline \
  --format cyclonedx-json --output cyclonedx-json=<dir>.cdx.json 2> <dir>.log
```

- On `main`: there is no `pkg:generic/liblzma` component and no census line for the file.
- On the wired branch: `pkg:generic/liblzma@5.4.5`, and a `liblzma_static` event with `rule_a.verdict = versioned`.

## The tests

```sh
cargo test -p waybill --test version_string_static
cargo test -p waybill --lib s1647
```

## The gate

```sh
./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh
```

## The census (CI only for TheRock)

Dispatch `census-versions.yml` in kusari-sandbox/waybill-fixture-therock-sdk, with `--ref main` and `-f waybill_ref=<full SHA>`, then verify every job's `refs.txt`. `census/versions.py` summarises the `liblzma_static` events per target. The `control` job asserts its real static build's result.
