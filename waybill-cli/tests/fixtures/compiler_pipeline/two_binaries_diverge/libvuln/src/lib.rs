//! libvuln — a hypothetical vulnerable library. m210 SC-001 fixture.
//!
//! The "vulnerability" here is symbolic — it's the presence of this
//! file in a binary's source-read-set that matters for the SC-001
//! attribution test, not any real security issue.
pub fn vulnerable_computation() -> u32 {
    // Imagine a CVE lives on this line.
    1337
}
