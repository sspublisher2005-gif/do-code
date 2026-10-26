//! Project-bound, bounded, append-only AgentMessage transcripts.
//!
//! The transcript is evidence of what Forge recorded, not proof that a model's statements are true.
use crate::{
    error::{ForgeError, Result, io_error},
    mission::{AgentMessage, Mission},
    mission_store::MissionStore,
    redaction, storage,
};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
};
use uuid::Uuid;

const MAX_MESSAGE_BYTES: usize = 200_000;
const MAX_TRANSCRIPT_BYTES: u64 = 20 * 1024 * 1024;
const MAX_TRANSCRIPT_MESSAGES: usize = 50_000;
const ZERO_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

#[derive(Clone, Debug, Serialize, Deserialize)]
struct TranscriptEvent {
    schema_version: u8,
    sequence: u64,
    previous_sha256: String,
    sha256: String,
    message: AgentMessage,
}

pub fn append(root: &Path, message: AgentMessage) -> Result<AgentMessage> {
    let store = MissionStore::open(root)?;
    let mission = store.get(&message.mission_id)?;
    validate_message(&mission, &message)?;
    let content = truncate_utf8(&redaction::redact(&message.content), MAX_MESSAGE_BYTES);
    let mut safe = message;
    safe.redacted = safe.redacted || content != safe.content;
    safe.content = content;
    safe.role = redaction::safe_terminal(&redaction::redact(&safe.role));
    safe.sender_id = redaction::safe_terminal(&redaction::redact(&safe.sender_id));
    safe.recipient_id = safe
        .recipient_id
        .map(|v| redaction::safe_terminal(&redaction::redact(&v)));
    safe.provider = safe
        .provider
        .map(|v| redaction::safe_terminal(&redaction::redact(&v)));
    safe.model = safe
        .model
        .map(|v| redaction::safe_terminal(&redaction::redact(&v)));
    let encoded_message =
        serde_json::to_vec(&safe).map_err(|e| ForgeError::Message(e.to_string()))?;
    if encoded_message.len() > MAX_MESSAGE_BYTES + 64 * 1024 {
        return Err(ForgeError::Safety(
            "serialized agent message exceeds the transcript entry limit.".into(),
        ));
    }
    let canonical_root = fs::canonicalize(root).map_err(|e| io_error(root, e))?;
    let home = storage::ensure_project_home(&canonical_root)?;
    let dir = home.join("transcripts");
    storage::ensure_private_dir(&dir)?;
    let path = dir.join(format!("{}.jsonl", safe.mission_id));
    let _lock = lock_transcript(
        &home
            .join("locks")
            .join(format!("{}.transcript.lock", safe.mission_id)),
    )?;
    let previous_events = read_events(&canonical_root, &path, &safe.mission_id)?;
    if previous_events.len() >= MAX_TRANSCRIPT_MESSAGES {
        return Err(ForgeError::Safety(
            "mission transcript reached its message-count limit.".into(),
        ));
    }
    let previous = previous_events
        .last()
        .map(|event| event.sha256.as_str())
        .unwrap_or(ZERO_HASH);
    let sequence = previous_events.len() as u64 + 1;
    let sha256 = event_hash(sequence, previous, &safe)?;
    let event = TranscriptEvent {
        schema_version: 1,
        sequence,
        previous_sha256: previous.to_owned(),
        sha256,
        message: safe.clone(),
    };
    let mut line = serde_json::to_vec(&event).map_err(|e| ForgeError::Message(e.to_string()))?;
    line.push(b'\n');
    let existing_bytes = if path.exists() {
        storage::read_bounded_regular(&path, MAX_TRANSCRIPT_BYTES)?
    } else {
        Vec::new()
    };
    if existing_bytes.len() as u64 + line.len() as u64 > MAX_TRANSCRIPT_BYTES {
        return Err(ForgeError::Safety(
            "mission transcript reached the 20 MiB limit; archive it before appending.".into(),
        ));
    }
    let mut options = OpenOptions::new();
    options.create(true).append(true).read(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let mut file = options.open(&path).map_err(|e| io_error(&path, e))?;
    let metadata = file.metadata().map_err(|e| io_error(&path, e))?;
    if !metadata.is_file() || metadata.len() as usize != existing_bytes.len() {
        return Err(ForgeError::Safety(
            "transcript changed while the append lock was held; refusing to append.".into(),
        ));
    }
    // Compare bytes from the opened handle so a path swap between read and open cannot
    // silently append to a different same-sized file. The advisory lock coordinates Forge
    // processes; a hostile same-user writer remains outside this local file-lock boundary.
    let mut opened_bytes = Vec::with_capacity(existing_bytes.len());
    file.seek(SeekFrom::Start(0))
        .map_err(|e| io_error(&path, e))?;
    file.read_to_end(&mut opened_bytes)
        .map_err(|e| io_error(&path, e))?;
    if opened_bytes != existing_bytes {
        return Err(ForgeError::Safety(
            "transcript bytes changed during append setup; refusing to append.".into(),
        ));
    }
    storage::reject_hardlink_handle(&file, &path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(|e| io_error(&path, e))?;
    }
    file.write_all(&line).map_err(|e| io_error(&path, e))?;
    file.sync_all().map_err(|e| io_error(&path, e))?;
    Ok(safe)
}

pub fn list(root: &Path, mission_id: &str) -> Result<Vec<AgentMessage>> {
    validate_uuid(mission_id)?;
    let store = MissionStore::open(root)?;
    let _ = store.get(mission_id)?;
    let canonical_root = fs::canonicalize(root).map_err(|e| io_error(root, e))?;
    let home = storage::ensure_project_home(&canonical_root)?;
    let dir = home.join("transcripts");
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let _lock = lock_transcript(
        &home
            .join("locks")
            .join(format!("{mission_id}.transcript.lock")),
    )?;
    let path = dir.join(format!("{mission_id}.jsonl"));
    Ok(read_events(&canonical_root, &path, mission_id)?
        .into_iter()
        .map(|event| event.message)
        .collect())
}

fn read_events(
    root: &Path,
    path: &Path,
    expected_mission_id: &str,
) -> Result<Vec<TranscriptEvent>> {
    let bytes = match storage::read_bounded_regular(path, MAX_TRANSCRIPT_BYTES) {
        Ok(bytes) => bytes,
        Err(ForgeError::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Vec::new());
        }
        Err(error) => return Err(error),
    };
    let mut out: Vec<TranscriptEvent> = Vec::new();
    let mut previous = ZERO_HASH.to_owned();
    for (idx, line) in bytes.split(|b| *b == b'\n').enumerate() {
        if line.is_empty() {
            continue;
        }
        if out.len() >= MAX_TRANSCRIPT_MESSAGES {
            return Err(ForgeError::Safety(
                "transcript message-count limit exceeded.".into(),
            ));
        }
        let event: TranscriptEvent = serde_json::from_slice(line).map_err(|_| {
            ForgeError::Safety(format!(
                "transcript line {} is invalid JSON; no transcript was returned.",
                idx + 1
            ))
        })?;
        if event.message.mission_id != expected_mission_id {
            return Err(ForgeError::Safety(
                "transcript contains a message bound to a different mission.".into(),
            ));
        }
        let store = MissionStore::open(root)?;
        let mission = store.get(&event.message.mission_id)?;
        validate_message(&mission, &event.message)?;
        if event.schema_version != 1
            || event.sequence != out.len() as u64 + 1
            || event.previous_sha256 != previous
            || event.sha256 != event_hash(event.sequence, &event.previous_sha256, &event.message)?
        {
            return Err(ForgeError::Safety(format!(
                "transcript hash chain or sequence failed at line {}; no transcript was returned.",
                idx + 1
            )));
        }
        if event.message.content.len() > MAX_MESSAGE_BYTES {
            return Err(ForgeError::Safety(
                "transcript contains an oversized message.".into(),
            ));
        }
        previous = event.sha256.clone();
        out.push(event);
    }
    Ok(out)
}

