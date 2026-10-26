use crate::{
    config::set_private_permissions,
    error::{ForgeError, Result, io_error},
    redaction,
};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
};
use uuid::Uuid;

const MAX_LOG_BYTES: u64 = 10 * 1024 * 1024;
const MAX_EVENT_BYTES: usize = 16 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuditEvent {
    pub schema_version: u8,
    pub id: String,
    pub sequence: u64,
    pub at: String,
    pub project_id: String,
    pub event: String,
    pub outcome: String,
    pub details: BTreeMap<String, Value>,
}

pub fn append(
    root: &Path,
    event_name: &str,
    outcome: &str,
    details: BTreeMap<String, Value>,
) -> Result<AuditEvent> {
    let root = fs::canonicalize(root).map_err(|e| io_error(root, e))?;
    let home = root.join(".forge");
    match fs::symlink_metadata(&home) {
        Ok(meta) if meta.file_type().is_symlink() || !meta.is_dir() => {
            return Err(ForgeError::Safety(
                ".forge must be a real directory.".into(),
            ));
        }
        Ok(_) => {
            set_private_permissions(&home)?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            match fs::create_dir(&home) {
                Ok(()) => {}
                Err(create_error) if create_error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(create_error) => return Err(io_error(&home, create_error)),
            }
            let after = fs::symlink_metadata(&home).map_err(|e| io_error(&home, e))?;
            if after.file_type().is_symlink() || !after.is_dir() {
                return Err(ForgeError::Safety(
                    ".forge must be a real directory.".into(),
                ));
            }
            set_private_permissions(&home)?;
        }
        Err(e) => return Err(io_error(&home, e)),
    }
    let log_path = home.join("audit.jsonl");
    let _pre_open_meta = match fs::symlink_metadata(&log_path) {
        Ok(meta) => {
            if meta.file_type().is_symlink() || !meta.is_file() {
                return Err(ForgeError::Safety(
                    "audit log is not a regular file.".into(),
                ));
            }
            if meta.len() > MAX_LOG_BYTES {
                return Err(ForgeError::Safety(
                    "audit log exceeds 10 MiB; back it up and rotate it manually.".into(),
                ));
            }
            Some(meta)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(io_error(&log_path, e)),
    };
    let mut options = OpenOptions::new();
    options.create(true).read(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let mut file = options
        .open(&log_path)
        .map_err(|e| io_error(&log_path, e))?;
    FileExt::lock_exclusive(&file).map_err(|e| io_error(&log_path, e))?;
    let result = (|| {
        let opened_meta = file.metadata().map_err(|e| io_error(&log_path, e))?;
        if !opened_meta.is_file() {
            return Err(ForgeError::Safety(
                "opened audit target is not a regular file.".into(),
            ));
        }
        reject_audit_hardlink(&opened_meta, &log_path)?;
        crate::storage::reject_hardlink_handle(&file, &log_path)?;
        let length = opened_meta.len();
        if length > MAX_LOG_BYTES {
            return Err(ForgeError::Safety(
                "audit log exceeded 10 MiB before append.".into(),
            ));
        }
        // Enforce owner-only access on the opened file handle, not a separately resolved path.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(fs::Permissions::from_mode(0o600))
                .map_err(|e| io_error(&log_path, e))?;
        }
        let mut contents = String::new();
        file.seek(SeekFrom::Start(0))
            .map_err(|e| io_error(&log_path, e))?;
        (&mut file)
            .take(MAX_LOG_BYTES + 1)
            .read_to_string(&mut contents)
            .map_err(|e| io_error(&log_path, e))?;
        if contents.len() as u64 > MAX_LOG_BYTES {
            return Err(ForgeError::Safety(
                "audit log exceeded 10 MiB while being read; refusing to append.".into(),
            ));
        }
        let previous_sequence = contents
            .lines()
            .map(|line| serde_json::from_str::<AuditEvent>(line).map(|event| event.sequence))
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|_| {
                ForgeError::Safety(
                    "audit log contains a malformed event; back it up and repair it manually."
                        .into(),
                )
            })?
            .into_iter()
            .max()
            .unwrap_or(0);
        let next_sequence = previous_sequence.checked_add(1).ok_or_else(|| {
            ForgeError::Safety("audit sequence counter overflowed; refusing to append.".into())
        })?;
        // Keep audit identity consistent with mission, memory and evidence stores.
        let project_id = crate::storage::project_id(&root)?;
        let mut safe_details = Value::Object(details.into_iter().collect::<Map<String, Value>>());
        redaction::redact_json(&mut safe_details);
        let details: BTreeMap<String, Value> = match safe_details {
            Value::Object(map) => map.into_iter().collect(),
            _ => unreachable!("audit details are always serialized as an object"),
        };
        let event = AuditEvent {
            schema_version: 1,
            id: Uuid::new_v4().to_string(),
            sequence: next_sequence,
            at: rfc3339_timestamp(),
            project_id,
            event: redaction::redact(event_name),
            outcome: outcome.to_owned(),
            details,
        };
        let mut serialized = serde_json::to_vec(&event)
            .map_err(|e| ForgeError::Message(format!("cannot encode audit event: {e}")))?;
        serialized.push(b'\n');
        if serialized.len() > MAX_EVENT_BYTES {
            return Err(ForgeError::Safety("audit event exceeds 16 KiB.".into()));
        }
        if length + serialized.len() as u64 > MAX_LOG_BYTES {
            return Err(ForgeError::Safety(
                "audit log would exceed 10 MiB; rotate it manually.".into(),
            ));
        }
        file.write_all(&serialized)
            .map_err(|e| io_error(&log_path, e))?;
        file.sync_all().map_err(|e| io_error(&log_path, e))?;
        Ok(event)
    })();
    let unlock_result = FileExt::unlock(&file);
    result.and_then(|value| {
        unlock_result
            .map(|_| value)
            .map_err(|e| io_error(&log_path, e))
    })
}

