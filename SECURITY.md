# Security policy

## Reporting a vulnerability

Please **do not open a public GitHub issue** for vulnerabilities. Report
them privately through GitHub Security Advisories:

  https://github.com/kusari-sandbox/waybill/security/advisories/new

This routes the report to the maintainers without exposing it publicly.

## What to expect

- **Acknowledgment** within 7 days of report.
- **Triage decision** (accepted / declined / needs-more-info) within
  14 days.
- **Fix or detailed status update** within 30 days.

Coordinated disclosure timelines can be negotiated case-by-case if a
fix requires longer (e.g., a deep refactor or upstream dependency
update).

## Supported versions

waybill is pre-1.0 alpha. Only the **most recent alpha release** is
supported for security fixes. SemVer guarantees do not apply until 1.0.

| Version          | Supported          |
|------------------|--------------------|
| latest alpha     | ✅ yes              |
| older alphas     | ❌ no               |
| 1.0 and beyond   | (policy TBD at 1.0) |

## Scope

In scope:

- Crashes, hangs, or memory-safety issues during `scan` / `verify` / `trace`.
- SBOM emission that misrepresents the underlying observed evidence
  (e.g., a component reported as present that wasn't observed in the
  trace, or vice versa — this contradicts Constitution Principles VIII
  + IX at `.specify/memory/constitution.md`).
- Attestation verification failures that incorrectly accept malformed,
  expired, or revoked signatures.
- Privilege-escalation or container-escape via the eBPF tracer.
- Path-traversal, command-injection, or arbitrary-file-write in scan
  modes that consume operator-controlled paths.

Out of scope:

- Vulnerabilities in waybill's upstream Rust crate dependencies — those
  go through normal CVE channels and are tracked by the Kusari Inspector
  CI gate that runs on every PR. If you've found a dependency CVE you
  believe waybill is exposed to, please mention it in an advisory anyway
  — we'd rather have a duplicate than miss one.
- Bugs that produce wrong SBOM output without a security implication
  (those should be filed as regular GitHub issues).
- Denial-of-service via legitimate-but-large inputs (e.g., a 10 GB
  container layer that exhausts memory). Resource-exhaustion mitigations
  are tracked as ordinary issues.
