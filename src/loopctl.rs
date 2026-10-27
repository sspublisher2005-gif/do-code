//! Mission loop controller — the `while tool_use` engine.
//!
//! Wraps every mission step in one bounded loop policy: a maximum step
//! count, wall-clock budget, and repeat-failure (stall) detection. The
//! controller is a pure decision engine; the workflow consults it at safe
//! boundaries (between steps, never mid-tool-call) so a pause never
//! destroys in-flight work or evidence.

use crate::error::{ForgeError, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Loop policy values; overridable from `[loop]` in forge.config.toml.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LoopPolicy {
    /// Hard ceiling on model/step iterations per mission run.
    pub max_steps: usize,
    /// Same-failure repetitions before the loop declares a stall.
    pub stall_limit: usize,
    /// Wall-clock ceiling for one mission run, in minutes.
    pub max_wall_minutes: u64,
}

impl Default for LoopPolicy {
    fn default() -> Self {
        Self {
            max_steps: 40,
            stall_limit: 3,
            max_wall_minutes: 30,
        }
    }
}

/// What the workflow must do next at a safe boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoopDecision {
    /// Keep going.
    Continue,
    /// Finish the current work, persist, and stop before starting new work.
    Pause(String),
}

/// Mutable loop state for one mission run.
#[derive(Clone, Debug)]
pub struct LoopState {
    policy: LoopPolicy,
    steps_taken: usize,
    consecutive_failures: usize,
    last_failure: Option<String>,
    started_at: std::time::Instant,
}

impl LoopState {
    pub fn new(policy: LoopPolicy) -> Self {
        Self {
            policy,
            steps_taken: 0,
            consecutive_failures: 0,
            last_failure: None,
            started_at: std::time::Instant::now(),
        }
    }

    pub fn steps_taken(&self) -> usize {
        self.steps_taken
    }

    /// Consult at a safe boundary before starting a new step.
    pub fn begin_step(&self) -> Result<LoopDecision> {
        if self.steps_taken >= self.policy.max_steps {
            return Ok(LoopDecision::Pause(format!(
                "step budget reached ({} steps); mission paused at a safe boundary",
                self.policy.max_steps
            )));
        }
        let elapsed = Duration::from_secs(self.started_at.elapsed().as_secs());
        if elapsed >= Duration::from_secs(self.policy.max_wall_minutes * 60) {
            return Ok(LoopDecision::Pause(format!(
                "wall-clock budget reached ({} minutes); mission paused at a safe boundary",
                self.policy.max_wall_minutes
            )));
        }
        if self.consecutive_failures >= self.policy.stall_limit {
            return Ok(LoopDecision::Pause(format!(
                "stall detected: the same failure repeated {} times ({:?}); mission paused",
                self.consecutive_failures, self.last_failure
            )));
        }
        Ok(LoopDecision::Continue)
    }

    pub fn record_success(&mut self) {
        self.steps_taken = self.steps_taken.saturating_add(1);
        self.consecutive_failures = 0;
        self.last_failure = None;
    }

    /// Record a failure; `fingerprint` should identify the *kind* of failure
    /// (stable across retries of the same underlying problem).
    pub fn record_failure(&mut self, fingerprint: &str) {
        self.steps_taken = self.steps_taken.saturating_add(1);
        if self.last_failure.as_deref() == Some(fingerprint) {
            self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        } else {
            self.consecutive_failures = 1;
            self.last_failure = Some(fingerprint.to_string());
        }
    }

    /// Stable fingerprint for a workflow error, used by stall detection.
    pub fn fingerprint(error: &ForgeError) -> String {
        match error {
            ForgeError::Message(m) => format!("message:{m}"),
            ForgeError::Io { path, .. } => format!("io:{}", path.display()),
            ForgeError::Config(m) => format!("config:{m}"),
            ForgeError::Safety(m) => format!("safety:{m}"),
            ForgeError::Provider(m) => format!("provider:{m}"),
        }
    }
}

/// Persisted loop summary stored with the mission so resume can show why a
/// run paused.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct LoopSummary {
    pub steps_taken: usize,
    pub paused_reason: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> LoopPolicy {
        LoopPolicy {
            max_steps: 3,
            stall_limit: 2,
            max_wall_minutes: 30,
        }
    }

    #[test]
    fn pauses_at_step_budget() {
        let mut state = LoopState::new(policy());
        for _ in 0..3 {
            state.record_success();
        }
        assert_eq!(
            state.begin_step().unwrap(),
            LoopDecision::Pause(
                "step budget reached (3 steps); mission paused at a safe boundary".into()
            )
        );
    }

    #[test]
    fn continues_within_budget() {
        let mut state = LoopState::new(policy());
        state.record_success();
        state.record_success();
        assert_eq!(state.begin_step().unwrap(), LoopDecision::Continue);
    }

    #[test]
    fn detects_stall_after_repeated_identical_failures() {
        // High step budget so the stall limit (not the step count) fires.
        let mut state = LoopState::new(LoopPolicy {
            max_steps: 20,
            stall_limit: 2,
            max_wall_minutes: 30,
        });
        state.record_failure("provider:rate limited");
        state.record_failure("provider:rate limited");
        assert!(matches!(
            state.begin_step().unwrap(),
            LoopDecision::Pause(_)
        ));
    }

    #[test]
    fn resets_failure_streak_on_success() {
        let mut state = LoopState::new(LoopPolicy {
            max_steps: 20,
            stall_limit: 2,
            max_wall_minutes: 30,
        });
        state.record_failure("provider:rate limited");
        state.record_success();
        state.record_failure("provider:rate limited");
        assert_eq!(state.begin_step().unwrap(), LoopDecision::Continue);
    }

    #[test]
    fn distinct_failures_do_not_stall() {
        let mut state = LoopState::new(LoopPolicy {
            max_steps: 20,
            stall_limit: 2,
            max_wall_minutes: 30,
        });
        state.record_failure("provider:a");
        state.record_failure("provider:b");
        state.record_failure("provider:c");
        assert_eq!(state.begin_step().unwrap(), LoopDecision::Continue);
    }

    #[test]
    fn fingerprints_are_stable_per_error_kind() {
        let a = LoopState::fingerprint(&ForgeError::Safety("x".into()));
        let b = LoopState::fingerprint(&ForgeError::Safety("x".into()));
        assert_eq!(a, b);
        let c = LoopState::fingerprint(&ForgeError::Provider("x".into()));
        assert_ne!(a, c);
    }
}
