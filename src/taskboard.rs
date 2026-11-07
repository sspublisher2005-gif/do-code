//! Shared task board + file-lock registry.
//!
//! A mission's plan is decomposed into tasks with owners, file locks,
//! dependencies, and acceptance criteria. Locks make conflicting concurrent
//! work detectable before it happens: two tasks that lock the same file
//! cannot both be ready at once. State persists as JSON next to the mission
//! so `docode tasks` and resume can show the board.

use crate::error::{DocodeError, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Ready,
    InProgress,
    Done,
    Blocked,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub owner: String,
    pub status: TaskStatus,
    pub files_locked: Vec<String>,
    pub depends_on: Vec<String>,
    pub acceptance: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct TaskBoard {
    pub tasks: Vec<Task>,
}

impl TaskBoard {
    /// Decompose a plan into tasks. Complex plans get one task per file
    /// target (each with its own lock); anything smaller is one task.
    pub fn from_plan(plan_targets: &[String], owner: &str, acceptance: &[String]) -> Self {
        if plan_targets.len() <= 1 {
            let files = plan_targets.to_vec();
            return Self {
                tasks: vec![Task {
                    id: "task_01".into(),
                    title: if files.is_empty() {
                        "single-task mission".into()
                    } else {
                        format!("implement {}", files[0])
                    },
                    owner: owner.into(),
                    status: TaskStatus::Ready,
                    files_locked: files,
                    depends_on: vec![],
                    acceptance: acceptance.to_vec(),
                }],
            };
        }
        let tasks = plan_targets
            .iter()
            .enumerate()
            .map(|(i, path)| Task {
                id: format!("task_{:02}", i + 1),
                title: format!("implement {path}"),
                owner: owner.into(),
                status: TaskStatus::Ready,
                files_locked: vec![path.clone()],
                depends_on: vec![],
                acceptance: acceptance.to_vec(),
            })
            .collect();
        Self { tasks }
    }

    /// Tasks whose dependencies are all done and that hold no conflicting
    /// lock with another in-progress task.
    pub fn ready_tasks(&self) -> Vec<&Task> {
        let done: BTreeSet<&str> = self
            .tasks
            .iter()
            .filter(|t| t.status == TaskStatus::Done)
            .map(|t| t.id.as_str())
            .collect();
        let active_locks: BTreeSet<&String> = self
            .tasks
            .iter()
            .filter(|t| t.status == TaskStatus::InProgress)
            .flat_map(|t| t.files_locked.iter())
            .collect();
        self.tasks
            .iter()
            .filter(|t| {
                t.status == TaskStatus::Ready
                    && t.depends_on.iter().all(|d| done.contains(d.as_str()))
                    && !t.files_locked.iter().any(|f| active_locks.contains(f))
            })
            .collect()
    }

    /// Mark a task in progress; fails when another task already holds one of
    /// its file locks (conflict detection).
    pub fn start_task(&mut self, id: &str) -> Result<()> {
        let task = self
            .tasks
            .iter()
            .find(|t| t.id == id)
            .ok_or_else(|| DocodeError::Message(format!("task '{id}' is not on the board.")))?;
        let conflicting: Vec<String> = self
            .tasks
            .iter()
            .filter(|t| t.id != id && t.status == TaskStatus::InProgress)
            .flat_map(|t| t.files_locked.iter().cloned())
            .filter(|f| task.files_locked.contains(f))
            .collect();
        if let Some(file) = conflicting.first() {
            return Err(DocodeError::Safety(format!(
                "file lock conflict: '{file}' is locked by another in-progress task."
            )));
        }
        let task = self
            .tasks
            .iter_mut()
            .find(|t| t.id == id)
            .expect("checked above");
        task.status = TaskStatus::InProgress;
        Ok(())
    }

    pub fn complete_task(&mut self, id: &str) -> Result<()> {
        let task = self
            .tasks
            .iter_mut()
            .find(|t| t.id == id)
            .ok_or_else(|| DocodeError::Message(format!("task '{id}' is not on the board.")))?;
        task.status = TaskStatus::Done;
        Ok(())
    }

    /// Serialize to JSON for `.docode/missions/<id>/tasks.json`.
    pub fn to_json(&self) -> Result<Vec<u8>> {
        serde_json::to_vec_pretty(self).map_err(|e| DocodeError::Message(e.to_string()))
    }

    pub fn from_json(bytes: &[u8]) -> Result<Self> {
        serde_json::from_slice(bytes).map_err(|e| DocodeError::Message(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn board() -> TaskBoard {
        let b = TaskBoard::from_plan(
            &["src/a.rs".to_string(), "src/b.rs".to_string()],
            "coder",
            &["tests pass".to_string()],
        );
        assert_eq!(b.tasks.len(), 2);
        b
    }

    #[test]
    fn per_file_tasks_get_distinct_locks() {
        let b = board();
        assert_eq!(b.tasks[0].files_locked, vec!["src/a.rs"]);
        assert_eq!(b.tasks[1].files_locked, vec!["src/b.rs"]);
    }

    #[test]
    fn single_target_plan_is_one_task() {
        let b = TaskBoard::from_plan(&["src/a.rs".into()], "coder", &[]);
        assert_eq!(b.tasks.len(), 1);
        assert_eq!(b.tasks[0].id, "task_01");
    }

    #[test]
    fn start_and_complete_round_trip() {
        let mut b = board();
        b.start_task("task_01").unwrap();
        assert_eq!(b.tasks[0].status, TaskStatus::InProgress);
        b.complete_task("task_01").unwrap();
        assert_eq!(b.tasks[0].status, TaskStatus::Done);
        assert_eq!(b.ready_tasks().len(), 1);
        assert_eq!(b.ready_tasks()[0].id, "task_02");
    }

    #[test]
    fn dependency_gates_readiness() {
        let mut b = board();
        b.tasks[1].depends_on = vec!["task_01".into()];
        assert_eq!(b.ready_tasks().len(), 1);
        assert_eq!(b.ready_tasks()[0].id, "task_01");
    }

    #[test]
    fn lock_conflict_is_rejected() {
        let mut b = board();
        b.tasks[1].files_locked = vec!["src/a.rs".into()];
        b.start_task("task_01").unwrap();
        let err = b.start_task("task_02").unwrap_err();
        assert!(err.to_string().contains("file lock conflict"));
    }

    #[test]
    fn json_round_trip_preserves_board() {
        let mut b = board();
        b.start_task("task_01").unwrap();
        let bytes = b.to_json().unwrap();
        let parsed = TaskBoard::from_json(&bytes).unwrap();
        assert_eq!(parsed.tasks[0].status, TaskStatus::InProgress);
        assert_eq!(parsed.tasks[1].files_locked, vec!["src/b.rs"]);
    }
}