fn validate_message(mission: &Mission, message: &AgentMessage) -> Result<()> {
    if mission.id != message.mission_id
        || mission.objective.is_empty()
        || !valid_uuid(&message.id)
        || message.role.trim().is_empty()
        || message.role.len() > 100
        || message.sender_id.trim().is_empty()
        || message.sender_id.len() > 200
        || message.content.len() > MAX_MESSAGE_BYTES * 2
        || message.tool_call_ids.len() > 1000
        || message.evidence_ids.len() > 1000
    {
        return Err(ForgeError::Safety(
            "agent message violates the schema, project binding, or transcript resource limits."
                .into(),
        ));
    }
    if let Some(step_id) = &message.step_id
        && !mission.steps.iter().any(|step| step.id == *step_id)
    {
        return Err(ForgeError::Safety(
            "agent message references a step outside its mission.".into(),
        ));
    }
    for id in message
        .evidence_ids
        .iter()
        .chain(message.tool_call_ids.iter())
    {
        if !valid_uuid(id) {
            return Err(ForgeError::Safety(
                "agent message contains an invalid evidence/tool-call ID.".into(),
            ));
        }
    }
    if let Some(usage) = &message.usage
        && (usage.input_tokens.is_some_and(|n| n > 10_000_000)
            || usage.output_tokens.is_some_and(|n| n > 10_000_000)
            || usage
                .provider_reported_cost_usd
                .is_some_and(|n| !n.is_finite() || n < 0.0))
    {
        return Err(ForgeError::Safety(
            "agent message usage metadata is invalid.".into(),
        ));
    }
    Ok(())
}

