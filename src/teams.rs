//! Feature-lead teams — complexity-gated agent staffing.
//!
//! A Simple task is one agent. A Standard task adds a lead (architect) with
//! two specialists (tester, reviewer). A Complex task staffs the full team
//! including security. Teams are sequential specialist perspectives sharing
//! the mission ledger — never independent models claiming to be a meeting.
//! Best-of-N is opt-in only: for Complex tasks the user may request N coder
//! proposals and pick one; it doubles model cost, so it is never default.

use crate::dispatch::TaskComplexity;
use crate::mission::AgentRole;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TeamPlan {
    pub lead: AgentRole,
    pub specialists: Vec<AgentRole>,
    pub strategy: TeamStrategy,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TeamStrategy {
    /// One coder proposal; the default.
    Single,
    /// Opt-in: N proposals for the same step, reviewer picks one.
    BestOfN { n: u8 },
}

/// Assign the team for a task. No flags, no config: the tool decides from
/// the adaptive complexity classification.
pub fn assign_team(complexity: TaskComplexity) -> TeamPlan {
    match complexity {
        TaskComplexity::Simple => TeamPlan {
            lead: AgentRole::Architect,
            specialists: vec![AgentRole::Coder],
            strategy: TeamStrategy::Single,
        },
        TaskComplexity::Standard => TeamPlan {
            lead: AgentRole::Architect,
            specialists: vec![AgentRole::Tester, AgentRole::Reviewer],
            strategy: TeamStrategy::Single,
        },
        TaskComplexity::Complex => TeamPlan {
            lead: AgentRole::Architect,
            specialists: vec![AgentRole::Tester, AgentRole::Reviewer, AgentRole::Security],
            strategy: TeamStrategy::Single,
        },
    }
}

/// Opt in to best-of-N for hard tasks. N is clamped to 2..=3 — beyond that
/// the token cost outweighs the quality gain.
pub fn with_best_of_n(mut plan: TeamPlan, n: u8) -> TeamPlan {
    let n = n.clamp(2, 3);
    plan.strategy = TeamStrategy::BestOfN { n };
    plan
}

/// Roles that will actually issue model calls, in execution order.
pub fn execution_order(plan: &TeamPlan) -> Vec<AgentRole> {
    let mut order = vec![plan.lead];
    order.extend(plan.specialists.iter().copied());
    order
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_tasks_staff_one_specialist() {
        let plan = assign_team(TaskComplexity::Simple);
        assert_eq!(plan.specialists, vec![AgentRole::Coder]);
        assert_eq!(plan.strategy, TeamStrategy::Single);
    }

    #[test]
    fn standard_tasks_add_tester_and_reviewer() {
        let plan = assign_team(TaskComplexity::Standard);
        assert_eq!(plan.specialists.len(), 2);
    }

    #[test]
    fn complex_tasks_add_security() {
        let plan = assign_team(TaskComplexity::Complex);
        assert!(plan.specialists.contains(&AgentRole::Security));
    }

    #[test]
    fn best_of_n_is_clamped() {
        let plan = with_best_of_n(assign_team(TaskComplexity::Complex), 9);
        assert_eq!(plan.strategy, TeamStrategy::BestOfN { n: 3 });
        let plan = with_best_of_n(assign_team(TaskComplexity::Complex), 1);
        assert_eq!(plan.strategy, TeamStrategy::BestOfN { n: 2 });
    }

    #[test]
    fn execution_order_starts_with_lead() {
        let plan = assign_team(TaskComplexity::Complex);
        let order = execution_order(&plan);
        assert_eq!(order.first(), Some(&AgentRole::Architect));
        assert_eq!(order.len(), 4);
    }
}
