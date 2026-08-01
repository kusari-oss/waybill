# Not a Pants repo

Fixture for T030 (SC-003 / FR-007) — repo with no `pants.toml` and no
`3rdparty/python/*.lock` files. The pants-pex reader MUST return early
(zero components, no FR-010 log line) so this feature adds zero cost to
non-Pants repos.
