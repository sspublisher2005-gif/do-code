//! P1 typed tool registry — the model proposes, typed code enforces.
//!
//! Every model-proposed tool call is validated against strict, hand-checked
//! schemas before anything executes. `process.run` and any unknown tool are
//! policy-denied here and always; read tools share the same bounded,
//! redacting floor as the rest of the CLI. Nothing in this module runs a
//! shell or a build. Tool output is untrusted data, bounded and redacted.

use crate::{
    error::{DocodeError, Result},
    files, redaction,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::path::Path;

/// Hard cap on what a tool may return into the model prompt, in bytes.
pub const MAX_TOOL_OUTPUT_BYTES: usize = 32 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolResult {
    pub tool: String,
    pub ok: bool,
    pub output: String,
    pub error_code: Option<String>,
    pub bytes: usize,
    pub duration_ms: u64,
}

impl ToolResult {
    fn ok(tool: &str, output: String, started: std::time::Instant) -> Self {
        Self {
            tool: tool.to_owned(),
            ok: true,
            output,
            error_code: None,
            bytes: 0,
            duration_ms: started.elapsed().as_millis() as u64,
        }
    }
    fn err(tool: &str, code: &str, message: String, started: std::time::Instant) -> Self {
        Self {
            tool: tool.to_owned(),
            ok: false,
            output: message,
            error_code: Some(code.to_owned()),
            bytes: 0,
            duration_ms: started.elapsed().as_millis() as u64,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ToolPolicy {
    /// Safe local read; may execute after schema validation.
    Execute,
    /// Never execute in any configuration of this build.
    Deny,
}

/// Policy table. The OS sandbox is not qualified, so no tool that
/// spawns a process is allowed — not even to "check it denies".
pub fn tool_policy(name: &str) -> ToolPolicy {
    match name {
        "workspace.list" | "workspace.read" | "workspace.search" => ToolPolicy::Execute,
        _ => ToolPolicy::Deny,
    }
}

/// Validate a proposed tool call against the tool's strict schema.
/// Unknown tools and wrong argument types are rejected BEFORE any side effect.
pub fn validate_tool_call(name: &str, args: &Value) -> Result<Value> {
    let obj = args.as_object().ok_or_else(|| {
        DocodeError::Message(format!("tool '{name}' arguments must be a JSON object"))
    })?;
    let value = match name {
        "workspace.list" => {
            let path = opt_rel_path(obj, "path")?.unwrap_or_else(|| ".".to_owned());
            let recursive = opt_bool(obj, "recursive")?.unwrap_or(true);
            let max_files = opt_bounded_u64(obj, "max_files", 200, 10_000)?;
            serde_json::json!({"path": path, "recursive": recursive, "max_files": max_files})
        }
        "workspace.read" => {
            let path = req_rel_path(obj, "path")?;
            let max_bytes = opt_bounded_u64(obj, "max_bytes", 65_536, 1_048_576)?;
            serde_json::json!({"path": path, "max_bytes": max_bytes})
        }
        "workspace.search" => {
            let query = req_string(obj, "query", 500)?;
            if query.trim().is_empty() {
                return Err(DocodeError::Message(
                    "tool 'workspace.search' query must not be empty.".into(),
                ));
            }
            let path = opt_rel_path(obj, "path")?.unwrap_or_else(|| ".".to_owned());
            let max_matches = opt_bounded_u64(obj, "max_matches", 50, 1_000)?;
            serde_json::json!({"query": query, "path": path, "max_matches": max_matches})
        }
        "process.run" => {
            let argv_value = obj.get("argv").ok_or_else(|| {
                DocodeError::Message(
                    "tool 'process.run' requires 'argv' (array of strings).".into(),
                )
            })?;
            let argv = argv_value.as_array().ok_or_else(|| {
                DocodeError::Message("tool 'process.run' argv must be an array of strings.".into())
            })?;
            if argv.is_empty() || argv.len() > 128 {
                return Err(DocodeError::Message(
                    "tool 'process.run' argv must be 1..=128 items.".into(),
                ));
            }
            let argv: Vec<String> = argv
                .iter()
                .map(|entry| {
                    entry.as_str().map(str::to_owned).ok_or_else(|| {
                        DocodeError::Message(
                            "tool 'process.run' argv items must be strings.".into(),
                        )
                    })
                })
                .collect::<Result<_>>()?;
            for item in &argv {
                if item.is_empty() || item.len() > 4_096 {
                    return Err(DocodeError::Message(
                        "tool 'process.run' argv items must be 1..=4096 bytes.".into(),
                    ));
                }
            }
            let cwd = opt_rel_path(obj, "cwd")?.unwrap_or_else(|| ".".to_owned());
            serde_json::json!({"argv": argv, "cwd": cwd})
        }
        other => {
            return Err(DocodeError::Message(format!(
                "unknown tool '{other}'; available tools: workspace.list, workspace.read, workspace.search."
            )));
        }
    };
    Ok(value)
}

/// Execute a validated, policy-granted tool call. Denied tools never reach here.
pub fn execute(root: &Path, name: &str, args: &Value, _max_read_bytes: u64) -> ToolResult {
    let started = std::time::Instant::now();
    if tool_policy(name) != ToolPolicy::Execute {
        return ToolResult::err(
            name,
            "policy_denied",
            format!(
                "tool '{name}' is denied by policy in this build (no execution driver qualified)."
            ),
            started,
        );
    }
    let result: Result<String> = match name {
        "workspace.list" => {
            let path = args["path"].as_str().unwrap_or(".");
            let recursive = args["recursive"].as_bool().unwrap_or(true);
            let max_files = args["max_files"].as_u64().unwrap_or(200) as usize;
            files::list(root, path, recursive, max_files).map(|entries| {
                entries
                    .iter()
                    .map(|entry| format!("{} {} {}B", entry.path, entry.kind, entry.size_bytes))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
        }
        "workspace.read" => files::read(
            root,
            args["path"].as_str().unwrap_or(""),
            args["max_bytes"].as_u64().unwrap_or(65_536),
        ),
        "workspace.search" => files::search(
            root,
            args["query"].as_str().unwrap_or(""),
            args["path"].as_str().unwrap_or("."),
            1_048_576,
            5_000,
            args["max_matches"].as_u64().unwrap_or(50) as usize,
        )
        .map(|matches| {
            matches
                .iter()
                .map(|m| format!("{}:{}: {}", m.path, m.line, m.text.trim()))
                .collect::<Vec<_>>()
                .join("\n")
        }),
        _ => Err(DocodeError::Message(format!(
            "tool '{name}' is not executable."
        ))),
    };
    match result {
        Ok(mut output) => {
            output = redaction::redact(&output);
            if output.len() > MAX_TOOL_OUTPUT_BYTES {
                output.truncate(MAX_TOOL_OUTPUT_BYTES);
                output.push_str("\n[tool output truncated at 32 KiB]");
            }
            let mut tool = ToolResult::ok(name, output, started);
            tool.bytes = tool.output.len();
            tool
        }
        Err(error) => {
            let message = format!("{error}");
            let code = match error {
                DocodeError::Safety(_) => "safety_denied",
                DocodeError::Io { .. } | DocodeError::Message(_) => "tool_error",
                _ => "tool_error",
            };
            ToolResult::err(name, code, message, started)
        }
    }
}

/// Workspace-relative path only: no absolute paths, no drive letters, no `..`.
fn require_rel_path(path: &str) -> Result<()> {
    if path.is_empty() || path.len() > 4_096 {
        return Err(DocodeError::Safety(
            "tool path must be 1..=4096 bytes.".into(),
        ));
    }
    let parsed = std::path::Path::new(path);
    if parsed.is_absolute() || path.contains(':') {
        return Err(DocodeError::Safety(
            "tool paths must be workspace-relative; absolute paths and drive letters are refused."
                .into(),
        ));
    }
    for segment in parsed.components() {
        if matches!(segment, std::path::Component::ParentDir) {
            return Err(DocodeError::Safety(
                "tool paths may not contain '..' segments.".into(),
            ));
        }
    }
    Ok(())
}

fn req_string(obj: &Map<String, Value>, key: &str, max_len: usize) -> Result<String> {
    let value = obj.get(key).ok_or_else(|| {
        DocodeError::Message(format!("tool argument '{key}' (string) is required."))
    })?;
    let text = value
        .as_str()
        .ok_or_else(|| DocodeError::Message(format!("tool argument '{key}' must be a string.")))?;
    if text.is_empty() || text.len() > max_len {
        return Err(DocodeError::Message(format!(
            "tool argument '{key}' must be 1..={max_len} bytes."
        )));
    }
    Ok(text.to_owned())
}

fn opt_rel_path(obj: &Map<String, Value>, key: &str) -> Result<Option<String>> {
    match obj.get(key) {
        None => Ok(None),
        Some(value) => {
            let text = value.as_str().ok_or_else(|| {
                DocodeError::Message(format!("tool argument '{key}' must be a string."))
            })?;
            require_rel_path(text)?;
            Ok(Some(text.to_owned()))
        }
    }
}

fn req_rel_path(obj: &Map<String, Value>, key: &str) -> Result<String> {
    opt_rel_path(obj, key)?
        .ok_or_else(|| DocodeError::Message(format!("tool argument '{key}' is required.")))
}

fn opt_bool(obj: &Map<String, Value>, key: &str) -> Result<Option<bool>> {
    match obj.get(key) {
        None => Ok(None),
        Some(value) => value.as_bool().map(Some).ok_or_else(|| {
            DocodeError::Message(format!("tool argument '{key}' must be a boolean."))
        }),
    }
}

fn opt_bounded_u64(obj: &Map<String, Value>, key: &str, default: u64, max: u64) -> Result<u64> {
    let value = match obj.get(key) {
        None => return Ok(default),
        Some(value) => value.as_u64().ok_or_else(|| {
            DocodeError::Message(format!(
                "tool argument '{key}' must be a non-negative integer."
            ))
        })?,
    };
    if value == 0 || value > max {
        return Err(DocodeError::Safety(format!(
            "tool argument '{key}' must be 1..={max}."
        )));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_tool_is_rejected() {
        let args = serde_json::json!({});
        assert!(validate_tool_call("evil.tool", &args).is_err());
    }

    #[test]
    fn wrong_types_are_rejected() {
        let args = serde_json::json!({"path": 42});
        assert!(validate_tool_call("workspace.read", &args).is_err());
        let args = serde_json::json!({"recursive": "yes"});
        assert!(validate_tool_call("workspace.list", &args).is_err());
    }

    #[test]
    fn traversal_paths_are_rejected() {
        let args = serde_json::json!({"path": "../outside.txt"});
        assert!(validate_tool_call("workspace.read", &args).is_err());
        let args = serde_json::json!({"path": "C:\\Windows\\System32"});
        assert!(validate_tool_call("workspace.read", &args).is_err());
    }

    #[test]
    fn process_run_validates_but_is_policy_denied() {
        let args = serde_json::json!({"argv": ["cmd.exe", "/c", "echo hi"]});
        let normalized = validate_tool_call("process.run", &args).expect("schema parses");
        assert_eq!(tool_policy("process.run"), ToolPolicy::Deny);
        let result = execute(Path::new("."), "process.run", &normalized, 1024);
        assert!(!result.ok);
        assert_eq!(result.error_code.as_deref(), Some("policy_denied"));
    }

    #[test]
    fn read_tools_are_policy_allowed() {
        assert_eq!(tool_policy("workspace.read"), ToolPolicy::Execute);
        assert_eq!(tool_policy("workspace.list"), ToolPolicy::Execute);
        assert_eq!(tool_policy("workspace.search"), ToolPolicy::Execute);
    }

    #[test]
    fn out_of_range_numbers_are_rejected() {
        let args = serde_json::json!({"path": ".", "max_files": 0});
        assert!(validate_tool_call("workspace.list", &args).is_err());
        let args = serde_json::json!({"query": "needle", "max_matches": 999_999});
        assert!(validate_tool_call("workspace.search", &args).is_err());
    }
}
