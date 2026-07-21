//! `mikebom fingerprints` — operator subcommands for the external
//! symbol-fingerprint corpus (milestone 108 US4 / FR-008 / FR-009).
//!
//! Three subcommands:
//!
//! - `fetch [--corpus-rev <SHA>]` — explicit one-shot fetch. The only
//!   subcommand in mikebom-cli that's REQUIRED to perform a network
//!   call. Air-gapped operators run this on an internet-connected
//!   machine, tar the cache, ship it, and untar on the air-gapped
//!   destination.
//! - `cache-clear [--keep-rev <SHA>]` — purely local cleanup. Removes
//!   every cached SHA directory; optionally preserves one.
//! - `list` — purely local introspection. Enumerates cached corpora
//!   with record counts + mtimes.
//!
//! Per `contracts/cli-surface.md` — exit-code table:
//!   0 success
//!   1 invalid argument (malformed SHA, etc.)
//!   2 network error (DNS, connection, 5xx after retries)
//!   3 HTTP 404 (SHA doesn't exist in the corpus repo)
//!   4 disk-write error
//!  10 other / uncategorized

use std::path::Path;
use std::process::ExitCode;

use anyhow::Context;
use clap::{Args, Subcommand};

use crate::scan_fs::binary::fingerprints::{cache, fetch, CorpusSha};
use crate::scan_fs::binary::fingerprints::cache::KeepRev;

#[derive(Args, Debug)]
pub struct FingerprintsCommand {
    #[command(subcommand)]
    pub command: FingerprintsSubcommand,
}

#[derive(Subcommand, Debug)]
pub enum FingerprintsSubcommand {
    /// Fetch the external corpus tarball + extract into the per-host
    /// cache. The only mikebom subcommand REQUIRED to perform a network
    /// call.
    ///
    /// Example: pre-fetch for an air-gapped operator.
    ///
    ///   $ mikebom fingerprints fetch
    ///   fetched: fff39c6ad22ce8420b506323ce1d5cce4b628d5c → /home/user/.cache/mikebom/fingerprints/fff39c6.../
    ///
    /// Idempotent — running twice against the same SHA short-circuits
    /// with a `cache hit:` message and exits 0.
    Fetch(FingerprintsFetchArgs),
    /// Remove cached corpus directories. Purely local; no network.
    ///
    /// Example: drop every cached SHA except the build-time-pinned one.
    ///
    ///   $ mikebom fingerprints cache-clear --keep-rev fff39c6ad22ce8420b506323ce1d5cce4b628d5c
    ///   removed: /home/user/.cache/mikebom/fingerprints/<other-sha>/
    ///
    /// Idempotent — running against an already-empty cache exits 0
    /// with no output.
    #[command(name = "cache-clear")]
    CacheClear(FingerprintsCacheClearArgs),
    /// List cached corpora (full SHA + record count + mtime).
    /// Purely local; no network.
    ///
    ///   $ mikebom fingerprints list
    ///   fff39c6ad22ce8420b506323ce1d5cce4b628d5c  7  2026-06-02T17:30:55Z
    List,
}

#[derive(Args, Debug)]
pub struct FingerprintsFetchArgs {
    /// Override the build-time-embedded corpus SHA. 40-char lowercase
    /// hex. When omitted, the build-time-pinned SHA from
    /// `tests/fingerprints.rev` is used.
    #[arg(long = "corpus-rev", value_name = "SHA")]
    pub corpus_rev: Option<String>,
}

#[derive(Args, Debug)]
pub struct FingerprintsCacheClearArgs {
    /// Preserve the cache directory for this specific SHA; remove all
    /// others. 40-char lowercase hex.
    #[arg(long = "keep-rev", value_name = "SHA")]
    pub keep_rev: Option<String>,
}

/// Top-level entry point. Returns the appropriate categorized
/// `ExitCode` per `contracts/cli-surface.md`'s exit-code table.
pub async fn execute(cmd: FingerprintsCommand) -> anyhow::Result<ExitCode> {
    match cmd.command {
        FingerprintsSubcommand::Fetch(args) => run_fetch(args),
        FingerprintsSubcommand::CacheClear(args) => run_cache_clear(args),
        FingerprintsSubcommand::List => run_list(),
    }
}

fn parse_sha_or_invalid(s: &str) -> anyhow::Result<CorpusSha> {
    CorpusSha::from_hex(s).with_context(|| {
        format!(
            "invalid SHA `{s}`: must be 40-char lowercase hex (e.g. fff39c6ad22ce8420b506323ce1d5cce4b628d5c)"
        )
    })
}

