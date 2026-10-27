//! Spec-first contract gate.
//!
//! No Coder work may start until the mission has a contract: the goal,
//! success criteria, allowed/forbidden files, required tests, security
//! requirements, and rollback plan. The workflow derives the contract
//! deterministically from the approved plan and the collected context, so
//! the gate never depends on a model "remembering" the constraints — they
//! are enforced as data before the first patch request is sent.

use crate::context::ContextPack;
use crate::error::{ForgeError, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MissionContract {
    pub goal: String,
    pub success_criteria: Vec<String>,
    /// Workspace-relative paths the patch is allowed to touch.
    pub allowed_paths: Vec<String>,
    /// Paths that must never be modified by this mission.
    pub forbidden_paths: Vec<String>,
    /// Test commands the verification contract will require.
    pub required_tests: Vec<String>,
    pub security_requirements: Vec<String>,
    pub rollback_plan: String,
    /// Set when the contract was accepted for this mission.
    pub approved: bool,
}

impl MissionContract {
    /// Deterministically derive the contract from the mission inputs. The
    /// allowed set is exactly the context's complete-file set (the same set
    /// the patch applier enforces), and forbidden paths always include the
    /// sensitive-path floor.
    pub fn derive(task: &str, pack: &ContextPack, test_plan: &str) -> Self {
        let allowed_paths = pack
            .files
            .iter()
            .filter(|f| f.complete)
            .map(|f| f.path.clone())
            .collect();
        let required_tests = test_plan
            .lines()
            .map(str::trim)
            .filter(|l| l.len() > 3 && (l.contains("test") || l.contains("Test")))
            .take(10)
            .map(str::to_string)
            .collect();
        Self {
            goal: task.trim().to_string(),
            success_criteria: vec![
                "patch applies only to allowed paths with matching pre-condition hashes".into(),
                "required tests are identified in the contract before implementation".into(),
                "verification report records exactly what ran and what did not".into(),
            ],
            allowed_paths,
            forbidden_paths: forbidden_path_floor(),
            required_tests,
            security_requirements: vec![
                "no secrets or credentials may appear in the patch".into(),
                "no network access is performed by this mission".into(),
            ],
            rollback_plan: "forge undo --approve restores checkpointed file bytes exactly".into(),
            approved: true,
        }
    }

    /// Validate the contract before the implement step may start.
    pub fn validate(&self) -> Result<()> {
        if !self.approved {
            return Err(ForgeError::Safety(
                "mission contract is not approved; the coder may not start.".into(),
            ));
        }
        if self.goal.trim().len() < 8 {
            return Err(ForgeError::Safety(
                "mission contract goal is missing or too short.".into(),
            ));
        }
        if self.allowed_paths.is_empty() {
            return Err(ForgeError::Safety(
                "mission contract allows no files; nothing can be implemented.".into(),
            ));
        }
        Ok(())
    }

    /// True when `path` is inside the allowed set.
    pub fn allows(&self, path: &str) -> bool {
        self.allowed_paths.iter().any(|p| p == path)
    }
}

/// The forbidden-path floor every contract carries, mirroring the scanner's
/// sensitive-path policy.
fn forbidden_path_floor() -> Vec<String> {
    [
        ".env",
        ".env.*",
        ".git/",
        ".forge/",
        "node_modules/",
        "id_rsa",
        "credentials",
        "secrets.*",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::ContextFile;

    fn pack(paths: &[&str]) -> ContextPack {
        let len = paths.len();
        ContextPack {
            files: paths
                .iter()
                .map(|p| ContextFile {
                    path: (*p).into(),
                    sha256: "x".into(),
                    content: "fn main() {}".into(),
                    relevance: 1,
                    complete: true,
                })
                .collect(),
            total_bytes: len * 10,
            index_file_count: len,
            skipped_secret_like: 0,
        }
    }

    #[test]
    fn derived_contract_allows_context_files_and_requires_approval() {
        let c = MissionContract::derive(
            "implement the add function in add.py",
            &pack(&["add.py"]),
            "1. Run pytest to verify add()\n2. Test edge cases",
        );
        c.validate().unwrap();
        assert!(c.allows("add.py"));
        assert!(!c.allows("other.py"));
        assert_eq!(c.required_tests.len(), 2);
        assert!(!c.forbidden_paths.is_empty());
    }

    #[test]
    fn unapproved_contract_blocks_the_coder() {
        let mut c = MissionContract::derive("implement add in add.py", &pack(&["add.py"]), "");
        c.approved = false;
        assert!(c.validate().is_err());
    }

    #[test]
    fn contract_without_allowed_files_is_rejected() {
        let c = MissionContract::derive("implement add in add.py", &pack(&[]), "");
        assert!(c.validate().is_err());
    }

    #[test]
    fn test_plan_lines_without_test_keywords_are_not_requirements() {
        let c = MissionContract::derive(
            "implement add in add.py",
            &pack(&["add.py"]),
            "add two numbers\nthen run the test suite",
        );
        assert_eq!(
            c.required_tests,
            vec!["then run the test suite".to_string()]
        );
    }
}
