# PURL casing

Whether a package identifier keeps its declared capitalisation is not a style
question. For some ecosystems it decides whether the component matches an
advisory at all, and getting it wrong fails silently — a scanner reports fewer
vulnerabilities, which reads as good news.

This page records where waybill preserves case, where it folds it, and the
measurement behind each.

## Go — preserve, always

waybill emits a Go module path exactly as `go.mod` and `go.sum` declare it:

```
go.sum:   github.com/DataDog/zstd v1.5.5 h1:...
waybill:  pkg:golang/github.com/DataDog/zstd@v1.5.5
```

**OSV's Go matching is case-sensitive on the canonical module path.** Measured
against the live API:

```
POST https://api.osv.dev/v1/query  {"package":{"name":"...","ecosystem":"Go"}}

  github.com/Masterminds/goutils  ->  2 vulns
  github.com/masterminds/goutils  ->  0 vulns

  github.com/Masterminds/vcs      ->  2 vulns
  github.com/masterminds/vcs      ->  0 vulns
```

Lowercasing does not degrade the match, it eliminates it. A tool that folds the
path reports zero advisories for that package and no error.

Modules with no OSV entries at all — `github.com/Azure/azure-sdk-for-go`,
`github.com/DataDog/datadog-agent` — return 0 for both spellings and say
nothing either way. The two Masterminds modules are the discriminating cases,
which is why they are the ones quoted.

Pinned by `waybill-cli/tests/go_purl_casing.rs`, which scans a fixture whose
module path has capitals in both the owner and repository segments and asserts
the emitted PURL is byte-identical. The test also asserts the folded form is
*absent*, so a normalisation pass names itself in the failure rather than
quietly changing the identifier.

## Where waybill does fold case, deliberately

`pkg:generic/*` names on the CMake/C++ path are lowercased (catalog row C103).
That is the normalised form for an ecosystem with no registry and no canonical
spelling to preserve — there is nothing to match against that would care.

**The boundary is the point.** Folding is correct where no downstream matcher
keys on the identifier, and wrong where one does. It should be decided per
ecosystem against how that ecosystem's advisory database actually behaves, not
extended from one reader to another by analogy. Extending the C103 lowercasing
to Go would pass every other test in the tree and silently stop matching
advisories.

## Ecosystems not yet measured

npm, PyPI and Maven have their own normalisation rules — PyPI's in particular
is specified (PEP 503) and waybill applies it via
`pip::normalize_pypi_name_for_purl`. Whether any other reader's case handling
diverges from what its advisory source expects has not been probed. Issue #928
raised this; only the Go arm has been measured.
