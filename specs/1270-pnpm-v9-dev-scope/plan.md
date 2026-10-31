# Implementation Plan: pnpm v9 development scope

**Branch**: `1270-pnpm-v9-dev-scope` | **Date**: 2026-10-29 | **Spec**: [spec.md](spec.md)

## Summary

Read importer-resolved declarations, walk the existing snapshot adjacency from runtime/optional roots, and seed only dev-only direct identities. Retain explicit Runtime for the non-development protected closure when no existing optional/peer classification applies, even without importer manifests. Leave other unclassified transitives for the existing additive scope propagation and post-resolution exclusion pass.

## Technical Context

**Language/Version**: Rust pinned 1.98.1.
**Primary Dependencies**: Existing serde_yaml and standard collections only.
**Storage**: In-process sets and adjacency only.
**Project Type**: Source CLI.
 Source CLI, no privileged/eBPF work, no network at scan time. One linear graph walk per lockfile, not per dependency. Preserve existing canonicalization and output carriers. No new crate or new metadata field.

## Constitution Check

Principles I/IV: Rust, typed package entries, no production unwrap. V: reuse CDX scope/C42, SPDX 2.3 DEV_DEPENDENCY_OF and SPDX 3 LifecycleScopedRelationship; no new property needs audit. VII: offline unprivileged CLI tests. IX/X: runtime wins and inferred transitives retain C62 attribution from the existing pass. Operating Modes: scan evidence only, no trace semantics changed. All pass before and after design.

## Project Structure

Production: waybill-cli/src/scan_fs/package_db/npm/pnpm_lock.rs.
Tests: pnpm_lock.rs unit controls plus waybill-cli/tests/pnpm_v9_scope.rs and generated fixtures.
Documentation: this directory and CHANGELOG.md.

## Validation

Write and run end-to-end regression before production edit, prove failure on base. Run targeted unit controls, all three format outputs, then scripts/pre-pr.sh with its actual status and positive completion marker. Never regenerate goldens to mask drift. Debug symbols may be disabled through normal Cargo profile environment to keep the full gate within local disk capacity; checks and assertions remain enabled.
