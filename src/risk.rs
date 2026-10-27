//! Risk engine + approval memory.
//!
//! Every action Forge is about to take gets a risk score 0-5. Risk 0-1 runs
//! silently, risk 2 asks once and remembers, risk 3+ always asks fresh, and
//! risk 5 is never auto-approved. Approval memory stores decisions at the
//! scope the user chose (once / this mission / this project) so Forge never
//! asks the same question twice — and never silently repeats a dangerous
//! one.

use crate::error::{ForgeError, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Risk {
    /// Read-only or easily reversible local work.
    Safe = 0,
    /// Normal local mutations (branch, local commit, reports).
    Normal = 1,
    /// Reviewable: dependency install, new MCP server, package install.
    Reviewable = 2,
    /// Risky: network request, unfamiliar script, CI modification.
    Risky = 3,
    /// Dangerous: deletion, force-push, secret access.
    Dangerous = 4,
    /// Critical: destruction that can never be auto-approved.
    Critical = 5,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RiskAssessment {
    pub risk: Risk,
    pub reason: String,
}

/// Score a shell-style command string. Deterministic, no I/O. Used by the
/// policy hooks now and by the sandbox driver when execution lands.
pub fn score_command(command: &str) -> RiskAssessment {
    let lower = command.to_ascii_lowercase();
    let destructive = [
        "rm -rf",
        "rmdir /s",
        "rd /s",
        "del /f",
        "format ",
        "mkfs",
        "git push --force",
        "git push -f",
        "git reset --hard",
        "git clean -fd",
    ];
    if destructive.iter().any(|p| lower.contains(p)) {
        return RiskAssessment {
            risk: Risk::Critical,
            reason: "destructive or history-rewriting command".into(),
        };
    }
    if lower.contains("sudo") || lower.contains("runas") {
        return RiskAssessment {
            risk: Risk::Dangerous,
            reason: "privilege elevation".into(),
        };
    }
    if ["curl", "wget", "invoke-webrequest", "irm", "iex"]
        .iter()
        .any(|p| lower.starts_with(p) || lower.contains(&format!("| {p}")))
    {
        return RiskAssessment {
            risk: Risk::Risky,
            reason: "network fetch or remote script execution".into(),
        };
    }
    if [
        "npm install",
        "pnpm install",
        "yarn add",
        "pip install",
        "cargo add",
        "cargo install",
    ]
    .iter()
    .any(|p| lower.contains(p))
    {
        return RiskAssessment {
            risk: Risk::Reviewable,
            reason: "installs third-party packages".into(),
        };
    }
    if lower.contains(".env") || lower.contains("id_rsa") || lower.contains("credential") {
        return RiskAssessment {
            risk: Risk::Dangerous,
            reason: "touches credential-shaped paths".into(),
        };
    }
    if lower.starts_with("git status")
        || lower.starts_with("git diff")
        || lower.starts_with("git log")
        || lower.starts_with("cargo test")
        || lower.starts_with("npm test")
        || lower.starts_with("pytest")
    {
        return RiskAssessment {
            risk: Risk::Safe,
            reason: "read-only or project-local verification".into(),
        };
    }
    RiskAssessment {
        risk: Risk::Normal,
        reason: "ordinary project-local command".into(),
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalScope {
    Once,
    Mission,
    Project,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ApprovalRecord {
    pub action: String,
    pub scope: ApprovalScope,
    pub mission_id: Option<String>,
    pub decision: String,
    pub recorded_at: String,
}

/// Approval memory persisted at `.forge/approvals.json`.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct ApprovalMemory {
    pub records: Vec<ApprovalRecord>,
}

impl ApprovalMemory {
    /// Should Forge ask before performing `action` inside `mission_id`?
    /// Risk 2 (reviewable) honors remembered approvals; risk 3+ always
    /// asks fresh; risk 5 is always denied outright.
    pub fn evaluate(
        &self,
        assessment: &RiskAssessment,
        action: &str,
        mission_id: Option<&str>,
    ) -> Decision {
        match assessment.risk {
            Risk::Critical => Decision::Deny(assessment.reason.clone()),
            Risk::Dangerous | Risk::Risky => Decision::Ask(assessment.reason.clone()),
            Risk::Reviewable | Risk::Normal | Risk::Safe => {
                if let Some(record) = self.matching(action, mission_id) {
                    if record.decision == "allow" {
                        return Decision::Allow(format!(
                            "remembered {} approval",
                            match record.scope {
                                ApprovalScope::Project => "project",
                                ApprovalScope::Mission => "mission",
                                ApprovalScope::Once => "single-use",
                            }
                        ));
                    }
                }
                match assessment.risk {
                    Risk::Reviewable => Decision::Ask(assessment.reason.clone()),
                    _ => Decision::Allow("low-risk project-local action".into()),
                }
            }
        }
    }

    fn matching(&self, action: &str, mission_id: Option<&str>) -> Option<&ApprovalRecord> {
        self.records.iter().rev().find(|r| {
            r.action == action
                && r.decision == "allow"
                && match r.scope {
                    ApprovalScope::Project => true,
                    ApprovalScope::Mission => r.mission_id.as_deref() == mission_id,
                    ApprovalScope::Once => false,
                }
        })
    }

    pub fn remember(
        &mut self,
        action: &str,
        scope: ApprovalScope,
        mission_id: Option<&str>,
    ) -> Result<()> {
        if scope == ApprovalScope::Once {
            // Once-approvals are consumed immediately and never stored.
            return Ok(());
        }
        self.records.push(ApprovalRecord {
            action: action.to_string(),
            scope,
            mission_id: mission_id.map(str::to_string),
            decision: "allow".into(),
            recorded_at: crate::storage::timestamp(),
        });
        if self.records.len() > 500 {
            self.records.remove(0);
        }
        Ok(())
    }

    pub fn forget(action: &str, records: &mut Vec<ApprovalRecord>) {
        records.retain(|r| r.action != action);
    }

    pub fn to_json(&self) -> Result<Vec<u8>> {
        serde_json::to_vec_pretty(self).map_err(|e| ForgeError::Message(e.to_string()))
    }

    pub fn from_json(bytes: &[u8]) -> Result<Self> {
        serde_json::from_slice(bytes).map_err(|e| ForgeError::Message(e.to_string()))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Decision {
    Allow(String),
    Ask(String),
    Deny(String),
}

/// Batch a group of actions into one question when they share a risk class —
/// installing five packages is one approval, not five.
pub fn batch(actions: &[RiskAssessment]) -> Option<RiskAssessment> {
    let first = actions.first()?;
    if actions.iter().all(|a| a.risk == first.risk) {
        Some(first.clone())
    } else {
        None
    }
}

/// Convenience map used by `forge approvals show`.
pub fn summarize(records: &[ApprovalRecord]) -> BTreeMap<String, usize> {
    let mut out = BTreeMap::new();
    for r in records {
        *out.entry(r.action.clone()).or_insert(0) += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn destructive_commands_are_critical() {
        let a = score_command("git push --force origin main");
        assert_eq!(a.risk, Risk::Critical);
        let a = score_command("rm -rf ./build");
        assert_eq!(a.risk, Risk::Critical);
    }

    #[test]
    fn tests_and_status_are_safe() {
        assert_eq!(score_command("cargo test --all-targets").risk, Risk::Safe);
        assert_eq!(score_command("git status --short").risk, Risk::Safe);
    }

    #[test]
    fn package_install_is_reviewable_and_network_is_risky() {
        assert_eq!(score_command("pnpm install zod").risk, Risk::Reviewable);
        assert_eq!(score_command("curl https://example.com").risk, Risk::Risky);
    }

    #[test]
    fn reviewable_actions_ask_once_then_remember() {
        let memory = ApprovalMemory::default();
        let a = score_command("pnpm install zod");
        assert!(matches!(
            memory.evaluate(&a, "package_install", Some("m1")),
            Decision::Ask(_)
        ));
        let mut memory = ApprovalMemory::default();
        memory
            .remember("package_install", ApprovalScope::Mission, Some("m1"))
            .unwrap();
        assert!(matches!(
            memory.evaluate(&a, "package_install", Some("m1")),
            Decision::Allow(_)
        ));
        // A different mission must ask again.
        assert!(matches!(
            memory.evaluate(&a, "package_install", Some("m2")),
            Decision::Ask(_)
        ));
    }

    #[test]
    fn project_scope_covers_all_missions() {
        let mut memory = ApprovalMemory::default();
        memory
            .remember("package_install", ApprovalScope::Project, None)
            .unwrap();
        let a = score_command("pnpm install zod");
        assert!(matches!(
            memory.evaluate(&a, "package_install", Some("anything")),
            Decision::Allow(_)
        ));
    }

    #[test]
    fn dangerous_actions_never_use_memory() {
        let mut memory = ApprovalMemory::default();
        memory
            .remember("secret_access", ApprovalScope::Project, None)
            .unwrap();
        let a = score_command("cat .env");
        assert!(matches!(
            memory.evaluate(&a, "secret_access", Some("m1")),
            Decision::Ask(_)
        ));
    }

    #[test]
    fn critical_is_denied_regardless_of_memory() {
        let mut memory = ApprovalMemory::default();
        memory
            .remember("force_push", ApprovalScope::Project, None)
            .unwrap();
        let a = score_command("git push --force");
        assert!(matches!(
            memory.evaluate(&a, "force_push", Some("m1")),
            Decision::Deny(_)
        ));
    }

    #[test]
    fn batch_collapses_same_risk_actions() {
        let actions = vec![
            score_command("pnpm install zod"),
            score_command("pnpm install dotenv"),
        ];
        assert!(batch(&actions).is_some());
        let mixed = vec![
            score_command("pnpm install zod"),
            score_command("git status"),
        ];
        assert!(batch(&mixed).is_none());
    }
}
