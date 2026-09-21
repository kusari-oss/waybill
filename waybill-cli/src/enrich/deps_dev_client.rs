// Several response-shape structs in this file are populated by serde
// JSON deserialization from the deps.dev API but only some fields are
// then read directly in waybill code (e.g., `VersionInfo::licenses`
// drives license enrichment and (since milestone 776) `links`
// -> component externalReferences). Rust's dead-code analysis doesn't
// see through serde, so
// the un-read fields are flagged. Allow dead_code per-struct to
// preserve the wire-shape definitions; serde populates everything,
// and downstream callers may add reads later without re-shaping the
// struct.
#![allow(dead_code)]

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Version information from deps.dev GetVersion API.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct VersionInfo {
    pub licenses: Vec<String>,
    #[serde(default)]
    pub links: Vec<Link>,
}

// Milestone 839 (FR-016a): `advisoryKeys` was deserialised here and
// read nowhere. deps.dev offers no field mask, so the server sends it
// regardless — what we control is whether we keep it. Retaining it
// mattered more once an on-disk cache existed: it is the field most
// obviously mutable after publication (a CVE lands against an
// already-published version), so caching it would have created
// staleness risk for data that reaches no output at all.

/// A link associated with a package version.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Link {
    pub label: String,
    pub url: String,
}

/// Response shape from the `:dependencies` endpoint. deps.dev returns
/// the full transitive tree from the queried coord — `nodes[0]` is
/// always the queried coord itself (`relation == "SELF"`), followed
/// by one entry per transitive dep (`relation == "DIRECT" |
/// "INDIRECT"`). `edges` references nodes by index.
#[derive(Clone, Debug, Deserialize)]
pub struct DependencyGraph {
    #[serde(default)]
    pub nodes: Vec<DependencyNode>,
    #[serde(default)]
    pub edges: Vec<DependencyEdge>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct DependencyNode {
    #[serde(rename = "versionKey")]
    pub version_key: VersionKey,
    #[serde(default)]
    pub relation: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct VersionKey {
    /// Ecosystem tag, uppercase (e.g. `"MAVEN"`, `"CARGO"`). Returned
    /// uppercased regardless of request case.
    pub system: String,
    /// Package name. For Maven this is `"group:artifact"`.
    pub name: String,
    pub version: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct DependencyEdge {
    #[serde(rename = "fromNode")]
    pub from_node: usize,
    #[serde(rename = "toNode")]
    pub to_node: usize,
    #[serde(default)]
    pub requirement: String,
}

/// HTTP client for the deps.dev v3 API.
///
/// Provides hash-based package lookup and version metadata retrieval
/// for license, advisory, and supplier enrichment. `Clone` is cheap
/// because `reqwest::Client` is reference-counted internally.
#[derive(Clone)]
pub struct DepsDevClient {
    http: reqwest::Client,
    base_url: String,
    #[allow(dead_code)]
    timeout: Duration,
}

impl DepsDevClient {
    /// Create a new deps.dev API client with the given timeout per request.
    pub fn new(timeout: Duration) -> Self {
        let http = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .unwrap_or_default();

        Self {
            http,
            base_url: "https://api.deps.dev/v3".to_string(),
            timeout,
        }
    }

    /// Point the client at a different origin. Test-only — it exists
    /// so the concurrent fetch path can be exercised against a local
    /// mock, which is the only way to make workers complete out of
    /// request order on purpose.
    #[cfg(test)]
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    /// Milestone 839 — the bulk endpoint lives on the `v3alpha`
    /// surface, which the API docs say "may change in incompatible
    /// ways from time to time". Derived from `base_url` rather than
    /// hard-coded so the test override reaches it too.
    /// Test-only: identifies which mock server this client talks to, so a
    /// shared test sink can filter out emissions from tests running
    /// concurrently in the same binary.
    #[cfg(test)]
    pub(crate) fn base_url(&self) -> &str {
        &self.base_url
    }

    pub(crate) fn version_batch_url(&self) -> String {
        format!("{}/versionbatch", self.base_url.replace("/v3", "/v3alpha"))
    }

    /// Milestone 839 (FR-001) — fetch up to one page of bulk version
    /// metadata. Paging is the caller's business; this returns the
    /// page it got, continuation token included.
    pub async fn get_version_batch(
        &self,
        keys: &[super::request_key::EnrichmentKey],
        page_token: Option<String>,
    ) -> anyhow::Result<(super::deps_dev_batch::BatchPage, Option<u64>)> {
        let url = self.version_batch_url();
        let body = super::deps_dev_batch::build_body(keys, page_token);
        let response = self
            .http
            .post(&url)
            .header("Content-Type", "application/json")
            .body(body)
            .send()
            .await?;
        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            anyhow::bail!("deps.dev GetVersionBatch failed: HTTP {status} — {text}");
        }
        let max_age = max_age_of(&response);
        Ok((response.json().await?, max_age))
    }

    /// Build the URL for a GetVersion request.
    fn version_url(&self, system: &str, name: &str, version: &str) -> String {
        format!(
            "{}/systems/{}/packages/{}/versions/{}",
            self.base_url,
            url_encode(system),
            url_encode(name),
            url_encode(version),
        )
    }

    /// Build the URL for a `:dependencies` request — the full
    /// transitive dep graph starting from this coord.
    fn dependencies_url(&self, system: &str, name: &str, version: &str) -> String {
        format!(
            "{}/systems/{}/packages/{}/versions/{}:dependencies",
            self.base_url,
            url_encode(system),
            url_encode(name),
            url_encode(version),
        )
    }

    /// Fetch the full transitive dependency graph for a coord. The
    /// returned `DependencyGraph` has one `SELF` node (the queried
    /// coord) plus every transitive dep reachable from it, along with
    /// the `from → to` edges between them. Used by the post-scan
    /// enrichment pass to fill in deps that weren't reconstructable
    /// from local JARs or the M2 cache (typically because a shade
    /// plugin stripped `META-INF/maven/` or the user hasn't run
    /// `mvn install` to populate their local cache).
    ///
    /// `system` must be lowercase (deps.dev accepts both but documents
    /// lowercase as canonical). `name` must be formatted appropriately
    /// for the ecosystem — Maven names are `group:artifact`.
    pub async fn get_dependency_graph(
        &self,
        system: &str,
        name: &str,
        version: &str,
    ) -> anyhow::Result<DependencyGraph> {
        let url = self.dependencies_url(system, name, version);
        tracing::debug!(url = %url, "querying deps.dev for dependency graph");

        let response = self.http.get(&url).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!(
                "deps.dev GetDependencies failed: HTTP {status} — {body}"
            );
        }

        let graph: DependencyGraph = response.json().await?;
        Ok(graph)
    }

    /// Retrieve version metadata (licenses, advisories, links) for a package.
    pub async fn get_version(
        &self,
        system: &str,
        name: &str,
        version: &str,
    ) -> anyhow::Result<(VersionInfo, Option<u64>)> {
        let url = self.version_url(system, name, version);
        tracing::debug!(url = %url, "querying deps.dev for version info");

        let response = self.http.get(&url).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!(
                "deps.dev GetVersion failed: HTTP {status} — {body}"
            );
        }

        // Milestone 839 (FR-012a): the service publishes its own
        // freshness policy on every response. Reading it is more
        // durable than any interval chosen here — a pinned version is
        // immutable but deps.dev's record about it is not.
        let max_age = max_age_of(&response);
        let info: VersionInfo = response.json().await?;
        Ok((info, max_age))
    }
}

