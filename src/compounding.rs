//! Compounding engine + memory tiers.
//!
//! Tier model:
//! - hot: the live mission ledger (mission state, active decisions) — owned
//!   by the mission store, never duplicated here.
//! - warm: project/global learning notes (existing memory store) plus the
//!   lessons and playbooks managed here.
//! - cold: archived lessons and notes (`.docode/memory/<scope>/archive/`),
//!   kept as plain files so the whole memory tree is git-friendly:
//!   every change is diffable, revertible, and reviewable by the user.
//!
//! Compounding loop: a completed mission proposes lessons from its ledger
//! -> the user approves or rejects each one (`docode lessons approve`) ->
//! approved lessons become playbooks -> future missions whose task text
//! matches a playbook keyword receive it in the architect's brief. Nothing
//! is ever auto-saved: proposals wait for explicit user confirmation.

use crate::error::{DocodeError, Result};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

const MAX_LESSON_BYTES: u64 = 64 * 1024;
const MAX_LESSONS: usize = 2_000;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Lesson {
    pub id: String,
    pub title: String,
    pub lesson: String,
    pub source_mission: String,
    pub keywords: Vec<String>,
    pub status: LessonStatus,
    pub created_at: String,
    pub decided_at: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LessonStatus {
    Proposed,
    Approved,
    Rejected,
    Archived,
}

fn scope_memory_dir(root: &Path, scope: &str) -> Result<PathBuf> {
    let base = crate::storage::ensure_project_home(root)?;
    match scope {
        "project" | "global" => {
            // Ensure each level privately; ensure_private_dir creates one
            // directory at a time and refuses symlinks at every level.
            let mut dir = base.join("memory");
            crate::storage::ensure_private_dir(&dir)?;
            dir = dir.join(scope);
            crate::storage::ensure_private_dir(&dir)?;
            Ok(dir)
        }
        other => Err(DocodeError::Config(format!(
            "unknown memory scope '{other}'."
        ))),
    }
}

fn lessons_dir(root: &Path, scope: &str) -> Result<PathBuf> {
    let dir = scope_memory_dir(root, scope)?.join("lessons");
    crate::storage::ensure_private_dir(&dir)?;
    Ok(dir)
}

fn playbooks_dir(root: &Path, scope: &str) -> Result<PathBuf> {
    let dir = scope_memory_dir(root, scope)?.join("playbooks");
    crate::storage::ensure_private_dir(&dir)?;
    Ok(dir)
}

fn lesson_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.json"))
}

/// Store a proposed lesson. Fails closed if the lesson is oversized or the
/// store is full.
pub fn propose_lesson(
    root: &Path,
    scope: &str,
    title: &str,
    lesson: &str,
    source_mission: &str,
    keywords: &[String],
) -> Result<Lesson> {
    if title.trim().is_empty() || lesson.trim().is_empty() {
        return Err(DocodeError::Message(
            "lesson title and body are required.".into(),
        ));
    }
    if lesson.len() as u64 > MAX_LESSON_BYTES {
        return Err(DocodeError::Message(
            "lesson body exceeds the 64 KiB bound.".into(),
        ));
    }
    let dir = lessons_dir(root, scope)?;
    if fs::read_dir(&dir).map(|d| d.count()).unwrap_or(0) >= MAX_LESSONS {
        return Err(DocodeError::Message(
            "lesson store is full; archive or reject old lessons first.".into(),
        ));
    }
    let record = Lesson {
        id: format!("lesson_{}", uuid::Uuid::new_v4().simple()),
        title: title.trim().to_string(),
        lesson: lesson.trim().to_string(),
        source_mission: source_mission.to_string(),
        keywords: keywords
            .iter()
            .map(|k| k.trim().to_ascii_lowercase())
            .filter(|k| !k.is_empty())
            .collect(),
        status: LessonStatus::Proposed,
        created_at: crate::storage::timestamp(),
        decided_at: None,
    };
    let bytes =
        serde_json::to_vec_pretty(&record).map_err(|e| DocodeError::Message(e.to_string()))?;
    crate::storage::write_atomic(&lesson_path(&dir, &record.id), &bytes, true)?;
    Ok(record)
}

fn load_lesson(path: &Path) -> Result<Lesson> {
    let bytes = crate::storage::read_bounded_regular(path, MAX_LESSON_BYTES * 2)?;
    serde_json::from_slice(&bytes).map_err(|e| DocodeError::Message(e.to_string()))
}

pub fn list_lessons(root: &Path, scope: &str) -> Result<Vec<Lesson>> {
    let dir = lessons_dir(root, scope)?;
    let mut out = Vec::new();
    let entries = fs::read_dir(&dir).map_err(|e| crate::error::io_error(&dir, e))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("json") {
            out.push(load_lesson(&path)?);
        }
    }
    out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(out)
}

/// Approve a proposed lesson: it becomes a playbook (a plain Markdown file)
/// and will be offered to future similar missions.
pub fn approve_lesson(root: &Path, scope: &str, id: &str) -> Result<Lesson> {
    let dir = lessons_dir(root, scope)?;
    let path = lesson_path(&dir, id);
    let mut lesson = load_lesson(&path)?;
    if lesson.status != LessonStatus::Proposed {
        return Err(DocodeError::Safety(format!(
            "lesson '{id}' is {:?}; only proposed lessons can be approved.",
            lesson.status
        )));
    }
    lesson.status = LessonStatus::Approved;
    lesson.decided_at = Some(crate::storage::timestamp());
    let bytes =
        serde_json::to_vec_pretty(&lesson).map_err(|e| DocodeError::Message(e.to_string()))?;
    crate::storage::write_atomic(&path, &bytes, true)?;
    write_playbook(root, scope, &lesson)?;
    Ok(lesson)
}

