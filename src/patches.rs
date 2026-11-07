//! Reviewable source patches with hash-conflict checks and recoverable checkpoints.
use crate::{
    error::{DocodeError, Result, io_error},
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
    /// Operation kind: "edit" (default), "create", "delete", or "rename".
    /// Older stored patches deserialize as edit.
    #[serde(default)]
    pub op: String,
    /// Rename target only.
    #[serde(default)]
    pub new_path: Option<String>,
}
/// One requested patch operation from the model or CLI.
#[derive(Clone, Debug)]
pub struct PatchOpInput {
    pub op: String,
    pub path: String,
    pub new_path: Option<String>,
    pub content: String,
    pub reason: String,
}
impl PatchOpInput {
    pub fn edit(
        path: impl Into<String>,
        content: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            op: "edit".into(),
            path: path.into(),
            new_path: None,
            content: content.into(),
            reason: reason.into(),
        }
    }
    pub fn create(
        path: impl Into<String>,
        content: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            op: "create".into(),
            path: path.into(),
            new_path: None,
            content: content.into(),
            reason: reason.into(),
        }
    }
    pub fn delete(path: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            op: "delete".into(),
            path: path.into(),
            new_path: None,
            content: String::new(),
            reason: reason.into(),
        }
    }
    pub fn rename(
        from: impl Into<String>,
        to: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            op: "rename".into(),
            path: from.into(),
            new_path: Some(to.into()),
            content: String::new(),
            reason: reason.into(),
        }
    }
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
    /// Operation performed, so undo can reverse create/delete/rename.
    #[serde(default)]
    op: String,
    /// Rename target only.
    #[serde(default)]
    new_path: Option<String>,
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
    changes: Vec<PatchOpInput>,
) -> Result<ProposedPatch> {
    validate_uuid(mission_id)?;
    if changes.is_empty() || changes.len() > MAX_PATCH_FILES {
        return Err(DocodeError::Safety(format!(
            "patch must change 1..{MAX_PATCH_FILES} files."
        )));
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut files_out = Vec::new();
    for change in changes {
        let op = change.op.to_ascii_lowercase();
        if !matches!(op.as_str(), "edit" | "create" | "delete" | "rename") {
            return Err(DocodeError::Safety(format!(
                "unsupported patch operation '{op}'."
            )));
        }
        if op == "rename"
            && change
                .new_path
                .as_deref()
                .map(str::trim)
                .unwrap_or("")
                .is_empty()
        {
            return Err(DocodeError::Safety(format!(
                "rename of '{}' is missing a destination path.",
                change.path
            )));
        }
        let display_path = change
            .new_path
            .clone()
            .unwrap_or_else(|| change.path.clone());
        if !seen.insert(display_path.clone()) {
            return Err(DocodeError::Safety(format!(
                "patch contains duplicate path '{display_path}'."
            )));
        }
        for p in std::iter::once(&change.path).chain(change.new_path.as_ref()) {
            if files::is_sensitive_path(p) {
                return Err(DocodeError::Safety(format!(
                    "patch cannot target sensitive path '{p}'."
                )));
            }
        }
        let content = if op == "edit" || op == "create" {
            change.content.clone()
        } else {
            String::new()
        };
        if (op == "edit" || op == "create") && content.contains('\0') {
            return Err(DocodeError::Safety(format!(
                "proposed file '{}' contains NUL bytes.",
                change.path
            )));
        }
        if content.len() as u64 > MAX_FILE_BYTES {
            return Err(DocodeError::Safety(format!(
                "proposed file '{}' exceeds the 2 MiB limit.",
                change.path
            )));
        }
        if !content.is_empty() && crate::redaction::redact(&content) != content {
            return Err(DocodeError::Safety(format!(
                "proposed file '{}' appears to contain a credential; remove it before saving the patch.",
                change.path
            )));
        }
        // The common content rules passed; per-operation validation follows.
        let (expected_sha256, new_content) = match op.as_str() {
            "edit" => {
                let absolute = files::resolve_workspace_file(root, &change.path)?;
                let current_bytes = storage::read_bounded_regular(&absolute, MAX_FILE_BYTES)?;
                let current = String::from_utf8(current_bytes).map_err(|_| {
                    DocodeError::Safety(format!(
                        "patch target '{}' is not UTF-8 text.",
                        change.path
                    ))
                })?;
                if current == content {
                    continue;
                }
                (hash(current.as_bytes()), content)
            }
            "create" => {
                let absolute = files::resolve_new_workspace_file(root, &change.path)?;
                if fs::symlink_metadata(&absolute).is_ok() {
                    return Err(DocodeError::Safety(format!(
                        "create refused: '{}' already exists.",
                        change.path
                    )));
                }
                (hash(String::new().as_bytes()), content)
            }
            "delete" => {
                let absolute = files::resolve_workspace_file(root, &change.path)?;
                let current_bytes = storage::read_bounded_regular(&absolute, MAX_FILE_BYTES)?;
                let current = String::from_utf8(current_bytes).map_err(|_| {
                    DocodeError::Safety(format!(
                        "delete target '{}' is not UTF-8 text.",
                        change.path
                    ))
                })?;
                // The precondition for a delete is the hash of the file that
                // will be removed, so apply can detect races.
                (hash(current.as_bytes()), String::new())
            }
            "rename" => {
                let to = change.new_path.clone().expect("checked above");
                if files::is_sensitive_path(&to) {
                    return Err(DocodeError::Safety(format!(
                        "patch cannot rename into sensitive path '{to}'."
                    )));
                }
                let from_absolute = files::resolve_workspace_file(root, &change.path)?;
                let to_absolute = files::resolve_new_workspace_file(root, &to)?;
                let current_bytes = storage::read_bounded_regular(&from_absolute, MAX_FILE_BYTES)?;
                let current = String::from_utf8(current_bytes).map_err(|_| {
                    DocodeError::Safety(format!(
                        "rename source '{}' is not UTF-8 text.",
                        change.path
                    ))
                })?;
                if fs::symlink_metadata(&to_absolute).is_ok() {
                    return Err(DocodeError::Safety(format!(
                        "rename refused: destination '{to}' already exists."
                    )));
                }
                // The rename precondition is the hash of the file being moved.
                (hash(current.as_bytes()), String::new())
            }
            _ => unreachable!("validated above"),
        };
        files_out.push(ProposedFileChange {
            path: change.path,
            expected_sha256,
            new_content,
            reason: bounded_text(&change.reason, 1_000),
            op,
            new_path: change.new_path,
        });
    }
    if files_out.is_empty() {
        return Err(DocodeError::Message(
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
    let canonical = serde_json::to_vec(patch).map_err(|e| DocodeError::Message(e.to_string()))?;
    let envelope = PatchEnvelope {
        store_version: PATCH_VERSION,
        sha256: hash(&canonical),
        patch: patch.clone(),
    };
    let bytes = serde_json::to_vec(&envelope).map_err(|e| DocodeError::Message(e.to_string()))?;
    storage::write_atomic(&path, &bytes, path.exists())
}

pub fn load_patch(root: &Path, mission_id: &str) -> Result<ProposedPatch> {
    validate_uuid(mission_id)?;
    let home = storage::ensure_project_home(root)?;
    let path = home.join("patches").join(format!("{mission_id}.json"));
    let bytes = storage::read_bounded_regular(&path, MAX_PATCH_BYTES)?;
    let envelope: PatchEnvelope = serde_json::from_slice(&bytes).map_err(|_| {
        DocodeError::Safety("patch proposal is corrupt; no files were changed.".into())
    })?;
    if envelope.store_version != PATCH_VERSION {
        return Err(DocodeError::Safety(
            "unsupported patch store version.".into(),
        ));
    }
    let payload =
        serde_json::to_vec(&envelope.patch).map_err(|e| DocodeError::Message(e.to_string()))?;
    if hash(&payload) != envelope.sha256 {
        return Err(DocodeError::Safety(
            "patch proposal checksum mismatch; no files were changed.".into(),
        ));
    }
    validate_patch(&envelope.patch)?;
    if envelope.patch.mission_id != mission_id {
        return Err(DocodeError::Safety(
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
        return Err(DocodeError::Safety(
            "patch file-count limit exceeded.".into(),
        ));
    }
    // Validate every target and conflict before creating checkpoint or changing any file.
    enum PlannedOp {
        Edit {
            absolute: PathBuf,
            before_text: String,
            before_hash: String,
            mode: u32,
        },
        Create {
            absolute: PathBuf,
            content: String,
        },
        Delete {
            absolute: PathBuf,
            before_text: String,
            before_hash: String,
        },
        Rename {
            from: PathBuf,
            to: PathBuf,
            before_text: String,
            before_hash: String,
            mode: u32,
        },
    }
    let mut prepared: Vec<(ProposedFileChange, PlannedOp)> = Vec::new();
    for change in &patch.files {
        match change.op.as_str() {
            "edit" => {
                let absolute = files::resolve_workspace_file(&canonical_root, &change.path)?;
                let before = storage::read_bounded_regular(&absolute, MAX_FILE_BYTES)?;
                let before_hash = hash(&before);
                if before_hash != change.expected_sha256 {
                    return Err(DocodeError::Safety(format!(
                        "PATCH_CONFLICT: '{}' changed after proposal creation; no files were changed.",
                        change.path
                    )));
                }
                let text = String::from_utf8(before).map_err(|_| {
                    DocodeError::Safety(format!(
                        "patch target '{}' is not UTF-8 text.",
                        change.path
                    ))
                })?;
                #[cfg(unix)]
                let mode = {
                    use std::os::unix::fs::PermissionsExt;
                    let metadata =
                        fs::symlink_metadata(&absolute).map_err(|e| io_error(&absolute, e))?;
                    metadata.permissions().mode() & 0o777
                };
                #[cfg(not(unix))]
                let mode = 0o600_u32;
                prepared.push((
                    change.clone(),
                    PlannedOp::Edit {
                        absolute,
                        before_text: text,
                        before_hash,
                        mode,
                    },
                ));
            }
            "create" => {
                let absolute = files::resolve_new_workspace_file(&canonical_root, &change.path)?;
                if fs::symlink_metadata(&absolute).is_ok() {
                    return Err(DocodeError::Safety(format!(
                        "PATCH_CONFLICT: '{}' was created after the proposal; no files were changed.",
                        change.path
                    )));
                }
                let parent = absolute.parent().ok_or_else(|| {
                    DocodeError::Safety(format!(
                        "create target '{}' has no parent directory.",
                        change.path
                    ))
                })?;
                if !parent.is_dir() {
                    return Err(DocodeError::Safety(format!(
                        "create refused: parent directory of '{}' does not exist.",
                        change.path
                    )));
                }
                prepared.push((
                    change.clone(),
                    PlannedOp::Create {
                        absolute,
                        content: change.new_content.clone(),
                    },
                ));
            }
            "delete" => {
                let absolute = files::resolve_workspace_file(&canonical_root, &change.path)?;
                let before = storage::read_bounded_regular(&absolute, MAX_FILE_BYTES)?;
                let before_hash = hash(&before);
                if before_hash != change.expected_sha256 {
                    return Err(DocodeError::Safety(format!(
                        "PATCH_CONFLICT: '{}' changed after proposal creation; no files were changed.",
                        change.path
                    )));
                }
                let text = String::from_utf8(before).map_err(|_| {
                    DocodeError::Safety(format!(
                        "delete target '{}' is not UTF-8 text.",
                        change.path
                    ))
                })?;
                prepared.push((
                    change.clone(),
                    PlannedOp::Delete {
                        absolute,
                        before_text: text,
                        before_hash,
                    },
                ));
            }
            "rename" => {
                let to = change.new_path.clone().ok_or_else(|| {
                    DocodeError::Safety(format!(
                        "rename of '{}' is missing its destination.",
                        change.path
                    ))
                })?;
                let from = files::resolve_workspace_file(&canonical_root, &change.path)?;
                let to_absolute = files::resolve_new_workspace_file(&canonical_root, &to)?;
                let before = storage::read_bounded_regular(&from, MAX_FILE_BYTES)?;
                let before_hash = hash(&before);
                if before_hash != change.expected_sha256 {
                    return Err(DocodeError::Safety(format!(
                        "PATCH_CONFLICT: '{}' changed after proposal creation; no files were changed.",
                        change.path
                    )));
                }
                let text = String::from_utf8(before).map_err(|_| {
                    DocodeError::Safety(format!(
                        "rename source '{}' is not UTF-8 text.",
                        change.path
                    ))
                })?;
                if fs::symlink_metadata(&to_absolute).is_ok() {
                    return Err(DocodeError::Safety(format!(
                        "PATCH_CONFLICT: rename destination '{to}' appeared after the proposal; no files were changed."
                    )));
                }
                #[cfg(unix)]
                let mode = {
                    use std::os::unix::fs::PermissionsExt;
                    let metadata = fs::symlink_metadata(&from).map_err(|e| io_error(&from, e))?;
                    metadata.permissions().mode() & 0o777
                };
                #[cfg(not(unix))]
                let mode = 0o600_u32;
                prepared.push((
                    change.clone(),
                    PlannedOp::Rename {
                        from,
                        to: to_absolute,
                        before_text: text,
                        before_hash,
                        mode,
                    },
                ));
            }
            other => {
                return Err(DocodeError::Safety(format!(
                    "unsupported patch operation '{other}'; no files were changed."
                )));
            }
        }
    }
    let checkpoint_id = Uuid::new_v4().to_string();
    let checkpoint_files = prepared
        .iter()
        .map(|(change, planned)| match planned {
            PlannedOp::Edit {
                before_text,
                before_hash,
                mode,
                ..
            } => CheckpointFile {
                path: change.path.clone(),
                before_content: before_text.clone(),
                before_sha256: before_hash.clone(),
                after_sha256: hash(change.new_content.as_bytes()),
                mode: *mode,
                op: "edit".into(),
                new_path: None,
            },
            PlannedOp::Create { content, .. } => CheckpointFile {
                path: change.path.clone(),
                before_content: String::new(),
                before_sha256: hash(String::new().as_bytes()),
                after_sha256: hash(content.as_bytes()),
                mode: 0o600_u32,
                op: "create".into(),
                new_path: None,
            },
            PlannedOp::Delete {
                before_text,
                before_hash,
                ..
            } => CheckpointFile {
                path: change.path.clone(),
                before_content: before_text.clone(),
                before_sha256: before_hash.clone(),
                after_sha256: hash(String::new().as_bytes()),
                mode: 0o600_u32,
                op: "delete".into(),
                new_path: None,
            },
            PlannedOp::Rename {
                before_text,
                before_hash,
                mode,
                ..
            } => CheckpointFile {
                path: change.path.clone(),
                before_content: before_text.clone(),
                before_sha256: before_hash.clone(),
                after_sha256: before_hash.clone(),
                mode: *mode,
                op: "rename".into(),
                new_path: change.new_path.clone(),
            },
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
        serde_json::to_vec(&checkpoint).map_err(|e| DocodeError::Message(e.to_string()))?;
    let checkpoint_envelope = CheckpointEnvelope {
        store_version: 1,
        sha256: hash(&canonical),
        checkpoint,
    };
    let checkpoint_bytes = serde_json::to_vec(&checkpoint_envelope)
        .map_err(|e| DocodeError::Message(e.to_string()))?;
    storage::write_atomic(
        &checkpoint_dir.join(format!("{checkpoint_id}.json")),
        &checkpoint_bytes,
        false,
    )?;

    let mut changed: Vec<(String, PathBuf, String, String, String, u32, String)> = Vec::new();
    for (change, planned) in &prepared {
        match planned {
            PlannedOp::Edit {
                absolute,
                before_text,
                before_hash,
                mode,
            } => {
                let current = storage::read_bounded_regular(absolute, MAX_FILE_BYTES)?;
                if hash(&current) != *before_hash {
                    rollback_applied(&changed);
                    return Err(DocodeError::Safety(format!(
                        "PATCH_RACE_DETECTED: '{}' changed while the patch was being applied; rollback was attempted for prior files.",
                        change.path
                    )));
                }
                if let Err(error) = storage::write_atomic_with_mode(
                    absolute,
                    change.new_content.as_bytes(),
                    true,
                    *mode,
                ) {
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
                    "edit".into(),
                ));
            }
            PlannedOp::Create {
                absolute, content, ..
            } => {
                if fs::symlink_metadata(absolute).is_ok() {
                    rollback_applied(&changed);
                    return Err(DocodeError::Safety(format!(
                        "PATCH_RACE_DETECTED: '{}' appeared while the patch was being applied; rollback was attempted for prior files.",
                        change.path
                    )));
                }
                if let Err(error) =
                    storage::write_atomic_with_mode(absolute, content.as_bytes(), false, 0o600_u32)
                {
                    rollback_applied(&changed);
                    return Err(error);
                }
                changed.push((
                    change.path.clone(),
                    absolute.clone(),
                    String::new(),
                    hash(String::new().as_bytes()),
                    hash(content.as_bytes()),
                    0o600_u32,
                    "create".into(),
                ));
            }
            PlannedOp::Delete {
                absolute,
                before_text,
                before_hash,
                ..
            } => {
                let current = storage::read_bounded_regular(absolute, MAX_FILE_BYTES)?;
                if hash(&current) != *before_hash {
                    rollback_applied(&changed);
                    return Err(DocodeError::Safety(format!(
                        "PATCH_RACE_DETECTED: '{}' changed while the patch was being applied; rollback was attempted for prior files.",
                        change.path
                    )));
                }
                if let Err(error) = fs::remove_file(absolute).map_err(|e| io_error(absolute, e)) {
                    rollback_applied(&changed);
                    return Err(error);
                }
                changed.push((
                    change.path.clone(),
                    absolute.clone(),
                    before_text.clone(),
                    before_hash.clone(),
                    hash(String::new().as_bytes()),
                    0o600_u32,
                    "delete".into(),
                ));
            }
            PlannedOp::Rename {
                from,
                to,
                before_text,
                before_hash,
                ..
            } => {
                let current = storage::read_bounded_regular(from, MAX_FILE_BYTES)?;
                if hash(&current) != *before_hash {
                    rollback_applied(&changed);
                    return Err(DocodeError::Safety(format!(
                        "PATCH_RACE_DETECTED: '{}' changed while the patch was being applied; rollback was attempted for prior files.",
                        change.path
                    )));
                }
                if fs::symlink_metadata(to).is_ok() {
                    rollback_applied(&changed);
                    return Err(DocodeError::Safety(
                        "PATCH_RACE_DETECTED: rename destination appeared while applying; rollback was attempted for prior files.".into(),
                    ));
                }
                if let Err(error) = fs::rename(from, to).map_err(|e| io_error(from, e)) {
                    rollback_applied(&changed);
                    return Err(error);
                }
                changed.push((
                    change.path.clone(),
                    to.clone(),
                    before_text.clone(),
                    before_hash.clone(),
                    before_hash.clone(),
                    0o600_u32,
                    "rename".into(),
                ));
            }
        }
    }
    // Code-over-API workspace journal: every applied mutation is recorded
    // with before/after hashes bound to its checkpoint. The journal is a
    // ledger, not the source of truth — the checkpoint is — so a journal
    // failure is recorded in the audit log and does not corrupt the apply.
    match crate::workspace::MutationJournal::for_mission(&canonical_root, &patch.mission_id) {
        Ok(mut journal) => {
            for (path, _, _, before_hash, after_hash, _, op) in &changed {
                if let Err(error) = journal.record(
                    &patch.mission_id,
                    "coder",
                    &format!("file_{op}"),
                    path,
                    before_hash,
                    after_hash,
                    &checkpoint_id,
                    true,
                ) {
                    let _ = crate::audit::append(
                        &canonical_root,
                        "workspace.journal_error",
                        "failed",
                        std::collections::BTreeMap::from([
                            ("missionId".into(), serde_json::json!(patch.mission_id)),
                            ("path".into(), serde_json::json!(path)),
                            ("error".into(), serde_json::json!(error.to_string())),
                        ]),
                    );
                }
            }
        }
        Err(error) => {
            let _ = crate::audit::append(
                &canonical_root,
                "workspace.journal_error",
                "failed",
                std::collections::BTreeMap::from([
                    ("missionId".into(), serde_json::json!(patch.mission_id)),
                    ("error".into(), serde_json::json!(error.to_string())),
                ]),
            );
        }
    }
    Ok((
        Some(checkpoint_id),
        prepared
            .iter()
            .map(|(c, _)| c.new_path.clone().unwrap_or_else(|| c.path.clone()))
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
        return Err(DocodeError::Safety(
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
            .ok_or_else(|| DocodeError::Message("no checkpoint exists for this mission.".into()))?
    };
    let bytes = storage::read_bounded_regular(&checkpoint_path, MAX_PATCH_BYTES)?;
    let envelope: CheckpointEnvelope = serde_json::from_slice(&bytes)
        .map_err(|_| DocodeError::Safety("checkpoint is corrupt; no restore attempted.".into()))?;
    if envelope.store_version != 1
        || envelope.checkpoint.schema_version != 1
        || envelope.checkpoint.mission_id != mission_id
        || envelope.checkpoint.root != canonical_root.to_string_lossy()
        || envelope.checkpoint.files.is_empty()
        || envelope.checkpoint.files.len() > MAX_PATCH_FILES
    {
        return Err(DocodeError::Safety(
            "checkpoint identity or schema is invalid; no restore attempted.".into(),
        ));
    }
    let canonical = serde_json::to_vec(&envelope.checkpoint)
        .map_err(|e| DocodeError::Message(e.to_string()))?;
    if hash(&canonical) != envelope.sha256 {
        return Err(DocodeError::Safety(
            "checkpoint checksum mismatch; no restore attempted.".into(),
        ));
    }
    let mut ready = Vec::new();
    for item in &envelope.checkpoint.files {
        match item.op.as_str() {
            // Delete undo: the file must be absent and gets its old bytes back.
            "delete" => {
                let absolute = files::resolve_new_workspace_file(&canonical_root, &item.path)?;
                if fs::symlink_metadata(&absolute).is_ok() {
                    return Err(DocodeError::Safety(format!(
                        "UNDO_CONFLICT: '{}' exists again since the checkpoint; nothing was restored.",
                        item.path
                    )));
                }
                if hash(item.before_content.as_bytes()) != item.before_sha256 {
                    return Err(DocodeError::Safety(format!(
                        "checkpoint integrity failed for '{}'.",
                        item.path
                    )));
                }
                ready.push((item.clone(), absolute));
            }
            // Create undo: the created file must still be exactly what Do Code wrote.
            "create" => {
                let absolute = files::resolve_workspace_file(&canonical_root, &item.path)?;
                let current = storage::read_bounded_regular(&absolute, MAX_FILE_BYTES)?;
                if hash(&current) != item.after_sha256 {
                    return Err(DocodeError::Safety(format!(
                        "UNDO_CONFLICT: '{}' changed since the checkpoint; nothing was restored.",
                        item.path
                    )));
                }
                ready.push((item.clone(), absolute));
            }
            // Rename undo: destination must hold the moved bytes and the source be gone.
            "rename" => {
                let to = item.new_path.clone().ok_or_else(|| {
                    DocodeError::Safety("checkpoint rename is missing its destination.".into())
                })?;
                // Source must be gone; the destination must exist.
                let source_gone = files::resolve_new_workspace_file(&canonical_root, &item.path)?;
                let to_absolute = files::resolve_workspace_file(&canonical_root, &to)?;
                let moved = storage::read_bounded_regular(&to_absolute, MAX_FILE_BYTES)?;
                if hash(&moved) != item.after_sha256 || fs::symlink_metadata(&source_gone).is_ok() {
                    return Err(DocodeError::Safety(format!(
                        "UNDO_CONFLICT: rename '{}' -> '{}' changed since the checkpoint; nothing was restored.",
                        item.path, to
                    )));
                }
                if hash(item.before_content.as_bytes()) != item.before_sha256 {
                    return Err(DocodeError::Safety(format!(
                        "checkpoint integrity failed for '{}'.",
                        item.path
                    )));
                }
                ready.push((item.clone(), to_absolute));
            }
            _ => {
                let absolute = files::resolve_workspace_file(&canonical_root, &item.path)?;
                let current = storage::read_bounded_regular(&absolute, MAX_FILE_BYTES)?;
                if hash(&current) != item.after_sha256 {
                    return Err(DocodeError::Safety(format!(
                        "UNDO_CONFLICT: '{}' changed since the checkpoint; nothing was restored.",
                        item.path
                    )));
                }
                if hash(item.before_content.as_bytes()) != item.before_sha256 {
                    return Err(DocodeError::Safety(format!(
                        "checkpoint integrity failed for '{}'.",
                        item.path
                    )));
                }
                ready.push((item.clone(), absolute));
            }
        }
    }
    for (item, path) in &ready {
        match item.op.as_str() {
            // Create undo removes the file Do Code added.
            "create" => {
                let current = storage::read_bounded_regular(path, MAX_FILE_BYTES)?;
                if hash(&current) != item.after_sha256 {
                    return Err(DocodeError::Safety(format!(
                        "UNDO_CONFLICT: '{}' changed during restore; no further files were restored.",
                        item.path
                    )));
                }
                fs::remove_file(path).map_err(|e| io_error(path, e))?;
            }
            // Rename undo moves the file back to its original path.
            "rename" => {
                let original = files::resolve_new_workspace_file(&canonical_root, &item.path)?;
                if fs::symlink_metadata(&original).is_ok() {
                    return Err(DocodeError::Safety(format!(
                        "UNDO_CONFLICT: '{}' appeared during restore; no further files were restored.",
                        item.path
                    )));
                }
                fs::rename(path, &original).map_err(|e| io_error(path, e))?;
            }
            // Edit undo restores the recorded bytes after a final race check.
            "edit" => {
                let current = storage::read_bounded_regular(path, MAX_FILE_BYTES)?;
                if hash(&current) != item.after_sha256 {
                    return Err(DocodeError::Safety(format!(
                        "UNDO_CONFLICT: '{}' changed during restore; no further files were restored.",
                        item.path
                    )));
                }
                storage::write_atomic_with_mode(
                    path,
                    item.before_content.as_bytes(),
                    true,
                    item.mode,
                )?;
            }
            // Delete undo: the file was validated absent; write the old bytes back.
            "delete" => {
                storage::write_atomic_with_mode(
                    path,
                    item.before_content.as_bytes(),
                    true,
                    item.mode,
                )?;
            }
            _ => {
                storage::write_atomic_with_mode(
                    path,
                    item.before_content.as_bytes(),
                    true,
                    item.mode,
                )?;
            }
        }
    }
    Ok(ready.iter().map(|(item, _)| item.path.clone()).collect())
}

fn rollback_applied(changed: &[(String, PathBuf, String, String, String, u32, String)]) {
    // Failure rollback never touches a target that no longer matches Do Code's output.
    for (relative, absolute, before_content, _before_hash, after_hash, _mode, op) in
        changed.iter().rev()
    {
        match op.as_str() {
            // A created file rolls back by removal; only remove what Do Code wrote.
            "create" => {
                let exists = fs::symlink_metadata(absolute)
                    .map(|m| !m.is_dir())
                    .unwrap_or(false);
                let matches = fs::read(absolute)
                    .map(|b| hash(&b) == *after_hash)
                    .unwrap_or(false);
                if exists && matches && fs::remove_file(absolute).is_err() {
                    eprintln!(
                        "docode: rollback incomplete for '{}'; use the retained checkpoint for manual recovery.",
                        crate::redaction::safe_terminal(relative)
                    );
                }
            }
            _ => {
                let current = match storage::read_bounded_regular(absolute, MAX_FILE_BYTES) {
                    Ok(bytes) => bytes,
                    Err(_) => continue,
                };
                if hash(&current) != *after_hash {
                    continue;
                }
                if storage::write_atomic_with_mode(
                    absolute,
                    before_content.as_bytes(),
                    true,
                    0o600_u32,
                )
                .is_err()
                {
                    eprintln!(
                        "docode: rollback incomplete for '{}'; use the retained checkpoint for manual recovery.",
                        crate::redaction::safe_terminal(relative)
                    );
                }
            }
        }
    }
}
fn validate_patch(patch: &ProposedPatch) -> Result<()> {
    if patch.schema_version != 1
        || patch.summary.len() > 4_000
        || patch.files.is_empty()
        || patch.files.len() > MAX_PATCH_FILES
    {
        return Err(DocodeError::Safety(
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
            return Err(DocodeError::Safety("patch contains an invalid path, duplicate, hash, sensitive target, or oversized content.".into()));
        }
    }
    Ok(())
}
fn validate_uuid(id: &str) -> Result<()> {
    let parsed = Uuid::parse_str(id)
        .map_err(|_| DocodeError::Message("expected a Do Code-generated UUID.".into()))?;
    if parsed.to_string() != id {
        return Err(DocodeError::Message(
            "expected a canonical lowercase Do Code UUID.".into(),
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
            vec![PatchOpInput::edit(
                "src/example.rs",
                "fn new_name() {}\n",
                "Use clearer name",
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
                PatchOpInput::edit("a.txt", "changed a\n", "first"),
                PatchOpInput::edit("b.txt", "changed b\n", "second"),
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
                vec![PatchOpInput::edit(".env", "API_KEY=secret", "no")]
            )
            .is_err()
        );
        assert!(
            create_proposal(
                root.path(),
                &mission_id,
                "Leak a token",
                vec![PatchOpInput::edit(
                    "main.rs",
                    "const API_KEY = \"long secret value here\";\n",
                    "no",
                )],
            )
            .is_err()
        );
    }

    #[test]
    fn create_operation_applies_and_undo_removes_the_created_file() {
        let root = tempdir().unwrap();
        storage::ensure_project_home(root.path()).unwrap();
        fs::create_dir_all(root.path().join("src")).unwrap();
        let mission_id = Uuid::new_v4().to_string();
        let patch = create_proposal(
            root.path(),
            &mission_id,
            "Add a new module",
            vec![PatchOpInput::create(
                "src/new_mod.rs",
                "pub fn born() {}\n",
                "new code",
            )],
        )
        .unwrap();
        assert_eq!(patch.files[0].op, "create");
        assert!(fs::read_to_string(root.path().join("src/new_mod.rs")).is_err());
        let (checkpoint, paths) = apply(root.path(), &mission_id, true).unwrap();
        assert_eq!(paths, vec!["src/new_mod.rs"]);
        assert_eq!(
            fs::read_to_string(root.path().join("src/new_mod.rs")).unwrap(),
            "pub fn born() {}\n"
        );
        let checkpoint = checkpoint.unwrap();
        undo(root.path(), &mission_id, Some(&checkpoint), true).unwrap();
        assert!(
            fs::read_to_string(root.path().join("src/new_mod.rs")).is_err(),
            "undo of a create must remove the created file"
        );
    }

    #[test]
    fn delete_operation_applies_and_undo_restores_exact_bytes() {
        let root = tempdir().unwrap();
        let original = "fn doomed() {}\n";
        setup_file(root.path(), "legacy.rs", original);
        storage::ensure_project_home(root.path()).unwrap();
        let mission_id = Uuid::new_v4().to_string();
        let patch = create_proposal(
            root.path(),
            &mission_id,
            "Remove legacy module",
            vec![PatchOpInput::delete("legacy.rs", "dead code")],
        )
        .unwrap();
        assert_eq!(patch.files[0].op, "delete");
        let (checkpoint, _) = apply(root.path(), &mission_id, true).unwrap();
        assert!(fs::read_to_string(root.path().join("legacy.rs")).is_err());
        let checkpoint = checkpoint.unwrap();
        let restored = undo(root.path(), &mission_id, Some(&checkpoint), true).unwrap();
        assert_eq!(restored, vec!["legacy.rs"]);
        assert_eq!(
            fs::read_to_string(root.path().join("legacy.rs")).unwrap(),
            original,
            "undo of a delete must restore the exact original bytes"
        );
    }

    #[test]
    fn rename_operation_applies_and_undo_moves_back() {
        let root = tempdir().unwrap();
        let content = "fn moved() {}\n";
        setup_file(root.path(), "old_name.rs", content);
        storage::ensure_project_home(root.path()).unwrap();
        let mission_id = Uuid::new_v4().to_string();
        let patch = create_proposal(
            root.path(),
            &mission_id,
            "Rename module",
            vec![PatchOpInput::rename(
                "old_name.rs",
                "new_name.rs",
                "clearer name",
            )],
        )
        .unwrap();
        assert_eq!(patch.files[0].op, "rename");
        assert_eq!(patch.files[0].new_path.as_deref(), Some("new_name.rs"));
        let (checkpoint, paths) = apply(root.path(), &mission_id, true).unwrap();
        assert_eq!(paths, vec!["new_name.rs"]);
        assert!(fs::read_to_string(root.path().join("old_name.rs")).is_err());
        assert_eq!(
            fs::read_to_string(root.path().join("new_name.rs")).unwrap(),
            content
        );
        let checkpoint = checkpoint.unwrap();
        let restored = undo(root.path(), &mission_id, Some(&checkpoint), true).unwrap();
        assert_eq!(restored, vec!["old_name.rs"]);
        assert_eq!(
            fs::read_to_string(root.path().join("old_name.rs")).unwrap(),
            content
        );
        assert!(fs::read_to_string(root.path().join("new_name.rs")).is_err());
    }

    #[test]
    fn create_refuses_existing_files_and_sensitive_paths() {
        let root = tempdir().unwrap();
        setup_file(root.path(), "exists.rs", "x\n");
        storage::ensure_project_home(root.path()).unwrap();
        let mission_id = Uuid::new_v4().to_string();
        assert!(
            create_proposal(
                root.path(),
                &mission_id,
                "clobber",
                vec![PatchOpInput::create("exists.rs", "y\n", "no")],
            )
            .is_err()
        );
        assert!(
            create_proposal(
                root.path(),
                &mission_id,
                "env",
                vec![PatchOpInput::create(".env.local", "KEY=1\n", "no")],
            )
            .is_err()
        );
    }

    #[test]
    fn rename_refuses_existing_destination_and_sensitive_targets() {
        let root = tempdir().unwrap();
        setup_file(root.path(), "a.rs", "a\n");
        setup_file(root.path(), "b.rs", "b\n");
        storage::ensure_project_home(root.path()).unwrap();
        let mission_id = Uuid::new_v4().to_string();
        assert!(
            create_proposal(
                root.path(),
                &mission_id,
                "clobber rename",
                vec![PatchOpInput::rename("a.rs", "b.rs", "no")],
            )
            .is_err()
        );
        assert!(
            create_proposal(
                root.path(),
                &mission_id,
                "sensitive rename",
                vec![PatchOpInput::rename("a.rs", "secrets.bin", "no")],
            )
            .is_err()
        );
    }

    #[test]
    fn multi_operation_patch_applies_atomically_with_one_checkpoint() {
        let root = tempdir().unwrap();
        setup_file(root.path(), "keep.txt", "original\n");
        storage::ensure_project_home(root.path()).unwrap();
        let mission_id = Uuid::new_v4().to_string();
        create_proposal(
            root.path(),
            &mission_id,
            "Mixed changes",
            vec![
                PatchOpInput::edit("keep.txt", "edited\n", "update"),
                PatchOpInput::create("fresh.txt", "created\n", "add"),
                PatchOpInput::delete("keep.txt.bak", "cleanup"),
            ],
        )
        .unwrap_err(); // delete target does not exist -> whole patch rejected
        // Now a consistent mix.
        setup_file(root.path(), "keep.txt.bak", "backup\n");
        let patch = create_proposal(
            root.path(),
            &mission_id,
            "Mixed changes",
            vec![
                PatchOpInput::edit("keep.txt", "edited\n", "update"),
                PatchOpInput::create("fresh.txt", "created\n", "add"),
                PatchOpInput::delete("keep.txt.bak", "cleanup"),
            ],
        )
        .unwrap();
        assert_eq!(patch.files.len(), 3);
        let (checkpoint, _) = apply(root.path(), &mission_id, true).unwrap();
        assert_eq!(
            fs::read_to_string(root.path().join("keep.txt")).unwrap(),
            "edited\n"
        );
        assert_eq!(
            fs::read_to_string(root.path().join("fresh.txt")).unwrap(),
            "created\n"
        );
        assert!(fs::read_to_string(root.path().join("keep.txt.bak")).is_err());
        undo(root.path(), &mission_id, Some(&checkpoint.unwrap()), true).unwrap();
        assert_eq!(
            fs::read_to_string(root.path().join("keep.txt")).unwrap(),
            "original\n"
        );
        assert!(fs::read_to_string(root.path().join("fresh.txt")).is_err());
        assert_eq!(
            fs::read_to_string(root.path().join("keep.txt.bak")).unwrap(),
            "backup\n"
        );
    }
}
