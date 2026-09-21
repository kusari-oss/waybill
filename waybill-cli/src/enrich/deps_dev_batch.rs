//! Milestone 839 (#766) — bulk version lookups via
//! `POST /v3alpha/versionbatch`.
//!
//! Behaviour here is pinned by `contracts/batch-client.md`. Two of its
//! clauses exist because probing the live service contradicted the
//! obvious implementation:
//!
//! - **C-2.2** — the service returns `"nextPageToken": ""` on the final
//!   page rather than omitting the field. Deserialised as
//!   `Option<String>` that is `Some("")`, so a presence check never
//!   terminates. Termination MUST test emptiness.
//! - **C-3.2** — `responses[].request.versionKey` echoes the
//!   *uncanonicalized* request. deps.dev normalises names per
//!   ecosystem, so matching against a key rebuilt through waybill's
//!   own canonicalisation mismatches wherever the two differ. Match
//!   on what was sent.
//!
//! Batch size is 100 (C-1.1a), the observed response page size. Larger
//! batches are accepted by the service and then split into serial
//! pages, measuring ~3-4x slower for identical coverage.

use serde::{Deserialize, Serialize};

/// Observed response page size. Undocumented — found by probing
/// (101 in, 100 out plus a continuation token). Re-verify with
/// `specs/839-batch-enrichment/measurements/pagecheck.py` before
/// trusting it after an upstream change.
pub const BATCH_SIZE: usize = 100;

/// Hard service limit. Exceeding it returns HTTP 400. Distinct from
/// `BATCH_SIZE`: this is what the service accepts, that is what we
/// send.
pub const MAX_BATCH_ENTRIES: usize = 5000;

// C-1.1a. Two different numbers and conflating them is the hazard:
// 5000 is what the service accepts, 100 is what it returns per page,
// and 100 is therefore what we send. Anything between is accepted and
// then silently split into serial pages. Checked at compile time so
// raising BATCH_SIZE toward the ceiling cannot happen quietly.
const _: () = assert!(BATCH_SIZE <= MAX_BATCH_ENTRIES);

#[derive(Debug, Serialize)]
pub struct VersionKeyRef {
    pub system: String,
    pub name: String,
    pub version: String,
}

#[derive(Debug, Serialize)]
struct BatchRequestEntry {
    #[serde(rename = "versionKey")]
    version_key: VersionKeyRef,
}

#[derive(Debug, Serialize)]
struct BatchRequestBody {
    requests: Vec<BatchRequestEntry>,
    #[serde(rename = "pageToken", skip_serializing_if = "Option::is_none")]
    page_token: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct EchoedVersionKey {
    pub system: String,
    pub name: String,
    pub version: String,
}

#[derive(Debug, Deserialize)]
pub struct EchoedRequest {
    #[serde(rename = "versionKey")]
    pub version_key: EchoedVersionKey,
}

#[derive(Debug, Deserialize)]
pub struct BatchResponseEntry {
    pub request: EchoedRequest,
    /// Absent when deps.dev carries no data for this coordinate. Not
    /// an error, and must not fail the batch (C-3.3).
    #[serde(default)]
    pub version: Option<super::deps_dev_client::VersionInfo>,
}

#[derive(Debug, Deserialize)]
pub struct BatchPage {
    #[serde(default)]
    pub responses: Vec<BatchResponseEntry>,
    /// **Empty string, not absent, on the last page.** See C-2.2.
    #[serde(rename = "nextPageToken", default)]
    pub next_page_token: String,
}

impl BatchPage {
    /// Whether another page remains.
    ///
    /// The whole point of this method is to make the emptiness test
    /// the only way to ask the question, so a caller cannot
    /// accidentally write `if page.next_page_token.is_some()`.
    pub fn has_more(&self) -> bool {
        !self.next_page_token.is_empty()
    }
}

/// Build the request body for one page.
pub(crate) fn build_body(
    keys: &[super::request_key::EnrichmentKey],
    page_token: Option<String>,
) -> String {
    let body = BatchRequestBody {
        requests: keys
            .iter()
            .map(|k| BatchRequestEntry {
                version_key: VersionKeyRef {
                    system: k.system.to_uppercase(),
                    name: k.name.clone(),
                    version: k.version.clone(),
                },
            })
            .collect(),
        page_token,
    };
    serde_json::to_string(&body).unwrap_or_else(|_| "{\"requests\":[]}".to_string())
}


#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use crate::enrich::request_key::EnrichmentKey;

    fn k(n: &str) -> EnrichmentKey {
        EnrichmentKey::from_purl_parts("cargo", None, n, "1.0.0").unwrap()
    }