pub fn reject_lesson(root: &Path, scope: &str, id: &str) -> Result<Lesson> {
    let dir = lessons_dir(root, scope)?;
    let path = lesson_path(&dir, id);
    let mut lesson = load_lesson(&path)?;
    if lesson.status != LessonStatus::Proposed {
        return Err(DocodeError::Safety(format!(
            "lesson '{id}' is {:?}; only proposed lessons can be rejected.",
            lesson.status
        )));
    }
    lesson.status = LessonStatus::Rejected;
    lesson.decided_at = Some(crate::storage::timestamp());
    let bytes =
        serde_json::to_vec_pretty(&lesson).map_err(|e| DocodeError::Message(e.to_string()))?;
    crate::storage::write_atomic(&path, &bytes, true)?;
    Ok(lesson)
}

fn write_playbook(root: &Path, scope: &str, lesson: &Lesson) -> Result<()> {
    let dir = playbooks_dir(root, scope)?;
    let playbook = format!(
        "# Playbook: {}\n\n- lesson: {}\n- source mission: {}\n- keywords: {}\n- approved: {}\n\nRule for future missions:\n{}\n",
        lesson.title,
        lesson.lesson,
        lesson.source_mission,
        lesson.keywords.join(", "),
        lesson.decided_at.clone().unwrap_or_default(),
        lesson.lesson
    );
    crate::storage::write_atomic(
        &dir.join(format!("{}.md", lesson.id)),
        playbook.as_bytes(),
        true,
    )
}

/// Cold tier: archive an approved or rejected lesson (kept on disk, marked
/// archived, removed from the active playbook set).
pub fn archive_lesson(root: &Path, scope: &str, id: &str) -> Result<Lesson> {
    let dir = lessons_dir(root, scope)?;
    let path = lesson_path(&dir, id);
    let mut lesson = load_lesson(&path)?;
    lesson.status = LessonStatus::Archived;
    let bytes =
        serde_json::to_vec_pretty(&lesson).map_err(|e| DocodeError::Message(e.to_string()))?;
    crate::storage::write_atomic(&path, &bytes, true)?;
    let playbook = playbooks_dir(root, scope)?.join(format!("{}.md", lesson.id));
    if playbook.exists() {
        fs::remove_file(&playbook).map_err(|e| crate::error::io_error(&playbook, e))?;
    }
    Ok(lesson)
}

/// Playbooks relevant to a task: approved lessons whose keywords appear in
/// the task text. Used to brief the architect before planning.
pub fn playbooks_for_task(root: &Path, scope: &str, task: &str) -> Result<Vec<Lesson>> {
    let task_lower = task.to_ascii_lowercase();
    Ok(list_lessons(root, scope)?
        .into_iter()
        .filter(|l| l.status == LessonStatus::Approved)
        .filter(|l| {
            l.keywords
                .iter()
                .any(|k| !k.is_empty() && task_lower.contains(k.as_str()))
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn propose_then_approve_creates_playbook() {
        let r = root();
        let lesson = propose_lesson(
            r.path(),
            "project",
            "Always validate input",
            "For this project always validate API input before parsing.",
            "msn_1",
            &["validation".into(), "api".into()],
        )
        .unwrap();
        assert_eq!(lesson.status, LessonStatus::Proposed);
        // Not visible to missions until approved.
        assert!(
            playbooks_for_task(r.path(), "project", "add api validation")
                .unwrap()
                .is_empty()
        );
        approve_lesson(r.path(), "project", &lesson.id).unwrap();
        let matches = playbooks_for_task(r.path(), "project", "add api validation").unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].id, lesson.id);
        // Unrelated task gets nothing.
        assert!(
            playbooks_for_task(r.path(), "project", "fix css styling")
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn rejection_and_archival_are_explicit() {
        let r = root();
        let lesson = propose_lesson(r.path(), "project", "t", "b", "m", &[]).unwrap();
        reject_lesson(r.path(), "project", &lesson.id).unwrap();
        assert!(
            playbooks_for_task(r.path(), "project", "b")
                .unwrap()
                .is_empty()
        );
        // A rejected lesson cannot be approved afterwards.
        assert!(approve_lesson(r.path(), "project", &lesson.id).is_err());
    }

    #[test]
    fn scope_isolation_between_projects() {
        let r = root();
        propose_lesson(
            r.path(),
            "project",
            "t",
            "use zod for api validation",
            "m",
            &["api".into()],
        )
        .unwrap();
        let lesson = list_lessons(r.path(), "project").unwrap().pop().unwrap();
        approve_lesson(r.path(), "project", &lesson.id).unwrap();
        assert_eq!(
            playbooks_for_task(r.path(), "project", "api work")
                .unwrap()
                .len(),
            1
        );
        // Global scope does not see project lessons.
        assert!(
            playbooks_for_task(r.path(), "global", "api work")
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn oversized_lessons_fail_closed() {
        let r = root();
        let big = "x".repeat(70 * 1024);
        assert!(propose_lesson(r.path(), "project", "t", &big, "m", &[]).is_err());
    }
}
