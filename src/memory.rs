//! Human-readable, user-confirmed local learning memory (Markdown notes + JSON index).
use crate::{
    error::{DocodeError, Result, io_error},
    storage,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};
use uuid::Uuid;

const NOTE_VERSION: u8 = 1;
const MAX_NOTE_BYTES: u64 = 256 * 1024;
const MAX_EXPORT_BYTES: u64 = 20 * 1024 * 1024;
const MAX_NOTES: usize = 10_000;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryNote {
    pub schema_version: u8,
    pub id: String,
    pub note_type: String,
    pub title: String,
    pub problem: String,
    pub decision: String,
    pub reason: String,
    pub rejected_options: Vec<String>,
    pub limitations: Vec<String>,
    pub evidence: Vec<String>,
    pub confidence: f64,
    pub created_at: String,
    pub last_verified_at: Option<String>,
    pub status: String,
    pub scope: String,
    pub project_id: Option<String>,
    pub user_confirmed: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MemoryIndexEntry {
    pub id: String,
    pub filename: String,
    pub sha256: String,
    pub note_type: String,
    pub title: String,
    pub scope: String,
    pub project_id: Option<String>,
    pub last_indexed_at: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MemoryIndex {
    pub schema_version: u8,
    pub entries: Vec<MemoryIndexEntry>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct ExportBundle {
    schema_version: u8,
    exported_at: String,
    notes: Vec<MemoryNote>,
}
#[derive(Clone, Debug)]
struct ScopePaths {
    // KNOWN GAP: kept for diagnostics/debuggability; not read by any call site yet.
    #[allow(dead_code)]
    scope_dir: PathBuf,
    notes_dir: PathBuf,
    index_path: PathBuf,
    scope: String,
    project_id: Option<String>,
}

// Each argument is a distinct required field of a learning-memory note; grouping them into a
// params struct would add a layer with no safety benefit for this internal/CLI-adjacent entry point.
#[allow(clippy::too_many_arguments)]
pub fn save(
    root: &Path,
    note_type: &str,
    title: &str,
    problem: &str,
    decision: &str,
    reason: &str,
    rejected: Vec<String>,
    limitations: Vec<String>,
    evidence: Vec<String>,
    confidence: f64,
    scope: &str,
    confirmed: bool,
) -> Result<MemoryNote> {
    if !confirmed {
        return Err(DocodeError::Safety(
            "learning memory is never saved without explicit --confirm. No note was written."
                .into(),
        ));
    }
    let paths = scope_paths(root, scope)?;
    let note = MemoryNote {
        schema_version: NOTE_VERSION,
        id: Uuid::new_v4().to_string(),
        note_type: note_type.to_owned(),
        title: title.trim().to_owned(),
        problem: problem.trim().to_owned(),
        decision: decision.trim().to_owned(),
        reason: reason.trim().to_owned(),
        rejected_options: rejected,
        limitations,
        evidence,
        confidence,
        created_at: storage::timestamp(),
        last_verified_at: None,
        status: "active".into(),
        scope: paths.scope.clone(),
        project_id: paths.project_id.clone(),
        user_confirmed: true,
    };
    validate_note(&note)?;
    persist(&paths, &note, false)?;
    rebuild_index_path(&paths)?;
    Ok(note)
}

pub fn list(root: &Path, scope: &str) -> Result<Vec<MemoryNote>> {
    let mut out = Vec::new();
    for paths in selected_scopes(root, scope)? {
        out.extend(load_scope(&paths)?);
    }
    out.sort_by(|a, b| {
        b.created_at
            .cmp(&a.created_at)
            .then_with(|| a.title.cmp(&b.title))
    });
    Ok(out)
}

pub fn search(root: &Path, query: &str, scope: &str) -> Result<Vec<MemoryNote>> {
    if query.trim().len() < 2 || query.len() > 500 {
        return Err(DocodeError::Message(
            "memory search query must be 2..500 bytes.".into(),
        ));
    }
    let needle = query.to_lowercase();
    let mut scored = list(root, scope)?
        .into_iter()
        .filter_map(|note| {
            let hay = format!(
                "{} {} {} {} {} {}",
                note.title,
                note.problem,
                note.decision,
                note.reason,
                note.evidence.join(" "),
                note.limitations.join(" ")
            )
            .to_lowercase();
            let score = hay.match_indices(&needle).count() as u32 * 3
                + note.title.to_lowercase().matches(&needle).count() as u32 * 8
                + note.note_type.to_lowercase().matches(&needle).count() as u32;
            (score > 0).then_some((score, note))
        })
        .collect::<Vec<_>>();
    scored.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| b.1.created_at.cmp(&a.1.created_at))
    });
    Ok(scored.into_iter().map(|(_, n)| n).collect())
}