/// Extract `Cache-Control: max-age` from a response.
fn max_age_of(response: &reqwest::Response) -> Option<u64> {
    let raw = response
        .headers()
        .get(reqwest::header::CACHE_CONTROL)?
        .to_str()
        .ok()?;
    super::deps_dev_disk_cache::parse_max_age(Some(raw))
}

/// Percent-encode a URL path segment.
///
/// Covers the characters deps.dev's package-name field actually uses in
/// practice across its ecosystems:
///   `:` → `%3A` — Maven coord separator (`group:artifact`).
///   `/` → `%2F` — Go module paths (`github.com/spf13/cobra`).
///   `@` → `%40` — npm scoped packages (`@angular/core`).
///   `%`, space — defensive.
fn url_encode(s: &str) -> String {
    s.replace('%', "%25")
        .replace(' ', "%20")
        .replace('/', "%2F")
        .replace('@', "%40")
        .replace(':', "%3A")
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn version_url_construction() {
        let client = DepsDevClient::new(Duration::from_secs(5));
        let url = client.version_url("cargo", "serde", "1.0.197");
        assert_eq!(
            url,
            "https://api.deps.dev/v3/systems/cargo/packages/serde/versions/1.0.197"
        );
    }

    #[test]
    fn version_url_encodes_special_chars() {
        let client = DepsDevClient::new(Duration::from_secs(5));
        let url = client.version_url("npm", "@angular/core", "16.0.0");
        assert!(url.contains("%40angular%2Fcore"));
    }
}