    #[test]
    fn last_page_is_detected_by_emptiness_not_absence() {
        // C-2.2, the trap. deps.dev sends "" rather than omitting the
        // field, so a presence check loops forever. Both shapes must
        // read as "no more pages".
        let empty: BatchPage = serde_json::from_str(r#"{"responses":[],"nextPageToken":""}"#).unwrap();
        assert!(!empty.has_more(), "empty token means no more pages");

        let absent: BatchPage = serde_json::from_str(r#"{"responses":[]}"#).unwrap();
        assert!(!absent.has_more(), "absent token also means no more pages");

        let more: BatchPage =
            serde_json::from_str(r#"{"responses":[],"nextPageToken":"abc"}"#).unwrap();
        assert!(more.has_more());
    }

    #[test]
    fn missing_version_is_absence_not_failure() {
        // C-3.3. An entry with its request echoed and no `version` is
        // a package deps.dev does not carry.
        let page: BatchPage = serde_json::from_str(
            r#"{"responses":[
                 {"request":{"versionKey":{"system":"CARGO","name":"a","version":"1"}}},
                 {"request":{"versionKey":{"system":"CARGO","name":"b","version":"1"}},
                  "version":{"licenses":["MIT"],"links":[]}}
               ],"nextPageToken":""}"#,
        )
        .unwrap();
        assert_eq!(page.responses.len(), 2);
        assert!(page.responses[0].version.is_none());
        assert_eq!(page.responses[1].version.as_ref().unwrap().licenses, vec!["MIT"]);
    }

    #[test]
    fn chunks_never_exceed_the_page_size() {
        let keys: Vec<_> = (0..1001).map(|i| k(&format!("p{i}"))).collect();
        let chunks: Vec<_> = keys.chunks(BATCH_SIZE).collect();
        assert_eq!(chunks.len(), 11, "1001 keys at 100 per chunk");
        assert!(chunks.iter().all(|c| c.len() <= BATCH_SIZE));
        assert_eq!(chunks.iter().map(|c| c.len()).sum::<usize>(), 1001);
    }

    #[test]
    fn chosen_size_is_the_observed_page_size() {
        // The `<= MAX_BATCH_ENTRIES` relation is enforced at compile
        // time above; clippy rightly rejects re-asserting a constant.
        // What is worth pinning at runtime is the *value*: raising it
        // toward the ceiling reintroduces serial pagination silently,
        // so a change here has to be deliberate.
        assert_eq!(BATCH_SIZE, 100, "the observed response page size");
    }

    #[test]
    fn system_is_upper_cased_for_the_batch_api() {
        // The REST path uses lowercase (`/systems/cargo/`), the batch
        // body uses the enum form (`"CARGO"`). Sending lowercase here
        // yields entries with no data, which looks identical to a
        // package deps.dev does not carry.
        let body = build_body(&[k("serde")], None);
        assert!(body.contains(r#""system":"CARGO""#), "{body}");
        assert!(!body.contains("pageToken"), "no token on the first page");
    }

    #[test]
    fn page_token_is_sent_only_when_continuing() {
        let body = build_body(&[k("serde")], Some("tok".into()));
        assert!(body.contains(r#""pageToken":"tok""#), "{body}");
    }

    /// Issue #927 (m923) — FR-007c / T020. **Pin the surface this feature
    /// bet on.**
    ///
    /// Making batched enrichment the default accepts a dependency on
    /// `v3alpha`, which deps.dev documents as liable to "change in
    /// incompatible ways from time to time". That trade was made knowingly;
    /// this test makes it visible when *we* change the URL.
    ///
    /// It is the cheap floor, not the whole guard. It cannot see upstream
    /// graduating the endpoint to a stable surface — only the scheduled
    /// check (T019) can, and this test is not a substitute for it.
    ///
    /// Note `v3alpha` appears twice in the tree for two unrelated surfaces:
    /// the batch endpoint here, and hash-based lookup at
    /// `resolve/hash_resolver.rs`. A failure here means the **enrichment
    /// batch** endpoint moved; hash resolution has its own assertion and is
    /// not implicated.
    #[test]
    fn the_batch_endpoint_is_pinned_to_v3alpha() {
        let url = crate::enrich::deps_dev_client::DepsDevClient::new(std::time::Duration::from_secs(5)).version_batch_url();
        assert_eq!(
            url, "https://api.deps.dev/v3alpha/versionbatch",
            "the enrichment batch endpoint moved. If upstream graduated it off \
             v3alpha this is good news and FR-007c's risk argument should be \
             revisited; if this is an accidental edit it silently changes which \
             API the default path calls",
        );
    }

    /// Companion to the pin: the alpha path must be derived from the base
    /// URL, not hard-coded, or the test override stops reaching it and every
    /// batch test above would quietly exercise a URL no mock serves.
    #[test]
    fn the_alpha_path_is_derived_from_the_base_url() {
        let url = crate::enrich::deps_dev_client::DepsDevClient::new(std::time::Duration::from_secs(5))
            .with_base_url("http://127.0.0.1:9/v3")
            .version_batch_url();
        assert_eq!(url, "http://127.0.0.1:9/v3alpha/versionbatch");
    }
}