pub fn show(root: &Path, id: &str) -> Result<MemoryNote> {
    validate_uuid(id)?;
    for paths in selected_scopes(root, "all")? {
        let path = paths.notes_dir.join(format!("{id}.md"));
        match read_note(&paths, &path) {
            Ok(note) => return Ok(note),
            Err(DocodeError::Io { source, .. })
                if source.kind() == std::io::ErrorKind::NotFound =>
            {
                continue;
            }
            Err(error) => return Err(error),
        }
    }
    Err(DocodeError::Message(
        "memory note was not found in the current project or global store.".into(),
    ))
}

pub fn docodet(root: &Path, id: &str, approved: bool) -> Result<()> {
    if !approved {
        return Err(DocodeError::Safety(
            "docodetting memory requires --approve; no note was deleted.".into(),
        ));
    }
    let note = show(root, id)?;
    let paths = scope_paths(root, &note.scope)?;
    let path = paths.notes_dir.join(format!("{id}.md"));
    let _ = storage::read_bounded_regular(&path, MAX_NOTE_BYTES)?;
    fs::remove_file(&path).map_err(|e| io_error(&path, e))?;
    rebuild_index_path(&paths)?;
    Ok(())
}

pub fn edit(root: &Path, id: &str) -> Result<MemoryNote> {
    let original = show(root, id)?;
    let paths = scope_paths(root, &original.scope)?;
    let path = paths.notes_dir.join(format!("{id}.md"));
    let editor = std::env::var("VISUAL")
        .ok()
        .or_else(|| std::env::var("EDITOR").ok())
        .unwrap_or_else(|| {
            if cfg!(windows) {
                "notepad".into()
            } else {
                "vi".into()
            }
        });
    if editor.trim().is_empty()
        || editor.chars().any(|ch| {
            ch.is_whitespace() || ['\'', '\"', '`', ';', '$', '|', '&', '<', '>'].contains(&ch)
        })
    {
        return Err(DocodeError::Safety(
            "EDITOR/VISUAL must name one executable without arguments or shell expressions.".into(),
        ));
    }
    let status = std::process::Command::new(&editor)
        .arg(&path)
        .status()
        .map_err(|_| {
            DocodeError::Message(
                "could not start the configured editor; memory was not reindexed.".into(),
            )
        })?;
    if !status.success() {
        return Err(DocodeError::Message(format!(
            "editor exited with status {}; memory was not reindexed.",
            status.code().unwrap_or(1)
        )));
    }
    let bytes = storage::read_bounded_regular(&path, MAX_NOTE_BYTES)?;
    let source = String::from_utf8(bytes)
        .map_err(|_| DocodeError::Safety("edited memory note is not UTF-8.".into()))?;
    let edited = parse_markdown(&source)?;
    validate_note(&edited)?;
    if edited.id != original.id
        || edited.scope != original.scope
        || edited.project_id != original.project_id
    {
        return Err(DocodeError::Safety(
            "editing changed the memory ID, scope or project binding; refusing to reindex.".into(),
        ));
    }
    persist(&paths, &edited, true)?;
    rebuild_index_path(&paths)?;
    Ok(edited)
}