fn run_fetch(args: FingerprintsFetchArgs) -> anyhow::Result<ExitCode> {
    let sha = match args.corpus_rev {
        Some(s) => match parse_sha_or_invalid(&s) {
            Ok(sha) => sha,
            Err(e) => {
                eprintln!("error: {e}");
                return Ok(ExitCode::from(1));
            }
        },
        None => CorpusSha::build_time_embedded(),
    };

    let cache_path = cache::cache_dir_for_sha(&sha);

    if cache::cache_hit(&sha) {
        println!("cache hit: {}", sha.to_full_hex());
        return Ok(ExitCode::from(0));
    }

    match fetch::fetch_corpus(&sha) {
        Ok(()) => {
            println!(
                "fetched: {} → {}",
                sha.to_full_hex(),
                cache_path.display()
            );
            Ok(ExitCode::from(0))
        }
        Err(e) => {
            // Categorize the error per the exit-code table.
            let (code, label) = match &e {
                fetch::FetchError::NotFound { .. } => (3, "404"),
                fetch::FetchError::Network(_) => (2, "network"),
                fetch::FetchError::HttpError { status, .. } if *status == 404 => {
                    (3, "404")
                }
                fetch::FetchError::HttpError { .. } => (2, "network"),
                fetch::FetchError::Decompression(_) | fetch::FetchError::Extraction(_) => {
                    (10, "extraction")
                }
                fetch::FetchError::Io(_) => (4, "disk-write"),
            };
            eprintln!("error ({label}): {e}");
            Ok(ExitCode::from(code))
        }
    }
}

fn run_cache_clear(args: FingerprintsCacheClearArgs) -> anyhow::Result<ExitCode> {
    let keep_sha = match args.keep_rev {
        Some(s) => match parse_sha_or_invalid(&s) {
            Ok(sha) => Some(sha),
            Err(e) => {
                eprintln!("error: {e}");
                return Ok(ExitCode::from(1));
            }
        },
        None => None,
    };

    let keep = match &keep_sha {
        Some(sha) => KeepRev::Except(sha),
        None => KeepRev::All,
    };

    match cache::cache_clear(keep) {
        Ok(removed) => {
            for path in removed {
                println!("removed: {}", path.display());
            }
            Ok(ExitCode::from(0))
        }
        Err(e) => {
            eprintln!("error (disk-write): {e}");
            Ok(ExitCode::from(4))
        }
    }
}

fn run_list() -> anyhow::Result<ExitCode> {
    let root = cache::cache_root();
    if !root.is_dir() {
        // Empty / nonexistent cache → no output, exit 0 (consistent
        // with `cache-clear` idempotency).
        return Ok(ExitCode::from(0));
    }

    let mut entries: Vec<(String, usize, String)> = Vec::new();
    for dirent in std::fs::read_dir(&root)?.flatten() {
        let path = dirent.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        // Skip non-SHA-shaped entries (e.g. lingering `.tmp-<uuid>/`
        // staging dirs from a crashed fetcher).
        if name.len() != 40 || !name.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()) {
            continue;
        }
        let record_count = count_records(&path);
        let mtime = format_mtime(&path);
        entries.push((name.to_string(), record_count, mtime));
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    for (sha, count, mtime) in entries {
        println!("{sha}  {count}  {mtime}");
    }
    Ok(ExitCode::from(0))
}

/// Read `<cache>/<sha>/corpus/index.json` and return the
/// `entries.length` value. Returns 0 on any read / parse error —
/// the listing is best-effort, not authoritative.
fn count_records(sha_dir: &Path) -> usize {
    let index_path = sha_dir.join("corpus").join("index.json");
    let Ok(bytes) = std::fs::read(&index_path) else {
        return 0;
    };
    let Ok(value): Result<serde_json::Value, _> = serde_json::from_slice(&bytes) else {
        return 0;
    };
    value["entries"]
        .as_array()
        .map(|a| a.len())
        .unwrap_or(0)
}

fn format_mtime(path: &Path) -> String {
    let Ok(meta) = std::fs::metadata(path) else {
        return "?".to_string();
    };
    let Ok(mtime) = meta.modified() else {
        return "?".to_string();
    };
    chrono::DateTime::<chrono::Utc>::from(mtime)
        .format("%Y-%m-%dT%H:%M:%SZ")
        .to_string()
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn parse_sha_or_invalid_accepts_valid_lowercase() {
        let sha = parse_sha_or_invalid("fff39c6ad22ce8420b506323ce1d5cce4b628d5c").unwrap();
        assert_eq!(sha.to_full_hex(), "fff39c6ad22ce8420b506323ce1d5cce4b628d5c");
    }

    #[test]
    fn parse_sha_or_invalid_rejects_uppercase() {
        let err = parse_sha_or_invalid("FFF39C6AD22CE8420B506323CE1D5CCE4B628D5C").unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("invalid SHA"), "got: {msg}");
    }

    #[test]
    fn parse_sha_or_invalid_rejects_short() {
        let err = parse_sha_or_invalid("fff39c6").unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("invalid SHA"), "got: {msg}");
    }

    #[test]
    fn parse_sha_or_invalid_rejects_non_hex() {
        let err = parse_sha_or_invalid("zzz39c6ad22ce8420b506323ce1d5cce4b628d5c").unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("invalid SHA"), "got: {msg}");
    }
}
