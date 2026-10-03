# Contract: owning-component (anchor) identity

## PURL

```
pkg:generic/<resolve-name>?pants-namespace=<namespace>
```

- `<resolve-name>`: the resolve's name, percent-encoded as a PURL name segment.
- `<namespace>` ∈ {`python`, `jvm`}.
- Applies to every owning component, whether or not a name collides.
- `pants-namespace` is waybill's own qualifier key; the purl-spec `generic`
  type registers only `download_url` and `checksum` (#1106).

Examples: `pkg:generic/python-default?pants-namespace=python`,
`pkg:generic/java17?pants-namespace=jvm`.

## Derived identifiers

- CycloneDX `bom-ref` = the PURL string (as for every component).
- SPDX 2.3 `SPDXID` and SPDX 3 element IRI are derived from the PURL as for every
  component, so they change for existing Python anchors.

## Invariants

1. Two owning components with equal `<resolve-name>` and different
   `<namespace>` are distinct components in every format.
2. `purl.name()` is the resolve name: split filenames and the m922 naming root,
   which derive from it, do not change.
3. Code that looks up an anchor for a resolve MUST match on name **and** the
   `waybill:pants-resolve-namespace` annotation. Name plus `generic`
   ecosystem alone is ambiguous in a collision repository.
4. Comparing with qualifiers stripped is outside waybill's control. A consumer
   doing it sees same-named anchors as one (documented limitation, #1106).
