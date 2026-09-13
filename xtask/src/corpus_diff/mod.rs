// Feature 840 (issue #763) — review-time normaliser for public-corpus
// golden diffs.
//
// The goldens were last written on 2026-07-21 and roughly 147 merges
// have landed since. The raw diff is therefore enormous and mostly
// benign, which is exactly the condition under which a real regression
// hides. This tool exists so a human can read that diff.
//
// It is NOT part of the gate. `waybill-cli/tests/corpus_harness_195/`
// keeps byte-identity comparison as the authority. Two consequences
// follow, both enforced below and in tests:
//
//   * this never writes to a golden. A normaliser that wrote back would
//     make a reordered-but-equal golden compare EQUAL, silently
//     weakening the very gate it serves (contract C-2.3).
//   * this never re-applies the harness's masking. Goldens are stored
//     already masked (`layer2_golden.rs:51`); masking twice risks
//     diverging from what the lane actually compares (C-2.4).
//
// Masking substitutes values; it cannot reorder. Array ordering is the
// one category masking cannot close, and SPDX 3 wraps its output in a
// `@graph` array whose order is not stable across runs. Left alone, a
// reordered array presents as every element changing and buries real
// drift underneath the volume. Closing that gap is this tool's whole job.

use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::Command;

use clap::Args;
use serde_json::Value;

#[cfg(test)]
mod tests;

#[derive(Args, Debug)]
pub struct CorpusDiffArgs {
    /// Old golden. Mutually exclusive with `--target`.
    #[arg(long, conflicts_with = "target")]
    pub old: Option<PathBuf>,

    /// New golden. Used with `--old`.
    #[arg(long, requires = "old")]
    pub new: Option<PathBuf>,

    /// Corpus target name; compares its committed goldens against `--old-ref`.
    #[arg(long, conflicts_with = "old")]
    pub target: Option<String>,

    /// Git ref to compare the target's goldens against. Defaults to HEAD.
    #[arg(long, default_value = "HEAD")]
    pub old_ref: String,

    /// Restrict to one format. Default: all three.
    #[arg(long, value_parser = ["cdx", "spdx-2.3", "spdx-3"])]
    pub format: Option<String>,
}

const FORMATS: [&str; 3] = ["cdx", "spdx-2.3", "spdx-3"];

/// Contract C-1.2: exit 0 whether or not differences exist. This is a
/// reading tool, not a gate — a non-zero exit would invite someone to
/// wire it into CI as a second gate, which it must not become (C-4.1).
pub fn run(args: CorpusDiffArgs) -> Result<(), Box<dyn Error>> {
    match (&args.old, &args.target) {
        (Some(old), _) => {
            let new = args
                .new
                .as_ref()
                .ok_or_else(|| -> Box<dyn Error> { "--old requires --new".into() })?;
            let label = args.format.clone().unwrap_or_else(|| "file".to_string());
            report(&label, &read_json(old)?, &read_json(new)?);
            Ok(())
        }
        (None, Some(target)) => diff_target(target, &args.old_ref, args.format.as_deref()),
        (None, None) => Err("pass either --old/--new or --target".into()),
    }
}

fn diff_target(
    target: &str,
    old_ref: &str,
    only: Option<&str>,
) -> Result<(), Box<dyn Error>> {
    let formats: Vec<&str> = match only {
        Some(f) => vec![f],
        None => FORMATS.to_vec(),
    };
    for format in formats {
        let path = golden_path(target, format);
        if !path.exists() {
            return Err(format!(
                "no golden at {} — is `{target}` a corpus target?",
                path.display()
            )
            .into());
        }
        let new = read_json(&path)?;
        let old = read_json_at_ref(old_ref, &path)?;
        // C-3.1: identify target and format, so a diff pasted into a PR
        // is attributable without its invocation.
        report(&format!("{target} / {format}"), &old, &new);
    }
    Ok(())
}

fn golden_path(target: &str, format: &str) -> PathBuf {
    PathBuf::from("waybill-cli/tests/fixtures/public_corpus")
        .join(target)
        .join(format!("{format}.json"))
}

