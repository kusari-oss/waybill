//! Sysroot-vs-rootfs detection for Yocto/OE scans (milestone 107
//! US3, FR-005, FR-005a).
//!
//! The opkg reader needs to know whether the scan target is a Yocto
//! **SDK sysroot** (cross-compile artifact — every component tagged
//! `LifecycleScope::Build`) or a **device rootfs** (runtime artifact
//! — components carry no lifecycle scope). The detection uses a
//! two-signal heuristic per the 2026-06-01 clarification Q3.
//!
//! ## Two-signal heuristic
//!
//! **Primary signal** — Yocto SDK env-script presence.
//!
//! A file matching the glob `environment-setup-*` exists in EITHER:
//! 1. The scan-target dir itself, OR
//! 2. The scan-target's immediate parent dir.
//!
//! Yocto's SDK installer always writes `environment-setup-<TARGET-SYS>`
//! alongside the sysroot. Detection at the parent dir catches cases
//! where the operator scans the sysroot subdir directly.
//!
//! **Secondary signal** — filesystem shape.
//!
//! ALL of: `<scan-target>/usr/include/` exists AND
//! `<scan-target>/etc/init.d/` does NOT exist.
//!
//! Catches sysroots moved away from their SDK-installer parent dir
//! (no longer have an adjacent env-script).
//!
//! ## Combination logic
//!
//! | Primary | Secondary | Result |
//! |---|---|---|
//! | ✅ | ✅ | `Sysroot` |
//! | ✅ | ❌ | `AmbiguousSysroot` — primary wins, record conflict |
//! | ❌ | ✅ | `Sysroot` |
//! | ❌ | ❌ | `Rootfs` |
//!
//! `AmbiguousSysroot` emits a `mikebom:scan-ambiguity` diagnostic
//! annotation on the SBOM metadata via the caller's
//! `ScanDiagnostics` collector.

use std::path::Path;

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub(crate) enum ScanContext {
    /// Confirmed sysroot — at least one signal fires cleanly. Build
    /// scope applied to every emitted entry.
    Sysroot {
        primary_signal: bool,
        secondary_signal: bool,
    },
    /// Confirmed runtime rootfs — neither signal fires.
    Rootfs,
    /// Primary fires but secondary conflicts (env-script present AND
    /// `/etc/init.d/` also present — rare). Build-scope applied (primary
    /// wins) but ambiguity annotation recorded.
    AmbiguousSysroot { reason: String },
}

#[allow(dead_code)]
impl ScanContext {
    /// True when the context should drive build-scope tagging on
    /// emitted entries (sysroot semantics).
    pub(crate) fn applies_build_scope(&self) -> bool {
        matches!(
            self,
            ScanContext::Sysroot { .. } | ScanContext::AmbiguousSysroot { .. }
        )
    }

    /// `Some(reason)` when the context recorded a scan-ambiguity for
    /// SBOM-metadata emission; `None` otherwise.
    pub(crate) fn ambiguity_reason(&self) -> Option<&str> {
        match self {
            ScanContext::AmbiguousSysroot { reason } => Some(reason),
            _ => None,
        }
    }
}

/// Apply the two-signal heuristic to a scan target.
///
/// Ambiguity is recorded ONLY when the primary signal fires AND
/// `/etc/init.d/` is actively present (a strong "this is a rootfs"
/// counter-signal). Cases where the secondary signal merely lacks
/// corroborating evidence (e.g. a stripped sysroot with no
/// `/usr/include/`) are NOT ambiguous — primary alone is sufficient.
#[allow(dead_code)]
pub(crate) fn detect_scan_context(rootfs: &Path) -> ScanContext {
    let primary = has_env_script_in_target_or_parent(rootfs);
    let include_present = rootfs.join("usr").join("include").is_dir();
    let init_d_present = rootfs.join("etc").join("init.d").is_dir();
    let secondary = include_present && !init_d_present;

    // The "actively contradictory" case: primary fires AND init.d is
    // present (strong rootfs marker). Apply build-scope but record
    // the conflict for transparency.
    if primary && init_d_present {
        return ScanContext::AmbiguousSysroot {
            reason: format!(
                "env-script present but filesystem shape suggests rootfs \
                 (init.d=true, usr/include={include_present})"
            ),
        };
    }

    match (primary, secondary) {
        (true, _) => ScanContext::Sysroot {
            primary_signal: true,
            secondary_signal: secondary,
        },
        (false, true) => ScanContext::Sysroot {
            primary_signal: false,
            secondary_signal: true,
        },
        (false, false) => ScanContext::Rootfs,
    }
}

