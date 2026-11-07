//! Multi-model routing.
//!
//! Routes each agent role to a configured model without losing the strict
//! validation the base config enforces. `[routing]` maps role names to
//! `provider/model` strings plus an optional ordered fallback chain; when a
//! route fails with a provider error, the next route is tried once and the
//! switchover is recorded in the audit log. Privacy rules (loopback,
//! consent, local-only) are re-checked for every route.

use crate::config::{DocodeConfig, ModelConfig};
use crate::error::{DocodeError, Result};
use crate::mission::AgentRole;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct RoutingConfig {
    /// role -> "provider/model" (e.g. "anthropic/claude-sonnet").
    pub roles: BTreeMap<String, String>,
    /// Ordered fallback targets used when the primary route fails.
    pub fallback: Vec<String>,
}

/// Parse "provider/model" into its two parts.
pub fn parse_target(target: &str) -> Result<(String, String)> {
    let (provider, model) = target.split_once('/').ok_or_else(|| {
        DocodeError::Config(format!(
            "routing target '{target}' must be 'provider/model'."
        ))
    })?;
    let provider = provider.trim().to_ascii_lowercase();
    let model = model.trim().to_string();
    if provider.is_empty() || model.is_empty() {
        return Err(DocodeError::Config(format!(
            "routing target '{target}' has an empty provider or model."
        )));
    }
    match provider.as_str() {
        "openai-compatible" | "ollama" | "anthropic" | "google" => {}
        other => {
            return Err(DocodeError::Config(format!(
                "routing target '{target}' uses unsupported provider '{other}'."
            )));
        }
    }
    Ok((provider, model))
}

pub fn role_key(role: AgentRole) -> &'static str {
    match role {
        AgentRole::Orchestrator => "orchestrator",
        AgentRole::Researcher => "researcher",
        AgentRole::Architect => "architect",
        AgentRole::Coder => "coder",
        AgentRole::Tester => "tester",
        AgentRole::Reviewer => "reviewer",
        AgentRole::Security => "security",
        AgentRole::Devops => "devops",
    }
}

/// The ordered list of routes for a role: the role override first, then the
/// base config, then the fallback chain.
pub fn routes_for(
    cfg: &DocodeConfig,
    routing: &RoutingConfig,
    role: AgentRole,
) -> Vec<ModelConfig> {
    let mut routes = Vec::new();
    let key = role_key(role);
    if let Some((provider, model)) = routing.roles.get(key).and_then(|t| parse_target(t).ok()) {
        routes.push(override_model(&cfg.model, &provider, &model));
    }
    routes.push(cfg.model.clone());
    for target in &routing.fallback {
        if let Ok((provider, model)) = parse_target(target) {
            routes.push(override_model(&cfg.model, &provider, &model));
        }
    }
    routes
}

fn override_model(base: &ModelConfig, provider: &str, model: &str) -> ModelConfig {
    let mut next = base.clone();
    next.provider = provider.to_string();
    next.model = model.to_string();
    // Provider-specific endpoint defaults follow the provider switch; a
    // routing override may also set base_url explicitly via the base config
    // only, keeping secrets and endpoints out of per-role strings.
    match provider {
        "anthropic" if base.provider != "anthropic" => {
            next.base_url = "https://api.anthropic.com/v1".into();
            next.api_key_env = "ANTHROPIC_API_KEY".into();
        }
        "google" if base.provider != "google" => {
            next.base_url = "https://generativelanguage.googleapis.com/v1beta".into();
            next.api_key_env = "GOOGLE_API_KEY".into();
        }
        "ollama" if base.provider != "ollama" => {
            next.base_url = "http://127.0.0.1:11434/v1".into();
            next.api_key_env = "OLLAMA_API_KEY".into();
        }
        "openai-compatible" if base.provider != "openai-compatible" => {
            next.base_url = "https://api.openai.com/v1".into();
            next.api_key_env = "OPENAI_API_KEY".into();
        }
        _ => {}
    }
    next
}

/// Build the routed config for one attempt. Validation of the resulting
/// model block happens in the provider layer, fail-closed as usual.
pub fn config_for_route(base: &DocodeConfig, route: &ModelConfig) -> DocodeConfig {
    let mut cfg = base.clone();
    cfg.model = route.clone();
    cfg
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> DocodeConfig {
        DocodeConfig::default()
    }

    #[test]
    fn parses_provider_model_targets() {
        assert_eq!(
            parse_target("anthropic/claude-sonnet").unwrap(),
            ("anthropic".into(), "claude-sonnet".into())
        );
        assert!(parse_target("no-slash").is_err());
        assert!(parse_target("evil-provider/model").is_err());
        assert!(parse_target("anthropic/").is_err());
    }

    #[test]
    fn base_route_is_always_last_resort() {
        let c = cfg();
        let routes = routes_for(&c, &RoutingConfig::default(), AgentRole::Coder);
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].provider, "openai-compatible");
    }

    #[test]
    fn role_override_comes_first_then_base_then_fallback() {
        let c = cfg();
        let routing = RoutingConfig {
            roles: BTreeMap::from([("coder".into(), "anthropic/claude-sonnet".into())]),
            fallback: vec!["ollama/qwen3-coder".into()],
        };
        let routes = routes_for(&c, &routing, AgentRole::Coder);
        assert_eq!(routes.len(), 3);
        assert_eq!(routes[0].provider, "anthropic");
        assert_eq!(routes[0].model, "claude-sonnet");
        assert_eq!(routes[0].api_key_env, "ANTHROPIC_API_KEY");
        assert_eq!(routes[1].provider, "openai-compatible");
        assert_eq!(routes[2].provider, "ollama");
        assert_eq!(routes[2].base_url, "http://127.0.0.1:11434/v1");
    }

    #[test]
    fn other_roles_are_unaffected_by_coder_override() {
        let c = cfg();
        let routing = RoutingConfig {
            roles: BTreeMap::from([("coder".into(), "anthropic/claude-sonnet".into())]),
            fallback: vec![],
        };
        let routes = routes_for(&c, &routing, AgentRole::Researcher);
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].provider, "openai-compatible");
    }

    #[test]
    fn invalid_fallback_entries_are_skipped_not_fatal() {
        let c = cfg();
        let routing = RoutingConfig {
            roles: Default::default(),
            fallback: vec!["not-a-target".into(), "ollama/qwen3-coder".into()],
        };
        let routes = routes_for(&c, &routing, AgentRole::Tester);
        assert_eq!(routes.len(), 2);
        assert_eq!(routes[1].provider, "ollama");
    }
}