fn read_json(path: &Path) -> Result<Value, Box<dyn Error>> {
    let bytes = std::fs::read(path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    serde_json::from_slice(&bytes)
        .map_err(|e| format!("{} is not valid JSON: {e}", path.display()).into())
}

fn read_json_at_ref(git_ref: &str, path: &Path) -> Result<Value, Box<dyn Error>> {
    let spec = format!("{git_ref}:{}", path.display());
    let out = Command::new("git")
        .args(["show", &spec])
        .output()
        .map_err(|e| format!("git show {spec}: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!("git show {spec}: {}", err.lines().next().unwrap_or("failed")).into());
    }
    serde_json::from_slice(&out.stdout)
        .map_err(|e| format!("{spec} is not valid JSON: {e}").into())
}

/// Sort every array in the document by a stable key derived from its
/// contents, recursively.
///
/// Contract C-2.5: the key must be total and deterministic. Sorting by
/// the canonical serialisation of each element gives that for free — two
/// elements comparing equal are byte-identical, so ties cannot leave
/// order input-dependent.
///
/// Applied to BOTH sides identically (C-2.2). Asymmetric normalisation
/// manufactures differences.
pub fn normalise(value: &Value) -> Value {
    match value {
        Value::Array(items) => {
            let mut normalised: Vec<Value> = items.iter().map(normalise).collect();
            normalised.sort_by_cached_key(canonical_key);
            Value::Array(normalised)
        }
        Value::Object(map) => {
            // serde_json's Map preserves insertion order unless the
            // `preserve_order` feature is off; normalising the values is
            // enough because key order does not survive canonical
            // serialisation comparison below.
            let mut out = serde_json::Map::new();
            for (k, v) in map {
                out.insert(k.clone(), normalise(v));
            }
            Value::Object(out)
        }
        other => other.clone(),
    }
}

fn canonical_key(v: &Value) -> String {
    serde_json::to_string(v).unwrap_or_default()
}

/// Print a normalised diff. Empty output means the two documents differ
/// only in ways this tool considers non-semantic.
fn report(label: &str, old: &Value, new: &Value) {
    let old_n = normalise(old);
    let new_n = normalise(new);
    if old_n == new_n {
        println!("{label}: no semantic change after normalisation");
        return;
    }
    println!("=== {label} ===");
    let mut lines = Vec::new();
    walk(&old_n, &new_n, String::from("$"), &mut lines);
    // C-3.2: group repeated shapes so a reviewer sees "N components
    // gained X" rather than N separate hunks. C-3.3: anything matching
    // no group is shown individually and never summarised away — those
    // are the FR-007 cases, the entire point of the exercise.
    let (grouped, singles) = group(lines);
    for (shape, count) in grouped {
        println!("  [{count}x] {shape}");
    }
    for line in singles {
        println!("  {line}");
    }
}

fn walk(old: &Value, new: &Value, path: String, out: &mut Vec<String>) {
    match (old, new) {
        (Value::Object(a), Value::Object(b)) => {
            let mut keys: Vec<&String> = a.keys().chain(b.keys()).collect();
            keys.sort();
            keys.dedup();
            for k in keys {
                match (a.get(k), b.get(k)) {
                    (Some(x), Some(y)) => walk(x, y, format!("{path}.{k}"), out),
                    (Some(_), None) => out.push(format!("removed {path}.{k}")),
                    (None, Some(_)) => out.push(format!("added {path}.{k}")),
                    (None, None) => unreachable!(),
                }
            }
        }
        (Value::Array(a), Value::Array(b)) if a.len() == b.len() => {
            for (i, (x, y)) in a.iter().zip(b).enumerate() {
                walk(x, y, format!("{path}[{i}]"), out);
            }
        }
        // Lengths differ. Positional pairing is meaningless here, and
        // the catch-all below would collapse the whole array into one
        // `changed $.path` line — which is what made SPDX 3 `@graph`
        // unreviewable for every target that gained or lost an element
        // (7 of 11 in the milestone-840 refresh; image-postgres16
        // reported a 5036 -> 6315 change as a single line).
        (Value::Array(a), Value::Array(b)) => align_by_identity(a, b, &path, out),
        (a, b) if a != b => out.push(format!("changed {path}")),
        _ => {}
    }
}

/// Collapse repeated shapes. A shape is the path with array indices
/// stripped, so `$.components[3].licenses` and `$.components[91].licenses`
/// are one shape.
///
/// A change appearing exactly once is NOT a shape — it is returned
/// individually. Folding singletons into a group is how a regression
/// gets absorbed into a large benign diff (research.md R4).
fn group(lines: Vec<String>) -> (Vec<(String, usize)>, Vec<String>) {
    use std::collections::BTreeMap;
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut first: BTreeMap<String, String> = BTreeMap::new();
    for line in &lines {
        let shape = strip_indices(line);
        *counts.entry(shape.clone()).or_insert(0) += 1;
        first.entry(shape).or_insert_with(|| line.clone());
    }
    let mut grouped = Vec::new();
    let mut singles = Vec::new();
    for (shape, count) in counts {
        if count > 1 {
            grouped.push((shape, count));
        } else {
            singles.push(first[&shape].clone());
        }
    }
    grouped.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    (grouped, singles)
}

/// A stable identity for an array element, used to pair elements across
/// two documents when the arrays differ in length.
///
/// The key MUST NOT be derived from a content-addressed field.
/// `spdxId`, `SPDXID` and file-tier `bom-ref`s are hashes of the very
/// content whose change we are trying to describe: keying on them pairs
/// nothing, and every element reports as both added and removed. That
/// is the failure this function exists to avoid, so the exclusions are
/// deliberate rather than incidental.
///
/// Scalars are their own identity. Equal scalars then pair silently and
/// only genuinely added or removed values produce a line.
fn identity_key(v: &Value) -> Option<String> {
    let obj = match v.as_object() {
        Some(o) => o,
        // Scalar (or array) element: its canonical form is its identity.
        None => return Some(sanitise_key(&canonical_key(v))),
    };
    let get = |k: &str| obj.get(k).and_then(|x| x.as_str());

    // A PURL is a semantic identity that already includes the version,
    // so a version change correctly reads as one element replacing
    // another rather than as a field edit.
    if let Some(p) = get("purl").or_else(|| get("packageUrl")) {
        return Some(sanitise_key(&format!("purl={p}")));
    }
    if let Some(n) = get("name") {
        let ver = get("versionInfo")
            .or_else(|| get("software_packageVersion"))
            .or_else(|| get("version"))
            .unwrap_or("");
        return Some(sanitise_key(&format!("name={n}@{ver}")));
    }
    if let Some(r) = get("bom-ref") {
        if !is_content_addressed(r) {
            return Some(sanitise_key(&format!("ref={r}")));
        }
    }
    // Weakest useful key: elements of the same type bucket together, so
    // relationships (which carry no stable name) still pair pairwise
    // within their bucket instead of collapsing the whole array.
    if let Some(t) = get("type").or_else(|| get("@type")) {
        return Some(sanitise_key(&format!("type={t}")));
    }
    None
}

fn is_content_addressed(s: &str) -> bool {
    s.contains("content-sha256") || s.starts_with("SPDXRef-")
}

/// `strip_indices` treats everything between `[` and `]` as an index to
/// erase, which is what lets repeated shapes group. An identity key
/// containing a bracket would truncate the shape and split one group in
/// two, so brackets are folded out of the key.
fn sanitise_key(s: &str) -> String {
    s.replace(['[', ']'], "_")
}

/// Pair two differently-sized arrays by element identity, recursing into
/// matched pairs and reporting the remainder as added or removed.
///
/// Surplus elements sharing a key are reported one line each rather than
/// as a count, so `group` can collapse them into a counted shape and a
/// lone survivor still shows up individually (C-3.3).
fn align_by_identity(a: &[Value], b: &[Value], path: &str, out: &mut Vec<String>) {
    use std::collections::BTreeMap;

    let mut old_by: BTreeMap<String, Vec<&Value>> = BTreeMap::new();
    let mut new_by: BTreeMap<String, Vec<&Value>> = BTreeMap::new();
    let mut old_unkeyed = 0usize;
    let mut new_unkeyed = 0usize;

    for v in a {
        match identity_key(v) {
            Some(k) => old_by.entry(k).or_default().push(v),
            None => old_unkeyed += 1,
        }
    }
    for v in b {
        match identity_key(v) {
            Some(k) => new_by.entry(k).or_default().push(v),
            None => new_unkeyed += 1,
        }
    }

    let mut keys: Vec<String> = old_by.keys().chain(new_by.keys()).cloned().collect();
    keys.sort();
    keys.dedup();

    let empty: Vec<&Value> = Vec::new();
    for k in keys {
        let o = old_by.get(&k).unwrap_or(&empty);
        let n = new_by.get(&k).unwrap_or(&empty);
        let common = o.len().min(n.len());
        for i in 0..common {
            walk(o[i], n[i], format!("{path}[{k}]"), out);
        }
        for _ in common..o.len() {
            out.push(format!("removed {path}[{k}]"));
        }
        for _ in common..n.len() {
            out.push(format!("added {path}[{k}]"));
        }
    }

    // Elements no rule could key. Reported rather than dropped: a
    // silently ignored element is the failure mode this whole tool
    // exists to prevent.
    if old_unkeyed != new_unkeyed {
        out.push(format!(
            "changed {path} (unkeyable elements {old_unkeyed} -> {new_unkeyed})"
        ));
    }
}

fn strip_indices(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_index = false;
    for c in s.chars() {
        match c {
            '[' => {
                in_index = true;
                out.push_str("[]");
            }
            ']' => in_index = false,
            _ if in_index => {}
            _ => out.push(c),
        }
    }
    out
}
