//! Spec 1653 US4 (T032, FR-010): a CMake file-API reply, when the build
//! has one.
//!
//! A reply exists only where a query was placed under
//! `<build>/.cmake/api/v1/query/` before configure (research R6: 6/6
//! probe runs with a query, 0/18 without). waybill never writes one. The
//! newest `reply/index-*.json` names the `cache-v2` and `codemodel-v2`
//! objects; each codemodel target names its own JSON file, whose
//! `compileGroups[].includes[].path` and `sources[]` give the same
//! per-unit shape `compile_commands.json` does ([`Unit`]).

use std::path::{Path, PathBuf};

use super::cache::{CacheKind, CacheView};
use super::compile_db::{absolute, fragment_include_dirs, is_windows_dir, Unit};

/// What a reply supplies. Each part is `None` when the index names no
/// such object, and the caller falls back to the file it replaces.
#[derive(Debug, Default)]
pub(crate) struct FileApiReply {
    pub(crate) cache: Option<CacheView>,
    pub(crate) units: Option<Vec<Unit>>,
}

#[derive(Debug)]
pub(crate) enum Loaded {
    /// No reply directory, no index, or an index naming neither object.
    Absent,
    Read(FileApiReply),
    /// A reply file could not be read or parsed: none of it is used.
    Malformed,
}

pub(crate) fn load(build_root: &Path) -> Loaded {
    let dir = build_root.join(".cmake/api/v1/reply");
    // Only a reply directory that isn't there is absent. One that exists but
    // can't be listed is a reply that could not be read (#1735 review).
    let rd = match std::fs::read_dir(&dir) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Loaded::Absent,
        Err(_) => return Loaded::Malformed,
    };
    // Index names carry a sortable timestamp: the greatest is the newest.
    let index = rd
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.starts_with("index-") && n.ends_with(".json"))
        .max();
    let Some(index) = index else { return Loaded::Absent };
    match read(&dir, &index) {
        Ok(Some(r)) => Loaded::Read(r),
        Ok(None) => Loaded::Absent,
        Err(Malformed) => Loaded::Malformed,
    }
}

struct Malformed;

fn json(dir: &Path, name: &str) -> Result<serde_json::Value, Malformed> {
    // A reply names files in its own directory; anything else is not one.
    if name.is_empty() || name.contains(['/', '\\']) || name == ".." {
        return Err(Malformed);
    }
    let text = std::fs::read_to_string(dir.join(name)).map_err(|_| Malformed)?;
    serde_json::from_str(&text).map_err(|_| Malformed)
}

/// The `jsonFile` of the index object of `kind` at major version 2.
fn object<'a>(index: &'a serde_json::Value, kind: &str) -> Option<&'a str> {
    index["objects"]
        .as_array()?
        .iter()
        .find(|o| o["kind"] == kind && o["version"]["major"] == 2)
        .and_then(|o| o["jsonFile"].as_str())
}

fn read(dir: &Path, index_name: &str) -> Result<Option<FileApiReply>, Malformed> {
    let index = json(dir, index_name)?;
    let (cache, codemodel) = (object(&index, "cache"), object(&index, "codemodel"));
    if cache.is_none() && codemodel.is_none() {
        return Ok(None);
    }
    let mut reply = FileApiReply::default();
    if let Some(name) = cache {
        reply.cache = Some(cache_view(&json(dir, name)?)?);
    }
    if let Some(name) = codemodel {
        reply.units = Some(units(dir, &json(dir, name)?)?);
    }
    Ok(Some(reply))
}

/// `cache-v2`'s `entries[]` as a [`CacheView`].
fn cache_view(v: &serde_json::Value) -> Result<CacheView, Malformed> {
    // An entry without a string name and value is a reply CMake did not
    // write: the whole reply is malformed rather than silently partial
    // (#1735 review), so the intact `CMakeCache.txt` is used instead.
    let mut entries = Vec::new();
    for e in v["entries"].as_array().ok_or(Malformed)? {
        let (Some(name), Some(value)) = (e["name"].as_str(), e["value"].as_str()) else { return Err(Malformed) };
        let help = e["properties"]
            .as_array()
            .and_then(|ps| ps.iter().find(|p| p["name"] == "HELPSTRING"))
            .and_then(|p| p["value"].as_str())
            .map(str::to_string);
        entries.push((name.to_string(), CacheKind::parse(e["type"].as_str().unwrap_or("")), value.to_string(), help));
    }
    CacheView::from_entries(entries).map_err(|_| Malformed)
}