fn lock_transcript(path: &Path) -> Result<File> {
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options.open(path).map_err(|e| io_error(path, e))?;
    let metadata = file.metadata().map_err(|e| io_error(path, e))?;
    if !metadata.is_file() {
        return Err(ForgeError::Safety(
            "transcript lock is not a regular file.".into(),
        ));
    }
    storage::reject_hardlink_handle(&file, path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() > 1 {
            return Err(ForgeError::Safety(
                "hard-linked transcript lock is denied.".into(),
            ));
        }
    }
    FileExt::lock_exclusive(&file).map_err(|e| io_error(path, e))?;
    Ok(file)
}

fn event_hash(sequence: u64, previous: &str, message: &AgentMessage) -> Result<String> {
    let body = serde_json::to_vec(&(1_u8, sequence, previous, message))
        .map_err(|e| ForgeError::Message(e.to_string()))?;
    Ok(storage::sha256(&body))
}
fn valid_uuid(id: &str) -> bool {
    Uuid::parse_str(id).is_ok_and(|parsed| parsed.to_string() == id)
}
fn validate_uuid(id: &str) -> Result<()> {
    let parsed = Uuid::parse_str(id)
        .map_err(|_| ForgeError::Message("mission ID must be a Forge-generated UUID.".into()))?;
    if parsed.to_string() != id {
        return Err(ForgeError::Message(
            "mission ID must use a canonical lowercase UUID.".into(),
        ));
    }
    Ok(())
}
fn truncate_utf8(value: &str, max_bytes: usize) -> String {
    let mut end = max_bytes.min(value.len());
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mission::UsageReport;
    use tempfile::tempdir;

    fn make_message(root: &Path, mission_id: &str, content: &str) -> AgentMessage {
        let mission = MissionStore::open(root).unwrap().get(mission_id).unwrap();
        AgentMessage {
            id: Uuid::new_v4().to_string(),
            mission_id: mission_id.to_owned(),
            step_id: mission.steps.first().map(|s| s.id.clone()),
            role: "researcher".into(),
            sender_id: "provider".into(),
            recipient_id: None,
            content: content.into(),
            tool_call_ids: Vec::new(),
            evidence_ids: Vec::new(),
            created_at: storage::timestamp(),
            provider: Some("fixture".into()),
            model: Some("fixture-model".into()),
            usage: Some(UsageReport {
                input_tokens: Some(12),
                output_tokens: Some(7),
                provider_reported_cost_usd: None,
            }),
            redacted: false,
        }
    }

    #[test]
    fn transcript_appends_redacted_messages_and_validates_hash_chain() {
        let root = tempdir().unwrap();
        let store = MissionStore::open(root.path()).unwrap();
        let mission = store.create("preserve transcript details safely").unwrap();
        append(
            root.path(),
            make_message(
                root.path(),
                &mission.id,
                "safe\napi_key=private-token-value",
            ),
        )
        .unwrap();
        let items = list(root.path(), &mission.id).unwrap();
        assert_eq!(items.len(), 1);
        assert!(!items[0].content.contains("private-token-value"));
        assert!(items[0].content.contains("[REDACTED]"));
        let path = root
            .path()
            .join(".forge/transcripts")
            .join(format!("{}.jsonl", mission.id));
        let mut data = fs::read_to_string(&path).unwrap();
        data = data.replace("safe", "tampered");
        fs::write(path, data).unwrap();
        assert!(list(root.path(), &mission.id).is_err());
    }

    #[test]
    fn transcript_append_is_serialized_across_threads() {
        let root = tempdir().unwrap();
        let store = MissionStore::open(root.path()).unwrap();
        let mission = store
            .create("record concurrent role transcript safely")
            .unwrap();
        let mut threads = Vec::new();
        for i in 0..8 {
            let root = root.path().to_path_buf();
            let mission_id = mission.id.clone();
            threads.push(std::thread::spawn(move || {
                for j in 0..4 {
                    append(
                        &root,
                        make_message(&root, &mission_id, &format!("message-{i}-{j}")),
                    )
                    .unwrap();
                }
            }));
        }
        for thread in threads {
            thread.join().unwrap();
        }
        assert_eq!(list(root.path(), &mission.id).unwrap().len(), 32);
    }
}
