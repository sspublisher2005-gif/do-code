//! Reviewable source patches with hash-conflict checks and recoverable checkpoints.
use crate::{
    error::{ForgeError, Result, io_error},
    files, storage,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};
use uuid::Uuid;

const PATCH_VERSION: u8 = 1;
const MAX_PATCH_FILES: usize = 30;
const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_PATCH_BYTES: u64 = 20 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProposedFileChange {
    pub path: String,
    pub expected_sha256: String,
    pub new_content: String,
    pub reason: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProposedPatch {
    pub schema_version: u8,
    pub id: String,
    pub mission_id: String,
    pub created_at: String,
    pub summary: String,
    pub files: Vec<ProposedFileChange>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct PatchEnvelope {
    store_version: u8,
    sha256: String,
    patch: ProposedPatch,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct CheckpointFile {
    path: String,
    before_content: String,
    before_sha256: String,
    after_sha256: String,
    mode: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Checkpoint {
    schema_version: u8,
    id: String,
    mission_id: String,
    root: String,
    created_at: String,
    files: Vec<CheckpointFile>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct CheckpointEnvelope {
    store_version: u8,
    sha256: String,
    checkpoint: Checkpoint,
}

pub fn create_proposal(
    root: &Path,
    mission_id: &str,
    summary: &str,
    changes: Vec<(String, String, String)>,
) -> Result<ProposedPatch> {
    validate_uuid(mission_id)?;
    if changes.is_empty() || changes.len() > MAX_PATCH_FILES {
        return Err(ForgeError::Safety(format!(
            "patch must change 1..{MAX_PATCH_FILES} existing text files."
        )));
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut files_out = Vec::new();
    for (path, new_content, reason) in changes {
        if !seen.insert(path.clone()) {
            return Err(ForgeError::Safety(format!(
                "patch contains duplicate path '{}'.",
                path
            )));
        }
        if files::is_sensitive_path(&path) {
            return Err(ForgeError::Safety(format!(
                "patch cannot target sensitive path '{}'.",
                path
            )));
        }
        if new_content.len() as u64 > MAX_FILE_BYTES {
            return Err(ForgeError::Safety(format!(
                "proposed file '{}' exceeds the 2 MiB limit.",
                path
            )));
        }
        if new_content.contains('\0') {
            return Err(ForgeError::Safety(format!(
                "proposed file '{}' contains NUL bytes.",
                path
            )));
        }
        if crate::redaction::redact(&new_content) != new_content {
            return Err(ForgeError::Safety(format!(
                "proposed file '{}' appears to contain a credential; remove it before saving the patch.",
                path
            )));
        }
        let absolute = files::resolve_workspace_file(root, &path)?;
        let current_bytes = storage::read_bounded_regular(&absolute, MAX_FILE_BYTES)?;
        let current = String::from_utf8(current_bytes).map_err(|_| {
            ForgeError::Safety(format!("patch target '{}' is not UTF-8 text.", path))
        })?;
        let expected_sha256 = hash(current.as_bytes());
        if current == new_content {
            continue;
        }
        files_out.push(ProposedFileChange {
            path,
            expected_sha256,
            new_content,
            reason: bounded_text(&reason, 1_000),
        });
    }
    if files_out.is_empty() {
        return Err(ForgeError::Message(
            "model proposal did not change any files.".into(),
        ));
    }
    let patch = ProposedPatch {
        schema_version: 1,
        id: Uuid::new_v4().to_string(),
        mission_id: mission_id.to_owned(),
        created_at: storage::timestamp(),
        summary: bounded_text(summary, 4_000),
        files: files_out,
    };
    save_patch(root, &patch)?;
    Ok(patch)
}

pub fn save_patch(root: &Path, patch: &ProposedPatch) -> Result<()> {
    validate_patch(patch)?;
    let home = storage::ensure_project_home(root)?;
    let path = home
        .join("patches")
        .join(format!("{}.json", patch.mission_id));
    let canonical = serde_json::to_vec(patch).map_err(|e| ForgeError::Message(e.to_string()))?;
    let envelope = PatchEnvelope {
        store_version: PATCH_VERSION,
        sha256: hash(&canonical),
        patch: patch.clone(),
    };
    let bytes = serde_json::to_vec(&envelope).map_err(|e| ForgeError::Message(e.to_string()))?;
    storage::write_atomic(&path, &bytes, path.exists())
}

pub fn load_patch(root: &Path, mission_id: &str) -> Result<ProposedPatch> {
    validate_uuid(mission_id)?;
    let home = storage::ensure_project_home(root)?;
    let path = home.join("patches").join(format!("{mission_id}.json"));
    let bytes = storage::read_bounded_regular(&path, MAX_PATCH_BYTES)?;
    let envelope: PatchEnvelope = serde_json::from_slice(&bytes).map_err(|_| {
        ForgeError::Safety("patch proposal is corrupt; no files were changed.".into())
    })?;
    if envelope.store_version != PATCH_VERSION {
        return Err(ForgeError::Safety(
            "unsupported patch store version.".into(),
        ));
    }
    let payload =
        serde_json::to_vec(&envelope.patch).map_err(|e| ForgeError::Message(e.to_string()))?;
    if hash(&payload) != envelope.sha256 {
        return Err(ForgeError::Safety(
            "patch proposal checksum mismatch; no files were changed.".into(),
        ));
    }
    validate_patch(&envelope.patch)?;
    if envelope.patch.mission_id != mission_id {
        return Err(ForgeError::Safety(
            "patch mission identity mismatch.".into(),
        ));
    }
    Ok(envelope.patch)
}

pub fn preview(patch: &ProposedPatch) -> String {
    let mut text = format!(
        "Patch {} for mission {}\nSummary: {}\nFiles: {}\n",
        patch.id,
        patch.mission_id,
        patch.summary,
        patch.files.len()
    );
    for change in &patch.files {
        text.push_str(&format!(
            "\n--- {} ---\nReason: {}\nRequired current-file SHA-256 (apply fails if the file changed since this patch was proposed): {}\nNew content SHA-256 (what the file will hash to after apply): {}\nNew size: {} bytes\n",
            change.path,
            change.reason,
            change.expected_sha256,
            hash(change.new_content.as_bytes()),
            change.new_content.len()
        ));
        text.push_str("Preview (first 30 lines; credential-like values masked):\n");
        for line in change.new_content.lines().take(30) {
            text.push_str(&crate::redaction::safe_terminal(&crate::redaction::redact(
                line,
            )));
            text.push('\n');
        }
    }
    text
}

pub fn apply(
    root: &Path,
    mission_id: &str,
    approved: bool,
) -> Result<(Option<String>, Vec<String>)> {
    if !approved {
        let patch = load_patch(root, mission_id)?;
        println!("{}", preview(&patch));
        println!(
            "Dry run only: no files were written. Re-run with --approve after reviewing the patch."
        );
        return Ok((None, patch.files.iter().map(|f| f.path.clone()).collect()));
    }
    crate::freeze::ensure_not_frozen(root, "patch application")?;
    let canonical_root = fs::canonicalize(root).map_err(|e| io_error(root, e))?;
    let patch = load_patch(&canonical_root, mission_id)?;
    if patch.files.len() > MAX_PATCH_FILES {
        return Err(ForgeError::Safety(
            "patch file-count limit exceeded.".into(),
        ));
    }
    // Validate every target and conflict before creating checkpoint or changing any file.
    let mut prepared = Vec::new();
    for change in &patch.files {
        let absolute = files::resolve_workspace_file(&canonical_root, &change.path)?;
        let before = storage::read_bounded_regular(&absolute, MAX_FILE_BYTES)?;
        let before_hash = hash(&before);
        if before_hash != change.expected_sha256 {
            return Err(ForgeError::Safety(format!(
                "PATCH_CONFLICT: '{}' changed after proposal creation; no files were changed.",
                change.path
            )));
        }
        let text = String::from_utf8(before).map_err(|_| {
            ForgeError::Safety(format!("patch target '{}' is not UTF-8 text.", change.path))
        })?;
        #[cfg(unix)]
        let mode = {
            use std::os::unix::fs::PermissionsExt;
            let metadata = fs::symlink_metadata(&absolute).map_err(|e| io_error(&absolute, e))?;
            metadata.permissions().mode() & 0o777
        };
        #[cfg(not(unix))]
        let mode = 0o600_u32;
        prepared.push((change.clone(), absolute, text, before_hash, mode));
    }
    let checkpoint_id = Uuid::new_v4().to_string();
    let checkpoint_files = prepared
        .iter()
        .map(|(change, _, before, before_hash, mode)| CheckpointFile {
            path: change.path.clone(),
            before_content: before.clone(),
            before_sha256: before_hash.clone(),
            after_sha256: hash(change.new_content.as_bytes()),
            mode: *mode,
        })
        .collect::<Vec<_>>();
    let checkpoint = Checkpoint {
        schema_version: 1,
        id: checkpoint_id.clone(),
        mission_id: patch.mission_id.clone(),
        root: canonical_root.to_string_lossy().into_owned(),
        created_at: storage::timestamp(),
        files: checkpoint_files,
    };
    let checkpoint_dir = storage::ensure_project_home(&canonical_root)?
        .join("checkpoints")
        .join(mission_id);
    storage::ensure_private_dir(&checkpoint_dir)?;
    let canonical =
        serde_json::to_vec(&checkpoint).map_err(|e| ForgeError::Message(e.to_string()))?;
    let checkpoint_envelope = CheckpointEnvelope {
        store_version: 1,
        sha256: hash(&canonical),
        checkpoint,
    };
    let checkpoint_bytes =
        serde_json::to_vec(&checkpoint_envelope).map_err(|e| ForgeError::Message(e.to_string()))?;
    storage::write_atomic(
        &checkpoint_dir.join(format!("{checkpoint_id}.json")),
        &checkpoint_bytes,
        false,
    )?;

    let mut changed: Vec<(String, PathBuf, String, String, String, u32)> = Vec::new();
    for (change, absolute, before_text, before_hash, mode) in &prepared {
        let current = storage::read_bounded_regular(absolute, MAX_FILE_BYTES)?;
        if hash(&current) != *before_hash {
            rollback_applied(&changed);
            return Err(ForgeError::Safety(format!(
                "PATCH_RACE_DETECTED: '{}' changed while patch was being applied; rollback was attempted for prior files if they still matched Forge's output.",
                change.path
            )));
        }
        if let Err(error) =
            storage::write_atomic_with_mode(absolute, change.new_content.as_bytes(), true, *mode)
        {
            rollback_applied(&changed);
            return Err(error);
        }
        changed.push((
            change.path.clone(),
            absolute.clone(),
            before_text.clone(),
            before_hash.clone(),
            hash(change.new_content.as_bytes()),
            *mode,
        ));
    }
    Ok((
        Some(checkpoint_id),
        prepared
            .iter()
            .map(|(c, _, _, _, _)| c.path.clone())
            .collect(),
    ))
}

pub fn undo(
    root: &Path,
    mission_id: &str,
    checkpoint_id: Option<&str>,
    approved: bool,
) -> Result<Vec<String>> {
    if !approved {
        return Err(ForgeError::Safety(
            "rollback requires --approve; no files were modified.".into(),
        ));
    }
    validate_uuid(mission_id)?;
    let canonical_root = fs::canonicalize(root).map_err(|e| io_error(root, e))?;
    let home = storage::ensure_project_home(&canonical_root)?;
    let dir = home.join("checkpoints").join(mission_id);
    let checkpoint_path = if let Some(id) = checkpoint_id {
        validate_uuid(id)?;
        dir.join(format!("{id}.json"))
    } else {
        let mut paths = fs::read_dir(&dir)
            .map_err(|e| io_error(&dir, e))?
            .filter_map(std::result::Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "json"))
            .collect::<Vec<_>>();
        paths.sort_by_key(|p| fs::metadata(p).and_then(|m| m.modified()).ok());
        paths
            .pop()
            .ok_or_else(|| ForgeError::Message("no checkpoint exists for this mission.".into()))?
    };
    let bytes = storage::read_bounded_regular(&checkpoint_path, MAX_PATCH_BYTES)?;
    let envelope: CheckpointEnvelope = serde_json::from_slice(&bytes)
        .map_err(|_| ForgeError::Safety("checkpoint is corrupt; no restore attempted.".into()))?;
    if envelope.store_version != 1
        || envelope.checkpoint.schema_version != 1
        || envelope.checkpoint.mission_id != mission_id
        || envelope.checkpoint.root != canonical_root.to_string_lossy()
        || envelope.checkpoint.files.is_empty()
        || envelope.checkpoint.files.len() > MAX_PATCH_FILES
    {
        return Err(ForgeError::Safety(
            "checkpoint identity or schema is invalid; no restore attempted.".into(),
        ));
    }
    let canonical =
        serde_json::to_vec(&envelope.checkpoint).map_err(|e| ForgeError::Message(e.to_string()))?;
    if hash(&canonical) != envelope.sha256 {
        return Err(ForgeError::Safety(
            "checkpoint checksum mismatch; no restore attempted.".into(),
        ));
    }
    let mut ready = Vec::new();
    for item in &envelope.checkpoint.files {
        let absolute = files::resolve_workspace_file(&canonical_root, &item.path)?;
        let current = storage::read_bounded_regular(&absolute, MAX_FILE_BYTES)?;
        if hash(&current) != item.after_sha256 {
            return Err(ForgeError::Safety(format!(
                "UNDO_CONFLICT: '{}' changed since the checkpoint; nothing was restored.",
                item.path
            )));
        }
        if hash(item.before_content.as_bytes()) != item.before_sha256 {
            return Err(ForgeError::Safety(format!(
                "checkpoint integrity failed for '{}'.",
                item.path
            )));
        }
        ready.push((item.clone(), absolute));
    }
    for (item, path) in &ready {
        let current = storage::read_bounded_regular(path, MAX_FILE_BYTES)?;
        if hash(&current) != item.after_sha256 {
            return Err(ForgeError::Safety(format!(
                "UNDO_CONFLICT: '{}' changed during restore; no further files were restored.",
                item.path
            )));
        }
        storage::write_atomic_with_mode(path, item.before_content.as_bytes(), true, item.mode)?;
    }
    Ok(ready.iter().map(|(item, _)| item.path.clone()).collect())
}

fn rollback_applied(changed: &[(String, PathBuf, String, String, String, u32)]) {
    // Failure rollback never overwrites a target that no longer matches Forge's proposed output.
    for (relative, absolute, before_content, _before_hash, after_hash, mode) in changed.iter().rev()
    {
        let current = match storage::read_bounded_regular(absolute, MAX_FILE_BYTES) {
            Ok(bytes) => bytes,
            Err(_) => continue,
        };
        if hash(&current) != *after_hash {
            continue;
        }
        if storage::write_atomic_with_mode(absolute, before_content.as_bytes(), true, *mode)
            .is_err()
        {
            eprintln!(
                "forge: rollback incomplete for '{}'; use the retained checkpoint for manual recovery.",
                crate::redaction::safe_terminal(relative)
            );
        }
    }
}
fn validate_patch(patch: &ProposedPatch) -> Result<()> {
    if patch.schema_version != 1
        || patch.summary.len() > 4_000
        || patch.files.is_empty()
        || patch.files.len() > MAX_PATCH_FILES
    {
        return Err(ForgeError::Safety(
            "patch schema or resource limits are invalid.".into(),
        ));
    }
    validate_uuid(&patch.id)?;
    validate_uuid(&patch.mission_id)?;
    let mut seen = std::collections::BTreeSet::new();
    for item in &patch.files {
        if item.path.is_empty()
            || item.path.len() > 1024
            || !seen.insert(item.path.as_str())
            || files::is_sensitive_path(&item.path)
            || !is_hash(&item.expected_sha256)
            || item.new_content.len() as u64 > MAX_FILE_BYTES
            || item.new_content.contains('\0')
        {
            return Err(ForgeError::Safety("patch contains an invalid path, duplicate, hash, sensitive target, or oversized content.".into()));
        }
    }
    Ok(())
}
fn validate_uuid(id: &str) -> Result<()> {
    let parsed = Uuid::parse_str(id)
        .map_err(|_| ForgeError::Message("expected a Forge-generated UUID.".into()))?;
    if parsed.to_string() != id {
        return Err(ForgeError::Message(
            "expected a canonical lowercase Forge UUID.".into(),
        ));
    }
    Ok(())
}
fn is_hash(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}
fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn bounded_text(value: &str, max: usize) -> String {
    crate::redaction::safe_terminal(&crate::redaction::redact(
        &value.chars().take(max).collect::<String>(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn setup_file(root: &Path, relative: &str, content: &str) {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    #[test]
    fn patch_requires_approval_and_apply_can_be_undone() {
        let root = tempdir().unwrap();
        setup_file(root.path(), "src/example.rs", "fn old() {}\n");
        storage::ensure_project_home(root.path()).unwrap();
        let mission_id = Uuid::new_v4().to_string();
        let patch = create_proposal(
            root.path(),
            &mission_id,
            "Rename function",
            vec![(
                "src/example.rs".into(),
                "fn new_name() {}\n".into(),
                "Use clearer name".into(),
            )],
        )
        .unwrap();
        assert_eq!(
            fs::read_to_string(root.path().join("src/example.rs")).unwrap(),
            "fn old() {}\n"
        );
        let (no_checkpoint, _) = apply(root.path(), &mission_id, false).unwrap();
        assert!(no_checkpoint.is_none());
        assert_eq!(
            fs::read_to_string(root.path().join("src/example.rs")).unwrap(),
            "fn old() {}\n"
        );
        let (checkpoint, paths) = apply(root.path(), &mission_id, true).unwrap();
        assert_eq!(paths, vec!["src/example.rs"]);
        assert_eq!(
            fs::read_to_string(root.path().join("src/example.rs")).unwrap(),
            "fn new_name() {}\n"
        );
        let checkpoint = checkpoint.unwrap();
        let restored = undo(root.path(), &mission_id, Some(&checkpoint), true).unwrap();
        assert_eq!(restored, vec!["src/example.rs"]);
        assert_eq!(
            fs::read_to_string(root.path().join("src/example.rs")).unwrap(),
            "fn old() {}\n"
        );
        assert_eq!(patch.mission_id, mission_id);
    }

    #[test]
    fn patch_conflict_is_detected_before_any_file_is_changed() {
        let root = tempdir().unwrap();
        setup_file(root.path(), "a.txt", "original a\n");
        setup_file(root.path(), "b.txt", "original b\n");
        storage::ensure_project_home(root.path()).unwrap();
        let mission_id = Uuid::new_v4().to_string();
        create_proposal(
            root.path(),
            &mission_id,
            "Update both files",
            vec![
                ("a.txt".into(), "changed a\n".into(), "first".into()),
                ("b.txt".into(), "changed b\n".into(), "second".into()),
            ],
        )
        .unwrap();
        fs::write(root.path().join("b.txt"), "a human edit\n").unwrap();
        assert!(apply(root.path(), &mission_id, true).is_err());
        assert_eq!(
            fs::read_to_string(root.path().join("a.txt")).unwrap(),
            "original a\n"
        );
        assert_eq!(
            fs::read_to_string(root.path().join("b.txt")).unwrap(),
            "a human edit\n"
        );
    }

    #[test]
    fn patch_rejects_secret_paths_and_credential_output() {
        let root = tempdir().unwrap();
        setup_file(root.path(), "main.rs", "fn main() {}\n");
        storage::ensure_project_home(root.path()).unwrap();
        let mission_id = Uuid::new_v4().to_string();
        assert!(
            create_proposal(
                root.path(),
                &mission_id,
                "Modify env",
                vec![(".env".into(), "API_KEY=secret".into(), "no".into())]
            )
            .is_err()
        );
        assert!(
            create_proposal(
                root.path(),
                &mission_id,
                "Leak a token",
                vec![(
                    "main.rs".into(),
                    "const API_KEY = \"long secret value here\";\n".into(),
                    "no".into()
                )]
            )
            .is_err()
        );
    }
}
