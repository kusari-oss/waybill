//! CMake exported-targets files (spec 1532, #1563; research R3).
//!
//! `install(EXPORT)` writes a main file (`<Pkg>Targets.cmake`,
//! `<pkg>-targets.cmake`, `<Pkg>Exports.cmake`) that sets `_IMPORT_PREFIX` to
//! the file's directory raised N times, and one file per configuration
//! (`…-<config>.cmake`) whose `set_target_properties` name each target's
//! installed file in `IMPORTED_LOCATION[_<CONFIG>]`. Both are read
//! textually; nothing is evaluated.

use super::cmake_version::{commands, strip_comments};

/// Which file of an export set this is.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Role {
    Main,
    Config(String),
}

/// The role a file name selects, or `None` when it is not an
/// exported-targets file.
pub(crate) fn classify(file_name: &str) -> Option<Role> {
    classify_with_base(file_name).map(|(role, _)| role)
}

/// The role and the package name the file is named for (`elfioTargets.cmake`
/// → `elfio`). The package name feeds the CMake search-location rules, which
/// accept `<prefix>/(lib*|share)/<name>*/cmake/`.
pub(crate) fn classify_with_base(file_name: &str) -> Option<(Role, &str)> {
    let stem = file_name.strip_suffix(".cmake")?;
    for marker in ["Targets", "Exports", "-targets"] {
        if let Some(base) = stem.strip_suffix(marker) {
            return (!base.is_empty() && !base.ends_with('-')).then_some((Role::Main, base));
        }
    }
    for marker in ["Targets-", "Exports-", "-targets-"] {
        if let Some(i) = stem.rfind(marker) {
            let (base, cfg) = (&stem[..i], &stem[i + marker.len()..]);
            if !base.is_empty() && !cfg.is_empty() && cfg.chars().all(|c| c.is_ascii_alphanumeric()) {
                return Some((Role::Config(cfg.to_ascii_lowercase()), base));
            }
        }
    }
    None
}

/// What one exported-targets file says.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Parsed {
    /// `Some(n)` when the file computes `_IMPORT_PREFIX` from its own
    /// location (`get_filename_component(_IMPORT_PREFIX "${CMAKE_CURRENT_LIST_FILE}" PATH)`)
    /// and raises it `n` times. `None` otherwise.
    pub(crate) raise: Option<usize>,
    /// Raw `IMPORTED_LOCATION` / `IMPORTED_LOCATION_<CONFIG>` values, in
    /// file order.
    pub(crate) locations: Vec<String>,
}