pub fn verify(root: &Path, id: &str) -> Result<MemoryNote> {
    let mut note = show(root, id)?;
    note.last_verified_at = Some(storage::timestamp());
    note.status = "active".into();
    let paths = scope_paths(root, &note.scope)?;
    persist(&paths, &note, true)?;
    rebuild_index_path(&paths)?;
    Ok(note)
}

pub fn reindex(root: &Path, scope: &str) -> Result<usize> {
    let paths = selected_scopes(root, scope)?;
    let mut total = 0;
    for item in paths {
        total += rebuild_index_path(&item)?;
    }
    Ok(total)
}

pub fn export(root: &Path, destination: &Path, scope: &str) -> Result<usize> {
    let notes = list(root, scope)?;
    if notes.len() > MAX_NOTES {
        return Err(DocodeError::Safety(
            "memory export exceeds the note-count limit.".into(),
        ));
    }
    let bundle = ExportBundle {
        schema_version: 1,
        exported_at: storage::timestamp(),
        notes,
    };
    let bytes =
        serde_json::to_vec_pretty(&bundle).map_err(|e| DocodeError::Message(e.to_string()))?;
    if bytes.len() as u64 > MAX_EXPORT_BYTES {
        return Err(DocodeError::Safety(
            "memory export exceeds the 20 MiB limit.".into(),
        ));
    }
    storage::write_atomic(destination, &bytes, false)?;
    Ok(bundle.notes.len())
}

pub fn import(root: &Path, source: &Path, confirmed: bool) -> Result<usize> {
    if !confirmed {
        return Err(DocodeError::Safety(
            "memory import requires explicit --confirm. No notes were written.".into(),
        ));
    }
    let bytes = storage::read_bounded_regular(source, MAX_EXPORT_BYTES)?;
    let bundle: ExportBundle = serde_json::from_slice(&bytes).map_err(|_| {
        DocodeError::Safety("memory import is not a valid Do Code JSON export.".into())
    })?;
    if bundle.schema_version != 1 || bundle.notes.len() > MAX_NOTES {
        return Err(DocodeError::Safety(
            "memory import schema or note-count limit is invalid.".into(),
        ));
    }
    let project_id = storage::project_id(root)?;
    // Validate every record before publishing any record, preventing partial cross-project imports.
    for note in &bundle.notes {
        validate_note(note)?;
        if note.scope == "project" && note.project_id.as_deref() != Some(project_id.as_str()) {
            return Err(DocodeError::Safety("memory import contains project notes bound to another project; no notes were written.".into()));
        }
    }
    for source_note in &bundle.notes {
        let scope = scope_paths(root, &source_note.scope)?;
        let mut note = source_note.clone();
        note.id = Uuid::new_v4().to_string();
        note.created_at = storage::timestamp();
        note.last_verified_at = None;
        note.user_confirmed = true;
        persist(&scope, &note, false)?;
        rebuild_index_path(&scope)?;
    }
    Ok(bundle.notes.len())
}

pub fn reset(root: &Path, scope: &str, confirmed: bool) -> Result<usize> {
    if !confirmed {
        return Err(DocodeError::Safety(
            "memory reset requires explicit --confirm. No notes were deleted.".into(),
        ));
    }
    let mut count = 0;
    for paths in selected_scopes(root, scope)? {
        for note in load_scope(&paths)? {
            fs::remove_file(paths.notes_dir.join(format!("{}.md", note.id)))
                .map_err(|e| io_error(&paths.notes_dir, e))?;
            count += 1;
        }
        rebuild_index_path(&paths)?;
    }
    Ok(count)
}