fn has_env_script_in_target_or_parent(rootfs: &Path) -> bool {
    // Walk up to TWO levels above the scan target so we catch both
    // common Yocto SDK layouts:
    //   - operator scans `/opt/poky/5.0/` (SDK root) — env-script in target
    //   - operator scans `/opt/poky/5.0/sysroots/<arch>/` (inner sysroot)
    //     — env-script lives at `/opt/poky/5.0/environment-setup-<arch>`,
    //       which is the GRANDPARENT
    let mut cursor: Option<&Path> = Some(rootfs);
    for _ in 0..3 {
        let Some(dir) = cursor else {
            break;
        };
        if dir_has_env_script(dir) {
            return true;
        }
        cursor = dir.parent();
    }
    false
}

fn dir_has_env_script(dir: &Path) -> bool {
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return false;
    };
    for entry in read_dir.flatten() {
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        if name.starts_with("environment-setup-") {
            // Match a file (not a directory) — env-scripts are regular files.
            if let Ok(ft) = entry.file_type() {
                if ft.is_file() {
                    return true;
                }
            }
        }
    }
    false
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    fn touch(path: &Path) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, "").unwrap();
    }

    fn mkdir(path: &Path) {
        std::fs::create_dir_all(path).unwrap();
    }

    #[test]
    fn env_script_in_scan_target_fires_primary() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path();
        touch(&target.join("environment-setup-cortexa7t2hf-mikebom-fixture"));
        let ctx = detect_scan_context(target);
        // Primary fires; secondary doesn't (no usr/include); should be
        // Sysroot, not AmbiguousSysroot (since secondary's absence
        // isn't a "conflict" — the conflict is when secondary actively
        // contradicts).
        assert!(matches!(ctx, ScanContext::Sysroot { primary_signal: true, secondary_signal: false }));
    }

    #[test]
    fn env_script_in_parent_dir_fires_primary() {
        let tmp = tempfile::tempdir().unwrap();
        let parent = tmp.path();
        let target = parent.join("sysroots").join("mikebom-fixture-target");
        mkdir(&target);
        touch(&parent.join("environment-setup-mikebom-fixture-target"));
        let ctx = detect_scan_context(&target);
        assert!(matches!(ctx, ScanContext::Sysroot { primary_signal: true, .. }));
    }

    #[test]
    fn secondary_signal_fires_on_include_without_init_d() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path();
        mkdir(&target.join("usr").join("include"));
        // /etc/init.d intentionally absent.
        let ctx = detect_scan_context(target);
        assert!(matches!(ctx, ScanContext::Sysroot { primary_signal: false, secondary_signal: true }));
    }

    #[test]
    fn rootfs_when_neither_signal_fires() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path();
        // systemd-style rootfs: no init.d, no usr/include, no env-script.
        mkdir(&target.join("usr").join("bin"));
        let ctx = detect_scan_context(target);
        assert!(matches!(ctx, ScanContext::Rootfs));
    }

    #[test]
    fn ambiguous_when_primary_fires_but_init_d_present() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path();
        touch(&target.join("environment-setup-mikebom-fixture"));
        mkdir(&target.join("etc").join("init.d"));
        // /usr/include intentionally absent so the secondary check
        // (include AND no init.d) fails — conflicting with the primary.
        let ctx = detect_scan_context(target);
        assert!(matches!(ctx, ScanContext::AmbiguousSysroot { .. }));
        assert!(ctx.ambiguity_reason().unwrap().contains("init.d=true"));
    }

    #[test]
    fn applies_build_scope_helper_covers_sysroot_and_ambiguous() {
        assert!(ScanContext::Sysroot {
            primary_signal: true,
            secondary_signal: false
        }
        .applies_build_scope());
        assert!(ScanContext::AmbiguousSysroot {
            reason: "test".into()
        }
        .applies_build_scope());
        assert!(!ScanContext::Rootfs.applies_build_scope());
    }
}