pub(crate) fn parse(content: &str) -> Parsed {
    let text = strip_comments(content);
    let mut out = Parsed::default();
    let mut based = false;
    let mut raises = 0usize;
    for (command, args) in commands(&text) {
        let command = command.to_ascii_lowercase();
        let a: Vec<&str> = args.iter().map(|x| x.text.as_str()).collect();
        match command.as_str() {
            "get_filename_component" if a.first() == Some(&"_IMPORT_PREFIX") && a.get(2) == Some(&"PATH") => {
                match a.get(1).copied() {
                    Some("${CMAKE_CURRENT_LIST_FILE}") => {
                        based = true;
                        raises = 0;
                    }
                    Some("${_IMPORT_PREFIX}") if based => raises += 1,
                    _ => {}
                }
            }
            "set_target_properties" => {
                let mut it = a.iter().skip_while(|x| **x != "PROPERTIES").skip(1);
                while let Some(key) = it.next() {
                    let Some(value) = it.next() else { break };
                    if *key == "IMPORTED_LOCATION" || key.starts_with("IMPORTED_LOCATION_") {
                        out.locations.push((*value).to_string());
                    }
                }
            }
            _ => {}
        }
    }
    if based {
        out.raise = Some(raises);
    }
    out
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    const MAIN: &str = include_str!("testdata/WaybillFixturePTargets.cmake.txt");
    const RELEASE: &str = include_str!("testdata/WaybillFixturePTargets-release.cmake.txt");

    #[test]
    fn classify_roles() {
        assert_eq!(classify("WaybillFixturePTargets.cmake"), Some(Role::Main));
        assert_eq!(classify("waybill-fixture-q-targets.cmake"), Some(Role::Main));
        assert_eq!(classify("LLVMExports.cmake"), Some(Role::Main));
        assert_eq!(classify("WaybillFixturePTargets-release.cmake"), Some(Role::Config("release".into())));
        assert_eq!(classify("waybill-fixture-q-targets-release.cmake"), Some(Role::Config("release".into())));
        assert_eq!(classify("LLVMExports-RelWithDebInfo.cmake"), Some(Role::Config("relwithdebinfo".into())));
        assert_eq!(classify_with_base("elfioTargets.cmake").map(|x| x.1), Some("elfio"));
        assert_eq!(classify_with_base("waybill-fixture-q-targets-release.cmake").map(|x| x.1), Some("waybill-fixture-q"));
        for n in ["FooTargets.cmake.in", "targets.cmake", "-targets.cmake", "FooConfig.cmake",
                  "FooConfigVersion.cmake", "Targets.cmake", "FooTargets-.cmake", "foo-targets-a.b.cmake"] {
            assert_eq!(classify(n), None, "{n}");
        }
    }

    /// A main file CMake 4.4.5 generated: three raises.
    #[test]
    fn a_generated_main_file() {
        let p = parse(MAIN);
        assert_eq!(p.raise, Some(3));
        assert!(p.locations.is_empty());
    }

    /// The matching release file: static archive, shared library, executable.
    #[test]
    fn a_generated_release_file() {
        let p = parse(RELEASE);
        assert_eq!(p.raise, None);
        assert_eq!(p.locations.len(), 3, "{:?}", p.locations);
        assert!(p.locations.iter().all(|l| l.starts_with("${_IMPORT_PREFIX}/")));
        assert!(p.locations[0].ends_with("/lib/libops.a"));
        assert!(p.locations[2].ends_with("/bin/tool"));
    }

    #[test]
    fn every_configuration_location_is_read() {
        let p = parse(
            "set_target_properties(x::y PROPERTIES\n  IMPORTED_LOCATION \"${_IMPORT_PREFIX}/lib/a.so\"\n  IMPORTED_LOCATION_DEBUG \"${_IMPORT_PREFIX}/lib/a_d.so\"\n  IMPORTED_SONAME_DEBUG \"liba_d.so\"\n  IMPORTED_IMPLIB_RELEASE \"${_IMPORT_PREFIX}/lib/a.lib\"\n  INTERFACE_INCLUDE_DIRECTORIES \"${_IMPORT_PREFIX}/include\"\n  )\n",
        );
        assert_eq!(p.locations, vec!["${_IMPORT_PREFIX}/lib/a.so", "${_IMPORT_PREFIX}/lib/a_d.so"]);
    }

    #[test]
    fn comments_and_truncation() {
        let p = parse("# set_target_properties(x PROPERTIES IMPORTED_LOCATION \"/x\")\nset_target_properties(y PROPERTIES IMPORTED_LOCATION \"${_IMPORT_PREFIX}/y\")\nset_target_properties(z PROPERTIES IMPORTED_LOC");
        assert_eq!(p.locations, vec!["${_IMPORT_PREFIX}/y"]);
    }

    #[test]
    fn raises_need_a_base() {
        // Raises without the base line do not define a prefix.
        let p = parse("get_filename_component(_IMPORT_PREFIX \"${_IMPORT_PREFIX}\" PATH)\n");
        assert_eq!(p.raise, None);
        let p = parse("get_filename_component(_IMPORT_PREFIX \"${CMAKE_CURRENT_LIST_FILE}\" PATH)\n");
        assert_eq!(p.raise, Some(0));
    }
}
