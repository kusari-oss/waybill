# Data model: Strip credentials from declared URLs

No new persisted or emitted entities. The feature changes the values of existing fields.

## Declared URL

A string a reader takes from a manifest or lockfile as a place to fetch from.

- **Userinfo:** the part of the authority before the last `@`, as RFC 3986 defines it. It is present when the `url` crate parses the string and finds a non-empty username or a password. An empty userinfo (`https://@host`) is not a credential.
- **Sanitised form:** the original string with the userinfo span and its `@` removed. Every other byte is unchanged (research R3). Not parseable, or no userinfo: the original string.
- **Not covered:** the query string (#1652) and scp-form remotes (`user@host:path`), which do not parse as URLs.

## Inventory row

| Field | Meaning |
|---|---|
| id | D*n* (sanitise), S*n* (already sanitised), N*n* (not a declared URL) |
| reader | the reader that writes the field |
| field | the annotation, PURL qualifier or external reference |
| disposition | `sanitise`, `already sanitised`, or `not declared` with a reason |
| case | the integration case covering it (D-rows and N5 rows) |

The table is research R1. T001 re-derives it.

## Sanitisation event (log only)

| Field | Value |
|---|---|
| level / target | `WARN` / `waybill::identifiers::sanitize` |
| `manifest_file` | the manifest or lockfile path the URL was read from |
| `url_redacted` | `redact_userinfo_for_log(raw)`: the URL with its userinfo replaced by `<userinfo redacted>` |
| message | `stripped credentials from URL` |

One event per sanitised occurrence. It never contains the username or password.
