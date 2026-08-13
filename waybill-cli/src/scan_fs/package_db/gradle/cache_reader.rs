//! Milestone 235 US2 — Gradle cache reader (stub for m235 MVP).
//!
//! Follow-on milestone will walk `${GRADLE_USER_HOME}/caches/modules-2/`
//! and reconstruct the resolved graph from cached POMs + `.module`
//! metadata. See contracts/gradle-cache-reader.md.
//!
//! MVP surface: nothing exported yet. When US2 lands, this file grows
//! `discover_cache`, `parse_cached_pom`, `parse_cached_module`,
//! `walk_transitives`, `cache_freshness`, `resolve_via_cache`.