pub fn tail(root: &Path, count: usize) -> Result<Vec<AuditEvent>> {
    if !(1..=1000).contains(&count) {
        return Err(ForgeError::Message(
            "audit --tail must be between 1 and 1000.".into(),
        ));
    }
    let root = fs::canonicalize(root).map_err(|e| io_error(root, e))?;
    let home = root.join(".forge");
    match fs::symlink_metadata(&home) {
        Ok(meta) if meta.file_type().is_symlink() || !meta.is_dir() => {
            return Err(ForgeError::Safety(
                ".forge must be a real directory.".into(),
            ));
        }
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(io_error(&home, e)),
    }
    let path = home.join("audit.jsonl");
    let meta = match fs::symlink_metadata(&path) {
        Ok(meta) => {
            if meta.file_type().is_symlink() || !meta.is_file() {
                return Err(ForgeError::Safety(
                    "audit log is not a regular file.".into(),
                ));
            }
            meta
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(io_error(&path, e)),
    };
    if meta.len() > MAX_LOG_BYTES {
        return Err(ForgeError::Safety(
            "audit log exceeds 10 MiB; rotate it manually before reading.".into(),
        ));
    }
    let mut opts = OpenOptions::new();
    opts.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let mut file = opts.open(&path).map_err(|e| io_error(&path, e))?;
    let opened_meta = file.metadata().map_err(|e| io_error(&path, e))?;
    if !opened_meta.is_file() {
        return Err(ForgeError::Safety(
            "opened audit target is not a regular file.".into(),
        ));
    }
    reject_audit_hardlink(&opened_meta, &path)?;
    crate::storage::reject_hardlink_handle(&file, &path)?;
    FileExt::lock_shared(&file).map_err(|e| io_error(&path, e))?;
    let locked_len = file.metadata().map_err(|e| io_error(&path, e))?.len();
    if locked_len > MAX_LOG_BYTES {
        let _ = FileExt::unlock(&file);
        return Err(ForgeError::Safety(
            "audit log exceeded 10 MiB before read.".into(),
        ));
    }
    let mut text = String::with_capacity(locked_len as usize);
    (&mut file)
        .take(MAX_LOG_BYTES + 1)
        .read_to_string(&mut text)
        .map_err(|e| io_error(&path, e))?;
    if text.len() as u64 > MAX_LOG_BYTES {
        let _ = FileExt::unlock(&file);
        return Err(ForgeError::Safety(
            "audit log exceeded 10 MiB while being read.".into(),
        ));
    }
    let parse_result = text
        .lines()
        .map(serde_json::from_str::<AuditEvent>)
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|_| {
            ForgeError::Safety(
                "audit log contains a malformed event; back it up and repair it manually.".into(),
            )
        });
    let _ = FileExt::unlock(&file);
    let mut values = parse_result?;
    let start = values.len().saturating_sub(count);
    Ok(values.drain(start..).collect())
}

fn reject_audit_hardlink(meta: &fs::Metadata, path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if meta.is_file() && meta.nlink() > 1 {
            return Err(ForgeError::Safety(format!(
                "hard-linked audit files are denied: {}",
                path.display()
            )));
        }
    }
    // All OS-specific opened-handle link checks are applied by reject_hardlink_handle.
    #[cfg(windows)]
    {
        let _ = (meta, path);
    }
    Ok(())
}