/// Every target's sources with their compile group's include directories,
/// over every configuration. Source paths are relative to the top source
/// directory unless absolute.
fn units(dir: &Path, codemodel: &serde_json::Value) -> Result<Vec<Unit>, Malformed> {
    let source = PathBuf::from(codemodel["paths"]["source"].as_str().ok_or(Malformed)?);
    let build = codemodel["paths"]["build"].as_str().map(PathBuf::from).unwrap_or_else(|| source.clone());
    let windows = is_windows_dir(&source);
    let mut out: Vec<Unit> = Vec::new();
    for cfg in codemodel["configurations"].as_array().ok_or(Malformed)? {
        for t in cfg["targets"].as_array().into_iter().flatten() {
            let target = json(dir, t["jsonFile"].as_str().ok_or(Malformed)?)?;
            let groups: Vec<Vec<PathBuf>> = target["compileGroups"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|g| {
                    let mut inc: Vec<PathBuf> =
                        g["includes"].as_array().into_iter().flatten().filter_map(|i| i["path"].as_str()).map(|p| absolute(&source, p)).collect();
                    // Flags-passed includes, relative to the build directory
                    // as in a compile command (#1735 review).
                    let frags = g["compileCommandFragments"].as_array().into_iter().flatten().filter_map(|f| f["fragment"].as_str());
                    for d in fragment_include_dirs(frags, windows) {
                        let p = absolute(&build, &d);
                        if !inc.contains(&p) {
                            inc.push(p);
                        }
                    }
                    inc
                })
                .collect();
            for s in target["sources"].as_array().into_iter().flatten() {
                let (Some(path), Some(g)) = (s["path"].as_str(), s["compileGroupIndex"].as_u64()) else { continue };
                let Some(includes) = usize::try_from(g).ok().and_then(|g| groups.get(g)) else { continue };
                out.push(Unit { file: absolute(&source, path), includes: includes.clone() });
            }
        }
    }
    out.sort_by(|a, b| a.file.cmp(&b.file).then_with(|| a.includes.cmp(&b.includes)));
    out.dedup();
    Ok(out)
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    fn write(dir: &Path, name: &str, v: serde_json::Value) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join(name), v.to_string()).unwrap();
    }

    /// The probe's shapes (FX/system's reply, CMake 3.31.6), cut down.
    fn reply(b: &Path) -> PathBuf {
        let r = b.join(".cmake/api/v1/reply");
        write(&r, "index-2026-01-01T00-00-00-0000.json", serde_json::json!({"objects": [
            {"kind": "codemodel", "version": {"major": 2, "minor": 7}, "jsonFile": "codemodel-v2-x.json"},
            {"kind": "cache", "version": {"major": 2, "minor": 0}, "jsonFile": "cache-v2-x.json"},
            {"kind": "toolchains", "version": {"major": 1, "minor": 0}, "jsonFile": "toolchains-v1-x.json"}]}));
        write(&r, "cache-v2-x.json", serde_json::json!({"entries": [
            {"name": "BACKEND", "type": "STRING", "value": "B"},
            {"name": "CMAKE_HOME_DIRECTORY", "type": "INTERNAL", "value": "/src"}]}));
        write(&r, "codemodel-v2-x.json", serde_json::json!({"paths": {"source": "/src", "build": "/b"},
            "configurations": [{"targets": [{"name": "app", "jsonFile": "target-app-x.json"}]}]}));
        write(&r, "target-app-x.json", serde_json::json!({
            "compileGroups": [{"includes": [{"path": "/b/_deps/urldep-src"}, {"path": "/p/include", "isSystem": true}]}],
            "sources": [{"path": "main.cpp", "compileGroupIndex": 0}, {"path": "README.md"}]}));
        r
    }

    #[test]
    fn a_reply_gives_the_cache_and_the_units() {
        let d = tempfile::tempdir().unwrap();
        reply(d.path());
        let Loaded::Read(r) = load(d.path()) else { panic!("not read") };
        let c = r.cache.unwrap();
        assert_eq!(c.get("BACKEND").unwrap().value, "B");
        assert_eq!(c.get("BACKEND").unwrap().kind, CacheKind::String);
        assert_eq!(c.home(), Some("/src"));
        assert_eq!(
            r.units.unwrap(),
            [Unit { file: "/src/main.cpp".into(), includes: vec!["/b/_deps/urldep-src".into(), "/p/include".into()] }]
        );
    }

    /// #1735 review: flags-passed includes in `compileCommandFragments`,
    /// relative ones against the build directory, both quoting styles.
    #[test]
    fn fragment_includes_are_read() {
        let d = tempfile::tempdir().unwrap();
        let r = reply(d.path());
        write(&r, "target-app-x.json", serde_json::json!({
            "compileGroups": [{"includes": [{"path": "/b/_deps/urldep-src"}],
                "compileCommandFragments": [{"fragment": "-O2"}, {"fragment": "-I/dep/include -isystem rel/inc"}, {"fragment": "-I\"/sp ace\""}]}],
            "sources": [{"path": "main.cpp", "compileGroupIndex": 0}]}));
        let Loaded::Read(rep) = load(d.path()) else { panic!("not read") };
        assert_eq!(
            rep.units.unwrap()[0].includes,
            vec![PathBuf::from("/b/_deps/urldep-src"), "/dep/include".into(), "/b/rel/inc".into(), "/sp ace".into()]
        );
    }

    /// #1735 review: an entry CMake would never write makes the reply
    /// malformed, not silently partial.
    #[test]
    fn a_cache_entry_without_a_string_value_is_malformed() {
        let d = tempfile::tempdir().unwrap();
        let r = reply(d.path());
        // One intact entry beside it, so the reply is not malformed merely
        // for being empty once the bad entry is dropped.
        write(&r, "cache-v2-x.json", serde_json::json!({"entries": [
            {"name": "CMAKE_HOME_DIRECTORY", "type": "INTERNAL", "value": "/src"},
            {"name": "foo_DIR", "value": null}]}));
        assert!(matches!(load(d.path()), Loaded::Malformed));
    }

    /// #1735 review: a reply directory that exists but can't be listed is
    /// not absent.
    #[cfg(unix)]
    #[test]
    fn an_unlistable_reply_directory_is_malformed() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        let r = reply(d.path());
        std::fs::set_permissions(&r, std::fs::Permissions::from_mode(0o000)).unwrap();
        let got = load(d.path());
        std::fs::set_permissions(&r, std::fs::Permissions::from_mode(0o755)).unwrap();
        // Root can list it anyway; only assert where the permission held.
        if std::fs::read_dir("/root").is_err() {
            assert!(matches!(got, Loaded::Malformed), "{got:?}");
        }
    }

    #[test]
    fn the_newest_index_is_read() {
        let d = tempfile::tempdir().unwrap();
        let r = reply(d.path());
        write(&r, "index-2025-01-01T00-00-00-0000.json", serde_json::json!({"objects": []}));
        assert!(matches!(load(d.path()), Loaded::Read(_)));
        write(&r, "index-2027-01-01T00-00-00-0000.json", serde_json::json!({"objects": []}));
        assert!(matches!(load(d.path()), Loaded::Absent), "the newest names nothing");
    }

    #[test]
    fn absent_malformed_and_escaping_replies() {
        let d = tempfile::tempdir().unwrap();
        assert!(matches!(load(d.path()), Loaded::Absent));
        let r = reply(d.path());
        std::fs::write(r.join("target-app-x.json"), "{").unwrap();
        assert!(matches!(load(d.path()), Loaded::Malformed));
        write(&r, "codemodel-v2-x.json", serde_json::json!({"paths": {"source": "/src"},
            "configurations": [{"targets": [{"jsonFile": "../../../../etc/passwd"}]}]}));
        assert!(matches!(load(d.path()), Loaded::Malformed), "a reply names only its own files");
    }
}