pub fn render_markdown(note: &MemoryNote) -> Result<String> {
    validate_note(note)?;
    let mut fields = vec![
        ("docode_schema_version", json!(note.schema_version)),
        ("docode_id", json!(note.id)),
        ("docode_type", json!(note.note_type)),
        ("docode_title", json!(note.title)),
        ("docode_problem", json!(note.problem)),
        ("docode_decision", json!(note.decision)),
        ("docode_reason", json!(note.reason)),
        ("docode_rejected_options", json!(note.rejected_options)),
        ("docode_limitations", json!(note.limitations)),
        ("docode_evidence", json!(note.evidence)),
        ("docode_confidence", json!(note.confidence)),
        ("docode_created_at", json!(note.created_at)),
        ("docode_last_verified_at", json!(note.last_verified_at)),
        ("docode_status", json!(note.status)),
        ("docode_scope", json!(note.scope)),
        ("docode_project_id", json!(note.project_id)),
        ("docode_user_confirmed", json!(note.user_confirmed)),
    ];
    fields.sort_by(|a, b| a.0.cmp(b.0));
    let mut output = String::from("---\n");
    for (key, value) in fields {
        output.push_str(&format!(
            "{key}: {}\n",
            serde_json::to_string(&value).map_err(|e| DocodeError::Message(e.to_string()))?
        ));
    }
    output.push_str(&format!("---\n\n# {}\n\n## Problem\n\n{}\n\n## Decision / solution\n\n{}\n\n## Reason\n\n{}\n\n## Rejected options\n\n{}\n\n## Limitations\n\n{}\n\n## Evidence / sources\n\n{}\n\n## Verification\n\n- Confidence: {:.2}\n- Created: {}\n- Last verified: {}\n- Status: {}\n- Scope: {}\n- User confirmed: {}\n",
        note.title, note.problem, note.decision, note.reason, bullet_list(&note.rejected_options), bullet_list(&note.limitations), bullet_list(&note.evidence), note.confidence, note.created_at, note.last_verified_at.as_deref().unwrap_or("never"), note.status, note.scope, if note.user_confirmed {"yes"} else {"no"}));
    Ok(crate::redaction::redact(&output))
}

fn scope_paths(root: &Path, scope: &str) -> Result<ScopePaths> {
    let canonical_root = fs::canonicalize(root).map_err(|e| io_error(root, e))?;
    let project_id = storage::project_id(&canonical_root)?;
    let (scope_dir, actual_scope, scoped_id) = match scope {
        "project" => (
            storage::ensure_project_home(&canonical_root)?
                .join("memory")
                .join("project"),
            "project".to_owned(),
            Some(project_id),
        ),
        "global" => {
            let home = std::env::var_os("USERPROFILE")
                .or_else(|| std::env::var_os("HOME"))
                .map(PathBuf::from)
                .ok_or_else(|| {
                    DocodeError::Config(
                        "could not resolve the user home directory for global memory.".into(),
                    )
                })?;
            let canonical_home = fs::canonicalize(&home).map_err(|e| io_error(&home, e))?;
            let app_home = canonical_home.join(".docode");
            storage::ensure_private_dir(&app_home)?;
            let memory_home = app_home.join("memory");
            storage::ensure_private_dir(&memory_home)?;
            (memory_home.join("global"), "global".to_owned(), None)
        }
        _ => {
            return Err(DocodeError::Message(
                "memory scope must be project, global, or all.".into(),
            ));
        }
    };
    storage::ensure_private_dir(&scope_dir)?;
    let notes_dir = scope_dir.join("notes");
    storage::ensure_private_dir(&notes_dir)?;
    Ok(ScopePaths {
        index_path: scope_dir.join("index.json"),
        scope_dir,
        notes_dir,
        scope: actual_scope,
        project_id: scoped_id,
    })
}