fn rfc3339_timestamp() -> String {
    use time::{OffsetDateTime, format_description::well_known::Rfc3339};
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "unknown-time".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    #[test]
    fn appends_sequenced_redacted_jsonl_events() {
        let temp = tempdir().unwrap();
        let mut details = BTreeMap::new();
        details.insert(
            "token".into(),
            Value::String("ghp_abcdefghijklmnopqrstuvwxyz123456".into()),
        );
        details.insert(
            "message".into(),
            Value::String("user entered sk-123456789012345678901234".into()),
        );
        append(temp.path(), "test.one", "succeeded", details).unwrap();
        append(temp.path(), "test.two", "info", BTreeMap::new()).unwrap();
        let values = tail(temp.path(), 10).unwrap();
        assert_eq!(values.len(), 2);
        assert_eq!(values[0].sequence, 1);
        assert_eq!(values[1].sequence, 2);
        assert_eq!(values[0].details["token"], "[REDACTED]");
        assert!(
            !values[0].details["message"]
                .as_str()
                .unwrap()
                .contains("sk-123456")
        );
    }
    #[cfg(unix)]
    #[test]
    fn audit_files_are_private() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempdir().unwrap();
        append(temp.path(), "test.private", "succeeded", BTreeMap::new()).unwrap();
        let home_mode = fs::metadata(temp.path().join(".forge"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        let file_mode = fs::metadata(temp.path().join(".forge/audit.jsonl"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(home_mode, 0o700);
        assert_eq!(file_mode, 0o600);
    }
    #[test]
    fn concurrent_audit_writes_have_unique_monotonic_sequences() {
        let temp = tempdir().unwrap();
        let root = temp.path().to_path_buf();
        // Initialize the private directory before the threads race on the audit log.
        append(&root, "test.seed", "info", BTreeMap::new()).unwrap();
        let mut workers = Vec::new();
        for worker in 0..8 {
            let worker_root = root.clone();
            workers.push(std::thread::spawn(move || {
                for item in 0..5 {
                    let details = BTreeMap::from([
                        ("worker".to_string(), Value::from(worker as u64)),
                        ("item".to_string(), Value::from(item as u64)),
                    ]);
                    append(&worker_root, "test.concurrent", "info", details).unwrap();
                }
            }));
        }
        for worker in workers {
            worker.join().unwrap();
        }
        let events = tail(&root, 100).unwrap();
        let sequences = events
            .iter()
            .map(|event| event.sequence)
            .collect::<Vec<_>>();
        assert_eq!(sequences, (1_u64..=41).collect::<Vec<_>>());
    }

    #[cfg(unix)]
    #[test]
    fn refuses_hardlinked_audit_log_without_modifying_external_target() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempdir().unwrap();
        let outside = tempdir().unwrap();
        fs::create_dir(root.path().join(".forge")).unwrap();
        fs::set_permissions(
            root.path().join(".forge"),
            fs::Permissions::from_mode(0o700),
        )
        .unwrap();
        let external = outside.path().join("external.log");
        fs::write(&external, "external-owned-content\n").unwrap();
        fs::hard_link(&external, root.path().join(".forge/audit.jsonl")).unwrap();
        let before = fs::read(&external).unwrap();
        assert!(append(root.path(), "test.hardlink", "info", BTreeMap::new()).is_err());
        assert!(tail(root.path(), 10).is_err());
        assert_eq!(fs::read(&external).unwrap(), before);
    }

    #[test]
    fn malformed_audit_entries_fail_closed_for_read_and_append() {
        let temp = tempdir().unwrap();
        let root = temp.path();
        append(root, "test.before-corruption", "info", BTreeMap::new()).unwrap();
        use std::io::Write;
        let path = root.join(".forge/audit.jsonl");
        let mut file = OpenOptions::new().append(true).open(&path).unwrap();
        writeln!(file, "{{malformed-json-line").unwrap();
        drop(file);
        assert!(
            tail(root, 10).is_err(),
            "tail must not silently discard corrupt records"
        );
        assert!(
            append(root, "test.after-corruption", "info", BTreeMap::new()).is_err(),
            "append must not reset or skip over corrupt history"
        );
    }

    #[test]
    fn oversized_audit_log_is_rejected_before_reading_or_appending() {
        let temp = tempdir().unwrap();
        let root = temp.path();
        std::fs::create_dir(root.join(".forge")).unwrap();
        std::fs::write(
            root.join(".forge/audit.jsonl"),
            "x".repeat(10 * 1024 * 1024 + 1),
        )
        .unwrap();
        assert!(tail(root, 10).is_err());
        assert!(append(root, "test.too-large", "info", BTreeMap::new()).is_err());
    }

    #[test]
    fn rejects_oversized_event_before_persisting() {
        let temp = tempdir().unwrap();
        let mut details = BTreeMap::new();
        details.insert("large".into(), Value::String("x".repeat(20 * 1024)));
        assert!(append(temp.path(), "too.large", "info", details).is_err());
        assert!(tail(temp.path(), 10).unwrap().is_empty());
    }
}
