# Testing

## Running Tests

waybill is a Rust workspace; tests are run via `cargo`. The
mandatory pre-PR gate runs both clippy and the full test suite:

```bash
# The mandatory pre-PR gate (clippy + workspace tests).
./scripts/pre-pr.sh

# Equivalent manual invocation:
cargo +stable clippy --workspace --all-targets -- -D warnings
cargo +stable test --workspace
```

For SBOM-spec-touching changes, also opt-in to the SPDX-3
conformance validator:

```bash
WAYBILL_REQUIRE_SPDX3_VALIDATOR=1 ./scripts/pre-pr.sh
```

See [`CONTRIBUTING.md`](../CONTRIBUTING.md) for the full pre-PR
contract and the speckit lifecycle.

## Test Requirements

- All pull requests must include tests for new functionality
- Bug fixes should include a regression test
- Tests must pass in CI before a pull request can be merged

## Writing Tests

- Place tests alongside the code they test, or in a dedicated `tests/` directory
- Follow existing test patterns and naming conventions
- Aim for meaningful coverage of critical paths and edge cases