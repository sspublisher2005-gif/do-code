//! Content-addressed local evidence records. A digest proves integrity, not truth.
use crate::{
    error::{ForgeError, Result, io_error},
    storage,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::Path};
use uuid::Uuid;

const EVIDENCE_VERSION: u8 = 1;
const MAX_EVIDENCE_BYTES: usize = 2 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EvidenceManifest {
    pub schema_version: u8,
    pub id: String,
    pub mission_id: String,
    pub project_id: String,
    pub root: String,
    pub kind: String,
    pub created_at: String,
    pub produced_by: String,
    pub sha256: String,
    pub byte_length: usize,
    pub media_type: String,
    pub metadata: BTreeMap<String, Value>,
    pub verified: bool,
    pub redacted: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Envelope {
    store_version: u8,
    manifest_sha256: String,
    manifest: EvidenceManifest,
}

pub fn put(
    root: &Path,
    mission_id: &str,
    kind: &str,
    produced_by: &str,
    content: &[u8],
    media_type: &str,
    metadata: BTreeMap<String, Value>,
) -> Result<EvidenceManifest> {
    validate_uuid(mission_id)?;
    if kind.len() > 100
        || kind.trim().is_empty()
        || produced_by.len() > 200
        || media_type.len() > 200
        || content.len() > MAX_EVIDENCE_BYTES
    {
        return Err(ForgeError::Safety(
            "evidence fields or content exceed their permitted limits.".into(),
        ));
    }
    let canonical_root = fs::canonicalize(root).map_err(|e| io_error(root, e))?;
    let home = storage::ensure_project_home(&canonical_root)?;
    let project_id = storage::project_id(&canonical_root)?;
    // Preserve binary evidence byte-for-byte. Text evidence is redacted before it can
    // be persisted; binary payloads are never rendered by the CLI and remain explicitly
    // marked as not redacted so callers cannot mistake hashing for privacy filtering.
    let (stored_content, redacted) = match std::str::from_utf8(content) {
        Ok(text) => {
            let redacted_text = crate::redaction::redact(text);
            let changed = redacted_text != text;
            (redacted_text.into_bytes(), changed)
        }
        Err(_) => (content.to_vec(), false),
    };
    if stored_content.len() > MAX_EVIDENCE_BYTES {
        return Err(ForgeError::Safety(
            "evidence exceeds its content limit after validation/redaction.".into(),
        ));
    }
    let id = Uuid::new_v4().to_string();
    let mission_dir = home.join("evidence").join(mission_id);
    storage::ensure_private_dir(&mission_dir)?;
    let sha256 = hash(&stored_content);
    storage::write_atomic(
        &mission_dir.join(format!("{id}.txt")),
        &stored_content,
        false,
    )?;
    let manifest = EvidenceManifest {
        schema_version: EVIDENCE_VERSION,
        id: id.clone(),
        mission_id: mission_id.to_owned(),
        project_id,
        root: canonical_root.to_string_lossy().into_owned(),
        kind: kind.to_owned(),
        created_at: storage::timestamp(),
        produced_by: crate::redaction::safe_terminal(produced_by),
        sha256,
        byte_length: stored_content.len(),
        media_type: media_type.to_owned(),
        metadata: sanitize_metadata(metadata),
        verified: false,
        redacted,
    };
    let payload = serde_json::to_vec(&manifest).map_err(|e| ForgeError::Message(e.to_string()))?;
    let envelope = Envelope {
        store_version: EVIDENCE_VERSION,
        manifest_sha256: hash(&payload),
        manifest: manifest.clone(),
    };
    let bytes = serde_json::to_vec(&envelope).map_err(|e| ForgeError::Message(e.to_string()))?;
    storage::write_atomic(&mission_dir.join(format!("{id}.json")), &bytes, false)?;
    Ok(manifest)
}

pub fn list(root: &Path, mission_id: &str) -> Result<Vec<EvidenceManifest>> {
    validate_uuid(mission_id)?;
    let canonical_root = fs::canonicalize(root).map_err(|e| io_error(root, e))?;
    let mission_dir = storage::ensure_project_home(&canonical_root)?
        .join("evidence")
        .join(mission_id);
    let mut result = Vec::new();
    let entries = match fs::read_dir(&mission_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(result),
        Err(error) => return Err(io_error(&mission_dir, error)),
    };
    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| io_error(&mission_dir, e))?;
        if entry.path().extension().is_some_and(|e| e == "json") {
            paths.push(entry.path());
        }
    }
    paths.sort();
    for path in paths {
        result.push(load_manifest(&canonical_root, mission_id, &path)?);
    }
    Ok(result)
}

