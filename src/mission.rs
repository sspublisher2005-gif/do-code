use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub type Id = String;
pub type IsoDateTime = String;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MissionStatus {
    Draft,
    AwaitingApproval,
    Ready,
    Running,
    Verifying,
    NeedsReview,
    Completed,
    Failed,
    Cancelled,
    Interrupted,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    Pending,
    Ready,
    Running,
    AwaitingApproval,
    Passed,
    Failed,
    Blocked,
    Skipped,
    Interrupted,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentRole {
    Orchestrator,
    Researcher,
    Architect,
    Coder,
    Tester,
    Reviewer,
    Security,
    Devops,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    Model,
    ReadWorkspace,
    WriteWorkspace,
    SandboxCommand,
    Browser,
    NetworkResearch,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VerificationKind {
    Test,
    Typecheck,
    Lint,
    Browser,
    Review,
    Security,
    Manual,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AcceptanceCriterion {
    pub id: Id,
    pub description: String,
    pub required: bool,
    pub verification_kinds: Vec<VerificationKind>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Mission {
    pub schema_version: u8,
    pub id: Id,
    pub project_id: Id,
    pub repo_root: String,
    pub objective: String,
    pub status: MissionStatus,
    pub created_at: IsoDateTime,
    pub updated_at: IsoDateTime,
    pub steps: Vec<Step>,
    pub acceptance_criteria: Vec<AcceptanceCriterion>,
    pub max_duration_ms: u64,
    pub dry_run: bool,
    pub approved_capabilities: Vec<Capability>,
    pub evidence_ids: Vec<Id>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Step {
    pub id: Id,
    pub mission_id: Id,
    pub ordinal: u32,
    pub kind: String,
    pub assigned_role: AgentRole,
    pub status: StepStatus,
    pub depends_on: Vec<Id>,
    pub attempt: u32,
    pub input_evidence_ids: Vec<Id>,
    pub output_evidence_ids: Vec<Id>,
    pub tool_call_ids: Vec<Id>,
    pub created_at: IsoDateTime,
    pub started_at: Option<IsoDateTime>,
    pub finished_at: Option<IsoDateTime>,
    pub failure_code: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AgentMessage {
    pub id: Id,
    pub mission_id: Id,
    pub step_id: Option<Id>,
    pub role: String,
    pub sender_id: String,
    pub recipient_id: Option<String>,
    pub content: String,
    pub tool_call_ids: Vec<Id>,
    pub evidence_ids: Vec<Id>,
    pub created_at: IsoDateTime,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub usage: Option<UsageReport>,
    pub redacted: bool,
}
// KNOWN GAP: ToolCall/SafetyDecision/EvidenceRef/VerificationResult are the data model for
// tool-call, safety-decision and verification-result records, but no code path constructs them
// yet -- actual tool execution is blocked pending the sandbox (see docs/STATUS.md), so there is
// nothing to record. Tracked in FEATURE_PARITY_LEDGER.md; not a false "complete" claim.
#[allow(dead_code)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: Id,
    pub mission_id: Id,
    pub step_id: Id,
    pub tool_name: String,
    pub input: serde_json::Value,
    pub risk: String,
    pub requested_capabilities: Vec<String>,
    pub status: String,
    pub safety_decision_id: Option<Id>,
    pub created_at: IsoDateTime,
    pub started_at: Option<IsoDateTime>,
    pub finished_at: Option<IsoDateTime>,
    pub output_evidence_ids: Vec<Id>,
    pub error_code: Option<String>,
}
#[allow(dead_code)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SafetyDecision {
    pub id: Id,
    pub mission_id: Id,
    pub tool_call_id: Option<Id>,
    pub outcome: String,
    pub policy_version: String,
    pub rule_ids: Vec<String>,
    pub resource: String,
    pub rationale: String,
    pub decided_at: IsoDateTime,
    pub expires_at: Option<IsoDateTime>,
    pub operator_id: Option<String>,
}
#[allow(dead_code)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EvidenceRef {
    pub id: Id,
    pub mission_id: Id,
    pub step_id: Option<Id>,
    pub kind: String,
    pub uri: String,
    pub sha256: String,
    pub created_at: IsoDateTime,
    pub produced_by: String,
    pub verified: bool,
    pub media_type: Option<String>,
    pub byte_length: Option<u64>,
    pub metadata: BTreeMap<String, serde_json::Value>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VerificationCheck {
    pub id: Id,
    pub name: String,
    pub status: String,
    pub required: bool,
    pub exit_code: Option<i32>,
    pub duration_ms: Option<u64>,
    pub evidence_ids: Vec<Id>,
    pub details: Option<String>,
}
#[allow(dead_code)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VerificationResult {
    pub id: Id,
    pub mission_id: Id,
    pub status: String,
    pub started_at: IsoDateTime,
    pub finished_at: IsoDateTime,
    pub checks: Vec<VerificationCheck>,
    pub unresolved_warnings: Vec<String>,
    pub completion_allowed: bool,
    pub policy_version: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UsageReport {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    /// Provider-reported USD cost only. Never estimate it or treat missing cost as zero.
    pub provider_reported_cost_usd: Option<f64>,
}

/// Cost fields are intentionally absent from Mission and workflow budgets. Only duration
/// and execution-safety limits can gate a run in this schema.
pub fn mission_transitions(from: &MissionStatus, to: &MissionStatus) -> bool {
    use MissionStatus::*;
    matches!(
        (from, to),
        (Draft, AwaitingApproval | Ready | Cancelled)
            | (AwaitingApproval, Ready | Cancelled | NeedsReview)
            | (Ready, Running | Cancelled | NeedsReview)
            | (
                Running,
                Verifying | Failed | Cancelled | Interrupted | NeedsReview
            )
            | (Verifying, Completed | Failed | NeedsReview | Interrupted)
            | (NeedsReview, Ready | Cancelled | Failed)
            | (Failed, Ready | Cancelled)
            | (Interrupted, NeedsReview | Ready | Cancelled)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mission_transitions_are_explicit() {
        assert!(mission_transitions(
            &MissionStatus::Draft,
            &MissionStatus::Ready
        ));
        assert!(!mission_transitions(
            &MissionStatus::Completed,
            &MissionStatus::Running
        ));
    }
}
