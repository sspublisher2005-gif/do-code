//! Evidence-driven sandbox qualification. Fail-closed by default.
//!
//! `is_qualified()` is no longer a hard-coded `false`. It reads a retained
//! qualification record that lives NEXT TO THE BINARY (outside the writable
//! workspace, so the agent cannot forge it via a patch), and returns true only
//! when the record is complete, status is exactly `QUALIFIED` (not a subset
//! pass), bound to the exact binary hash, and every case applicable to the
//! advertised platform passed. Any missing or inconsistent field fails closed.
//!
//! A digest proves integrity, not truth. This gate proves that a retained,
//! externally produced record exists and matches this binary — it does not by
//! itself prove isolation; the adversarial matrix and independent review do.

use crate::error::Result;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fs};

const QUALIFICATION_SCHEMA: u8 = 1;
const QUALIFIED_STATUS: &str = "QUALIFIED";
const RECORD_FILE: &str = "docode-qualification.json";
const MAX_RECORD_BYTES: usize = 256 * 1024;

/// Cases applicable to every advertised platform (from SANDBOX_ACCEPTANCE_MATRIX.json).
const ALL_PLATFORM_CASES: &[&str] = &[
    "FS-01", "FS-02", "FS-03", "FS-04", "FS-05", "NET-01", "NET-02", "PROC-01", "PROC-02",
    "RES-01", "ENV-01", "CMD-01", "EXEC-01", "LOG-01", "PROOF-01", "LIFE-01",
];
const WINDOWS_CASES: &[&str] = &["WIN-01", "WIN-02", "WIN-03"];
const PACKAGING_CASES: &[&str] = &["PKG-01"];

#[derive(Clone, Debug, Deserialize)]
struct CaseResult {
    id: String,
    passed: bool,
}

#[derive(Clone, Debug, Deserialize)]
struct QualificationRecord {
    schema_version: u8,
    status: String,
    /// SHA-256 of the exact binary the matrix was run against.
    binary_sha256: String,
    /// Platform this record qualifies (currently only "windows").
    platform: String,
    /// Identity of the disposable runner that produced the record.
    runner: String,
    /// SHA-256 of the retained raw adversarial report, hex encoded.
    report_sha256: String,
    cases: Vec<CaseResult>,
}

fn is_hex64(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}

fn required_cases(platform: &str) -> Result<Vec<&'static str>> {
    match platform {
        "windows" => {
            let mut v = Vec::with_capacity(
                ALL_PLATFORM_CASES.len() + WINDOWS_CASES.len() + PACKAGING_CASES.len(),
            );
            v.extend_from_slice(ALL_PLATFORM_CASES);
            v.extend_from_slice(WINDOWS_CASES);
            v.extend_from_slice(PACKAGING_CASES);
            Ok(v)
        }
        other => Err(crate::error::DocodeError::Safety(format!(
            "no qualification policy for platform '{other}'"
        ))),
    }
}

/// Validate a parsed record against a binary hash. Returns the reason on failure.
fn validate(record: &QualificationRecord, binary_sha256: &str) -> std::result::Result<(), String> {
    if record.schema_version != QUALIFICATION_SCHEMA {
        return Err(format!(
            "unsupported qualification schema {}",
            record.schema_version
        ));
    }
    // A subset pass is NOT a qualification. This is the exact failure mode
    // the current CI report (`WINDOWS_SUBSET_PASS`) must not trigger.
    if record.status != QUALIFIED_STATUS {
        return Err(format!(
            "qualification status '{}' is not '{QUALIFIED_STATUS}'",
            record.status
        ));
    }
    if !is_hex64(&record.binary_sha256) || !is_hex64(&record.report_sha256) {
        return Err("binary_sha256 and report_sha256 must be 64 hex chars".into());
    }
    if record.binary_sha256 != binary_sha256 {
        return Err("qualification was produced for a different binary".into());
    }
    if record.runner.trim().is_empty() {
        return Err("runner identity is missing".into());
    }
    let required = required_cases(&record.platform).map_err(|e| e.to_string())?;
    let passed: BTreeSet<&str> = record
        .cases
        .iter()
        .filter(|c| c.passed)
        .map(|c| c.id.as_str())
        .collect();
    if passed.len() != record.cases.len() {
        return Err("qualification record contains a failed case".into());
    }
    let mut missing: Vec<&str> = Vec::new();
    for id in required {
        if !passed.contains(id) {
            missing.push(id);
        }
    }
    if !missing.is_empty() {
        return Err(format!(
            "qualification is missing {} required case(s): {}",
            missing.len(),
            missing.join(", ")
        ));
    }
    Ok(())
}

fn current_binary_sha256() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let bytes = fs::read(&exe).ok()?;
    Some(format!("{:x}", Sha256::digest(&bytes)))
}

fn record_path() -> Option<std::path::PathBuf> {
    // Next to the running binary, never inside the writable workspace.
    Some(std::env::current_exe().ok()?.parent()?.join(RECORD_FILE))
}

/// Fail-closed qualification check. Returns true only for a complete,
/// binary-bound, externally produced record covering every applicable case.
pub fn is_qualified() -> bool {
    let (Some(binary_sha256), Some(path)) = (current_binary_sha256(), record_path()) else {
        return false;
    };
    let Ok(bytes) = fs::read(&path) else {
        return false;
    };
    if bytes.is_empty() || bytes.len() > MAX_RECORD_BYTES {
        return false;
    }
    let Ok(record) = serde_json::from_slice::<QualificationRecord>(&bytes) else {
        return false;
    };
    validate(&record, &binary_sha256).is_ok()
}

/// True if a record file is present (for honest diagnostics; not a verdict).
pub fn record_present() -> bool {
    record_path().map(|p| p.is_file()).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full_record(status: &str, binary: &str) -> QualificationRecord {
        let mut cases: Vec<CaseResult> = ALL_PLATFORM_CASES
            .iter()
            .chain(WINDOWS_CASES.iter())
            .chain(PACKAGING_CASES.iter())
            .map(|id| CaseResult {
                id: id.to_string(),
                passed: true,
            })
            .collect();
        cases[0].passed = true;
        QualificationRecord {
            schema_version: QUALIFICATION_SCHEMA,
            status: status.to_string(),
            binary_sha256: binary.to_string(),
            platform: "windows".to_string(),
            runner: "windows-2025".to_string(),
            report_sha256: "a".repeat(64),
            cases,
        }
    }

    const H: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    #[test]
    fn valid_full_record_qualifies() {
        assert!(validate(&full_record(QUALIFIED_STATUS, H), H).is_ok());
    }

    #[test]
    fn subset_pass_never_qualifies() {
        let r = full_record("WINDOWS_SUBSET_PASS", H);
        assert!(validate(&r, H).is_err());
    }

    #[test]
    fn wrong_binary_never_qualifies() {
        let r = full_record(QUALIFIED_STATUS, H);
        assert!(validate(&r, &"b".repeat(64)).is_err());
    }

    #[test]
    fn missing_case_never_qualifies() {
        let mut r = full_record(QUALIFIED_STATUS, H);
        r.cases.retain(|c| c.id != "FS-03");
        assert!(validate(&r, H).is_err());
    }

    #[test]
    fn failed_case_never_qualifies() {
        let mut r = full_record(QUALIFIED_STATUS, H);
        r.cases[0].passed = false;
        assert!(validate(&r, H).is_err());
    }

    #[test]
    fn unsupported_platform_never_qualifies() {
        let mut r = full_record(QUALIFIED_STATUS, H);
        r.platform = "linux".to_string();
        assert!(validate(&r, H).is_err());
    }
}