/// Mark an evidence record as verified by a trusted sandbox producer.
/// This is fail-closed: it requires a non-empty sandbox run ID and a matching
/// mission. Until a real sandbox driver is qualified, this function always
/// returns an error so that no evidence can be marked verified by untrusted code.
#[allow(dead_code)]
pub fn mark_verified(
    _root: &Path,
    mission_id: &str,
    evidence_id: &str,
    sandbox_run_id: &str,
) -> Result<EvidenceManifest> {
    validate_uuid(mission_id)?;
    validate_uuid(evidence_id)?;
    if sandbox_run_id.trim().is_empty() {
        return Err(ForgeError::Safety(
            "sandbox run ID is required to mark evidence as verified.".into(),
        ));
    }
    // Fail-closed: no qualified sandbox driver exists yet.
    Err(ForgeError::Safety(
        "no qualified sandbox driver is available; evidence cannot be marked verified.".into(),
    ))
}

pub fn read(
    root: &Path,
    mission_id: &str,
    evidence_id: &str,
) -> Result<(EvidenceManifest, Vec<u8>)> {
    validate_uuid(mission_id)?;
    validate_uuid(evidence_id)?;
    let canonical_root = fs::canonicalize(root).map_err(|e| io_error(root, e))?;
    let home = storage::ensure_project_home(&canonical_root)?;
    let dir = home.join("evidence").join(mission_id);
    let manifest = load_manifest(
        &canonical_root,
        mission_id,
        &dir.join(format!("{evidence_id}.json")),
    )?;
    let content = storage::read_bounded_regular(
        &dir.join(format!("{evidence_id}.txt")),
        MAX_EVIDENCE_BYTES as u64,
    )?;
    if content.len() != manifest.byte_length || hash(&content) != manifest.sha256 {
        return Err(ForgeError::Safety(
            "evidence content hash/length mismatch.".into(),
        ));
    }
    Ok((manifest, content))
}

fn load_manifest(root: &Path, mission_id: &str, path: &Path) -> Result<EvidenceManifest> {
    let bytes = storage::read_bounded_regular(path, 512 * 1024)?;
    let envelope: Envelope = serde_json::from_slice(&bytes)
        .map_err(|_| ForgeError::Safety("evidence manifest is corrupt.".into()))?;
    let payload =
        serde_json::to_vec(&envelope.manifest).map_err(|e| ForgeError::Message(e.to_string()))?;
    if envelope.store_version != EVIDENCE_VERSION || hash(&payload) != envelope.manifest_sha256 {
        return Err(ForgeError::Safety(
            "evidence manifest checksum failed.".into(),
        ));
    }
    if envelope.manifest.id != path.file_stem().and_then(|s| s.to_str()).unwrap_or("")
        || envelope.manifest.mission_id != mission_id
        || envelope.manifest.root != root.to_string_lossy()
        || envelope.manifest.project_id != storage::project_id(root)?
    {
        return Err(ForgeError::Safety(
            "evidence manifest project or identity binding failed.".into(),
        ));
    }
    Ok(envelope.manifest)
}
fn sanitize_metadata(input: BTreeMap<String, Value>) -> BTreeMap<String, Value> {
    input
        .into_iter()
        .filter_map(|(key, mut value)| {
            if key.len() > 100 || crate::redaction::is_secret_key(&key) {
                return None;
            }
            crate::redaction::redact_json(&mut value);
            Some((key, value))
        })
        .collect()
}
fn validate_uuid(id: &str) -> Result<()> {
    let parsed = Uuid::parse_str(id).map_err(|_| {
        ForgeError::Message("evidence and mission IDs must be Forge-generated UUIDs.".into())
    })?;
    if parsed.to_string() != id {
        return Err(ForgeError::Message(
            "evidence and mission IDs must use canonical lowercase UUIDs.".into(),
        ));
    }
    Ok(())
}
fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    #[test]
    fn evidence_is_project_bound_and_detects_content_tampering() {
        let root = tempdir().unwrap();
        let mission = Uuid::new_v4().to_string();
        let entry = put(
            root.path(),
            &mission,
            "model_response",
            "test fixture",
            b"safe evidence",
            "text/plain",
            BTreeMap::new(),
        )
        .unwrap();
        let (loaded, content) = read(root.path(), &mission, &entry.id).unwrap();
        assert_eq!(loaded.sha256, entry.sha256);
        assert_eq!(content, b"safe evidence");
        let binary = put(
            root.path(),
            &mission,
            "binary_fixture",
            "test fixture",
            &[0x00, 0xFF, 0x01, 0x80],
            "application/octet-stream",
            BTreeMap::new(),
        )
        .unwrap();
        let (_, binary_content) = read(root.path(), &mission, &binary.id).unwrap();
        assert_eq!(
            binary_content,
            vec![0x00, 0xFF, 0x01, 0x80],
            "binary evidence should not be lossy-converted to UTF-8"
        );
        assert!(
            !binary.redacted,
            "binary content is preserved, not claimed to be text-redacted"
        );
        let body = root
            .path()
            .join(".forge/evidence")
            .join(&mission)
            .join(format!("{}.txt", entry.id));
        fs::write(body, b"tampered evidence").unwrap();
        assert!(read(root.path(), &mission, &entry.id).is_err());
    }
}