fn selected_scopes(root: &Path, scope: &str) -> Result<Vec<ScopePaths>> {
    match scope {
        "all" => Ok(vec![
            scope_paths(root, "project")?,
            scope_paths(root, "global")?,
        ]),
        "project" | "global" => Ok(vec![scope_paths(root, scope)?]),
        _ => Err(DocodeError::Message(
            "memory scope must be project, global, or all.".into(),
        )),
    }
}
fn load_scope(paths: &ScopePaths) -> Result<Vec<MemoryNote>> {
    let mut names = Vec::new();
    for entry in fs::read_dir(&paths.notes_dir).map_err(|e| io_error(&paths.notes_dir, e))? {
        let entry = entry.map_err(|e| io_error(&paths.notes_dir, e))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.ends_with(".md") && Uuid::parse_str(name.trim_end_matches(".md")).is_ok() {
            names.push(entry.path());
        }
    }
    names.sort();
    names.iter().map(|path| read_note(paths, path)).collect()
}
fn read_note(paths: &ScopePaths, path: &Path) -> Result<MemoryNote> {
    let bytes = storage::read_bounded_regular(path, MAX_NOTE_BYTES)?;
    let source = String::from_utf8(bytes)
        .map_err(|_| DocodeError::Safety("memory note is not UTF-8 text.".into()))?;
    let note = parse_markdown(&source)?;
    validate_note(&note)?;
    if note.scope != paths.scope
        || note.project_id != paths.project_id
        || path.file_stem().and_then(|s| s.to_str()) != Some(note.id.as_str())
    {
        return Err(DocodeError::Safety(
            "memory note scope/project/filename binding failed.".into(),
        ));
    }
    Ok(note)
}
fn persist(paths: &ScopePaths, note: &MemoryNote, replace: bool) -> Result<()> {
    validate_note(note)?;
    if note.scope != paths.scope || note.project_id != paths.project_id {
        return Err(DocodeError::Safety(
            "memory note belongs to a different scope or project.".into(),
        ));
    }
    let path = paths.notes_dir.join(format!("{}.md", note.id));
    let markdown = render_markdown(note)?;
    if markdown.len() as u64 > MAX_NOTE_BYTES {
        return Err(DocodeError::Safety(
            "memory note exceeds the 256 KiB limit.".into(),
        ));
    }
    storage::write_atomic(&path, markdown.as_bytes(), replace)
}
fn rebuild_index_path(paths: &ScopePaths) -> Result<usize> {
    let notes = load_scope(paths)?;
    if notes.len() > MAX_NOTES {
        return Err(DocodeError::Safety(
            "memory scope exceeds 10000 notes.".into(),
        ));
    }
    let mut entries = Vec::new();
    for note in &notes {
        let filename = format!("{}.md", note.id);
        let path = paths.notes_dir.join(&filename);
        let bytes = storage::read_bounded_regular(&path, MAX_NOTE_BYTES)?;
        entries.push(MemoryIndexEntry {
            id: note.id.clone(),
            filename,
            sha256: hash(&bytes),
            note_type: note.note_type.clone(),
            title: note.title.clone(),
            scope: note.scope.clone(),
            project_id: note.project_id.clone(),
            last_indexed_at: storage::timestamp(),
        });
    }
    entries.sort_by(|a, b| a.id.cmp(&b.id));
    let index = MemoryIndex {
        schema_version: NOTE_VERSION,
        entries,
    };
    let bytes = serde_json::to_vec(&index).map_err(|e| DocodeError::Message(e.to_string()))?;
    storage::write_atomic(&paths.index_path, &bytes, paths.index_path.exists())?;
    Ok(index.entries.len())
}
fn parse_markdown(source: &str) -> Result<MemoryNote> {
    if source.len() as u64 > MAX_NOTE_BYTES {
        return Err(DocodeError::Safety("memory note exceeds 256 KiB.".into()));
    }
    let lines = source.lines().collect::<Vec<_>>();
    if lines.first() != Some(&"---") {
        return Err(DocodeError::Safety(
            "memory note has no YAML frontmatter.".into(),
        ));
    }
    let end = lines
        .iter()
        .enumerate()
        .skip(1)
        .find(|(_, line)| **line == "---")
        .map(|(i, _)| i)
        .ok_or_else(|| DocodeError::Safety("memory note frontmatter is not closed.".into()))?;
    let mut meta = Map::new();
    for line in &lines[1..end] {
        let (key, raw) = line.split_once(": ").ok_or_else(|| {
            DocodeError::Safety("memory note frontmatter contains a malformed line.".into())
        })?;
        if !key.starts_with("docode_") || meta.contains_key(key) {
            return Err(DocodeError::Safety(
                "memory frontmatter contains an unknown/duplicate key.".into(),
            ));
        }
        let value: Value = serde_json::from_str(raw).map_err(|_| {
            DocodeError::Safety("memory frontmatter contains a non-JSON value.".into())
        })?;
        meta.insert(key.to_owned(), value);
    }
    let get = |key: &str| {
        meta.get(key)
            .cloned()
            .ok_or_else(|| DocodeError::Safety(format!("memory frontmatter is missing {key}.")))
    };
    Ok(MemoryNote {
        schema_version: u8::try_from(value_u64(get("docode_schema_version")?)?)
            .map_err(|_| DocodeError::Safety("memory schema version is out of range.".into()))?,
        id: value_string(get("docode_id")?)?,
        note_type: value_string(get("docode_type")?)?,
        title: value_string(get("docode_title")?)?,
        problem: value_string(get("docode_problem")?)?,
        decision: value_string(get("docode_decision")?)?,
        reason: value_string(get("docode_reason")?)?,
        rejected_options: value_strings(get("docode_rejected_options")?)?,
        limitations: value_strings(get("docode_limitations")?)?,
        evidence: value_strings(get("docode_evidence")?)?,
        confidence: get("docode_confidence")?
            .as_f64()
            .ok_or_else(|| DocodeError::Safety("memory confidence must be a number.".into()))?,
        created_at: value_string(get("docode_created_at")?)?,
        last_verified_at: match get("docode_last_verified_at")? {
            Value::Null => None,
            v => Some(value_string(v)?),
        },
        status: value_string(get("docode_status")?)?,
        scope: value_string(get("docode_scope")?)?,
        project_id: match get("docode_project_id")? {
            Value::Null => None,
            v => Some(value_string(v)?),
        },
        user_confirmed: get("docode_user_confirmed")?.as_bool().ok_or_else(|| {
            DocodeError::Safety("memory confirmation marker must be boolean.".into())
        })?,
    })
}
fn validate_note(note: &MemoryNote) -> Result<()> {
    validate_uuid(&note.id)?;
    if note.schema_version != NOTE_VERSION
        || ![
            "solution",
            "decision",
            "research",
            "user_preference",
            "project",
            "failure",
            "workflow",
        ]
        .contains(&note.note_type.as_str())
        || !["active", "outdated", "disputed"].contains(&note.status.as_str())
        || !["global", "project"].contains(&note.scope.as_str())
    {
        return Err(DocodeError::Safety(
            "memory note schema, type, status, or scope is invalid.".into(),
        ));
    }
    if note.title.trim().is_empty()
        || note.title.len() > 300
        || note.problem.len() > 10_000
        || note.decision.trim().is_empty()
        || note.decision.len() > 20_000
        || note.reason.len() > 10_000
        || note.rejected_options.len() > 100
        || note.limitations.len() > 100
        || note.evidence.len() > 100
        || !(0.0..=1.0).contains(&note.confidence)
        || !note.confidence.is_finite()
        || !note.user_confirmed
    {
        return Err(DocodeError::Safety(
            "memory note exceeds a field limit, has invalid confidence, or was not user-confirmed."
                .into(),
        ));
    }
    if note.scope == "global" && note.project_id.is_some() {
        return Err(DocodeError::Safety(
            "global memory must not be bound to a project ID.".into(),
        ));
    }
    if note.scope == "project" && note.project_id.as_deref().is_none_or(str::is_empty) {
        return Err(DocodeError::Safety(
            "project memory must be bound to a project ID.".into(),
        ));
    }
    if matches!(
        note.note_type.as_str(),
        "solution" | "research" | "failure" | "workflow"
    ) && note.evidence.is_empty()
    {
        return Err(DocodeError::Safety(
            "solution/research/failure/workflow memory requires at least one evidence reference."
                .into(),
        ));
    }
    let content = [
        note.title.as_str(),
        note.problem.as_str(),
        note.decision.as_str(),
        note.reason.as_str(),
    ]
    .join("\n");
    if crate::redaction::redact(&content) != content
        || note
            .evidence
            .iter()
            .chain(note.rejected_options.iter())
            .chain(note.limitations.iter())
            .any(|s| crate::redaction::redact(s) != *s)
    {
        return Err(DocodeError::Safety(
            "memory contains recognizable credential-like content; it was not saved.".into(),
        ));
    }
    Ok(())
}
fn value_string(value: Value) -> Result<String> {
    value
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| DocodeError::Safety("memory frontmatter value must be a string.".into()))
}
fn value_strings(value: Value) -> Result<Vec<String>> {
    let array = value
        .as_array()
        .ok_or_else(|| DocodeError::Safety("memory list field must be an array.".into()))?;
    array
        .iter()
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .ok_or_else(|| DocodeError::Safety("memory list entries must be strings.".into()))
        })
        .collect()
}
fn value_u64(value: Value) -> Result<u64> {
    value
        .as_u64()
        .ok_or_else(|| DocodeError::Safety("memory schema version must be an integer.".into()))
}
fn validate_uuid(id: &str) -> Result<()> {
    Uuid::parse_str(id)
        .map(|_| ())
        .map_err(|_| DocodeError::Message("memory note ID must be a UUID.".into()))
}
fn bullet_list(items: &[String]) -> String {
    if items.is_empty() {
        "- None recorded".to_owned()
    } else {
        items
            .iter()
            .map(|item| format!("- {}", crate::redaction::safe_terminal(item)))
            .collect::<Vec<_>>()
            .join("\n")
    }
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
    fn memory_requires_confirmation_and_evidence_for_solution_notes() {
        let root = tempdir().unwrap();
        assert!(
            save(
                root.path(),
                "decision",
                "Storage choice",
                "Need local state",
                "Use JSON",
                "Readable diff",
                vec![],
                vec![],
                vec![],
                0.8,
                "project",
                false
            )
            .is_err()
        );
        assert!(
            save(
                root.path(),
                "solution",
                "Fix crash",
                "Crash on startup",
                "Validate config before run",
                "Observed in logs",
                vec![],
                vec![],
                vec![],
                0.8,
                "project",
                true
            )
            .is_err()
        );
        let note = save(
            root.path(),
            "solution",
            "Fix crash",
            "Crash on startup",
            "Validate config before run",
            "Observed in logs",
            vec![],
            vec![],
            vec!["evidence://test-log-1".into()],
            0.8,
            "project",
            true,
        )
        .unwrap();
        assert_eq!(
            show(root.path(), &note.id).unwrap().decision,
            "Validate config before run"
        );
        assert_eq!(search(root.path(), "config", "project").unwrap().len(), 1);
        assert_eq!(reindex(root.path(), "project").unwrap(), 1);
    }
    #[test]
    fn markdown_round_trip_and_scope_binding() {
        let root = tempdir().unwrap();
        let note = save(
            root.path(),
            "decision",
            "SQLite or JSON",
            "Need persistence",
            "Start with JSON",
            "Smaller milestone",
            vec!["SQLite".into()],
            vec!["No transactions".into()],
            vec!["decision-record-1".into()],
            0.7,
            "project",
            true,
        )
        .unwrap();
        let paths = scope_paths(root.path(), "project").unwrap();
        let markdown = fs::read_to_string(paths.notes_dir.join(format!("{}.md", note.id))).unwrap();
        assert_eq!(parse_markdown(&markdown).unwrap().id, note.id);
        assert!(docodet(root.path(), &note.id, false).is_err());
        docodet(root.path(), &note.id, true).unwrap();
        assert!(show(root.path(), &note.id).is_err());
    }
}
