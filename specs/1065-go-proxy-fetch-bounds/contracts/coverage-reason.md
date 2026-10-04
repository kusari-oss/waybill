# Contract: C111 codes for proxy-fetch bounds

Extends `waybill:go-transitive-coverage-reason` (C111). The grammar is unchanged:
`<code>: <detail>[; <code>: <detail>]*`.

## New codes

### `proxy-unreachable`

C110 value: `unknown`.

```
proxy-unreachable: <entry> failed at the network level (<class>); <n> modules resolved from go.sum only
```

- `<entry>`: `scheme://host[:port]`. Never userinfo, path or query.
- `<class>`: one of `connection`, `timeout`, `dns`, `tls` (the existing `ErrorClass::as_str` values).
- `<n>`: modules whose outcome came from the tripped entry: the failures that tripped it plus every module skipped after it.

There is one fragment per tripped entry, in chain order.

### `proxy-fetch-budget-exhausted`

C110 value: `partial`, or `unknown` if another reason is present.

```
proxy-fetch-budget-exhausted: <secs>s spent; <n> modules not attempted, resolved from go.sum only
```

- `<secs>`: the budget, as `<n>s` when it is whole seconds (`60s` by default), otherwise `<n>ms` (test overrides).
- `<n>`: modules a worker declined to start.

## Examples

Unreachable proxy, 64 modules:

```
waybill:go-transitive-coverage        = unknown
waybill:go-transitive-coverage-reason = proxy-unreachable: http://10.255.255.1 failed at the network level (timeout); 64 modules resolved from go.sum only
```

Both bounds, with a `|` chain whose first entry is dead and whose second is slow:

```
waybill:go-transitive-coverage-reason = proxy-unreachable: https://corp-proxy.example failed at the network level (connection); 900 modules resolved from go.sum only; proxy-fetch-budget-exhausted: 60s spent; 212 modules not attempted, resolved from go.sum only
```

## Invariants

1. No bound tripped means both annotations are byte-identical to the output before this feature (FR-009).
2. Identical decoded value in CycloneDX, SPDX 2.3 and SPDX 3 (FR-007, existing parity row).
3. Every module in go.sum is still a component with `waybill:go-transitive-source = go-sum-fallback` (FR-005). No per-component annotation is added.
