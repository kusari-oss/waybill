use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::types::hash::ContentHash;
use crate::types::timestamp::Timestamp;

use super::network::ProcessRef;

/// Aggregated file-system activity captured during a build trace.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FileAccess {
    pub operations: Vec<FileOperation>,
    pub summary: FileAccessSummary,
}

/// High-level summary statistics for file access during the trace.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileAccessSummary {
    pub total_operations: u64,
    pub unique_paths: u64,
    pub operations_by_type: BTreeMap<String, u64>,
}

/// A single observed file-system operation during the build.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FileOperation {
    pub path: String,
    pub operation: FileOpType,
    pub process: ProcessRef,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<ContentHash>,
    pub size: u64,
    pub timestamp: Timestamp,
    /// Milestone 1070: `true` when the path is relative and the directory it
    /// is relative to could not be established, so `path` is as the build
    /// passed it rather than absolute. Such operations are kept here as
    /// evidence and excluded from compiler read and write sets and from
    /// witness materials and products. Omitted when `false`.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub unresolved_relative: bool,
}

/// Classification of a file-system operation.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileOpType {
    Read,
    Write,
    Create,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_op_type_serde_snake_case() {
        let json = serde_json::to_string(&FileOpType::Create).expect("serialize file op type");
        assert_eq!(json, "\"create\"");

        let back: FileOpType = serde_json::from_str("\"write\"").expect("deserialize file op type");
        assert_eq!(back, FileOpType::Write);
    }

    #[test]
    fn file_operation_omits_none_hash() {
        let op = FileOperation {
            path: "/tmp/build/out.o".to_string(),
            operation: FileOpType::Write,
            process: ProcessRef {
                pid: 100,
                tid: 100,
                comm: "gcc".to_string(),
            },
            content_hash: None,
            size: 4096,
            timestamp: Timestamp::now(),
            unresolved_relative: false,
        };
        let json = serde_json::to_string(&op).expect("serialize file operation");
        assert!(!json.contains("\"content_hash\""));
        // Milestone 1070 (SC-005): the new flag is omitted at its default.
        assert!(!json.contains("unresolved_relative"));
    }

    /// Milestone 1070: an unresolved relative operation says so, and the flag
    /// survives a round trip; a document without it still deserializes.
    #[test]
    fn unresolved_relative_round_trips_and_defaults() {
        let op = FileOperation {
            path: "raw-dylibs".to_string(),
            operation: FileOpType::Read,
            process: ProcessRef { pid: 7, tid: 7, comm: "rustc".to_string() },
            content_hash: None,
            size: 0,
            timestamp: Timestamp::now(),
            unresolved_relative: true,
        };
        let json = serde_json::to_string(&op).expect("serialize");
        assert!(json.contains("\"unresolved_relative\":true"));
        let back: FileOperation = serde_json::from_str(&json).expect("deserialize");
        assert!(back.unresolved_relative);

        let legacy = json.replace(",\"unresolved_relative\":true", "");
        let back: FileOperation = serde_json::from_str(&legacy).expect("deserialize legacy");
        assert!(!back.unresolved_relative);
    }
}
