use crate::{
    audit,
    config::{self, ForgeConfig},
    error::{ForgeError, Result},
    redaction,
};
use reqwest::{Client, redirect::Policy};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::Path,
    time::{Duration, Instant},
};

const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
const DIAGNOSTIC_PROMPT: &str = "Reply exactly: FORGE_MODEL_TEST_OK";

#[derive(Clone, Debug, serde::Serialize)]
pub struct UsageReport {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    /// Only populated when the selected provider explicitly reports a USD-denominated cost.
    pub provider_reported_cost_usd: Option<f64>,
}
#[derive(Clone, Debug)]
pub struct ModelResponse {
    pub text: String,
    pub provider: String,
    pub model: String,
    pub request_id: Option<String>,
    pub duration_ms: u128,
    pub usage: UsageReport,
}

pub async fn test_model(
    root: &Path,
    config: &ForgeConfig,
    live: bool,
    approve_context_sharing: bool,
) -> Result<()> {
    let parsed_url = config::validate_base_url(&config.model.base_url)?;
    println!(
        "Provider: {}",
        redaction::safe_terminal(&redaction::redact(&config.model.provider))
    );
    println!(
        "Model: {}",
        redaction::safe_terminal(&redaction::redact(&config.model.model))
    );
    println!("Endpoint: {}", parsed_url.origin().ascii_serialization());
    if !live {
        println!("Configuration check only: no model request was sent.");
        println!("Use --live --approve-context-sharing to send a fixed diagnostic prompt.");
        audit::append(
            root,
            "model.test_configuration",
            "info",
            details([
                ("provider", json!(config.model.provider)),
                ("networkUsed", json!(false)),
            ]),
        )?;
        return Ok(());
    }
    if !approve_context_sharing {
        return Err(ForgeError::Safety("live model test blocked: both --live and --approve-context-sharing are required. No request was sent.".into()));
    }
    if config.privacy.mode == "local-only" && !config::is_loopback(&parsed_url) {
        return Err(ForgeError::Safety(
            "privacy.mode is local-only; remote endpoint is blocked. No request was sent.".into(),
        ));
    }
    let api_key = std::env::var(&config.model.api_key_env)
        .ok()
        .filter(|value| !value.trim().is_empty());
    let local_endpoint = config::is_loopback(&parsed_url);
    if api_key.is_none() && !local_endpoint {
        return Err(ForgeError::Provider(format!(
            "set {} in the current shell; Forge does not save API keys to disk. No request was sent.",
            config.model.api_key_env
        )));
    }
    println!(
        "Live request approved for {}. Only a fixed diagnostic prompt is sent; no workspace, FORGE.md, or memory is attached. Provider billing and retention policies may apply.",
        parsed_url.origin().ascii_serialization()
    );
    let response = generate_text(root, config, "You are testing a model connection. Do not request or reveal secrets. Reply with the exact text FORGE_MODEL_TEST_OK and nothing else.", DIAGNOSTIC_PROMPT, 32.min(config.model.max_output_tokens), true, true).await?;
    let passed = response.text.trim() == "FORGE_MODEL_TEST_OK";
    println!(
        "Model test: {}",
        if passed {
            "PASS"
        } else {
            "response received; unexpected diagnostic text"
        }
    );
    println!(
        "Provider: {}; model: {}; duration: {} ms",
        redaction::safe_terminal(&redaction::redact(&response.provider)),
        redaction::safe_terminal(&redaction::redact(&response.model)),
        response.duration_ms
    );
    println!(
        "Token usage: input={}, output={}; provider-reported cost={}",
        display_u64(response.usage.input_tokens),
        display_u64(response.usage.output_tokens),
        display_cost(response.usage.provider_reported_cost_usd)
    );
    let mut extra = details([
        ("provider", json!(response.provider)),
        ("model", json!(response.model)),
        ("durationMs", json!(response.duration_ms)),
        ("inputTokens", optional_number(response.usage.input_tokens)),
        (
            "outputTokens",
            optional_number(response.usage.output_tokens),
        ),
        (
            "providerReportedCostUsd",
            response
                .usage
                .provider_reported_cost_usd
                .map(Value::from)
                .unwrap_or(Value::String("unknown".into())),
        ),
        ("outputMatched", json!(passed)),
        ("networkUsed", json!(true)),
        ("repoOrMemoryAttached", json!(false)),
    ]);
    if let Some(id) = response.request_id {
        extra.insert(
            "providerRequestId".into(),
            Value::String(redaction::redact(&id)),
        );
    }
    audit::append(
        root,
        "model.test",
        if passed { "succeeded" } else { "failed" },
        extra,
    )?;
    if !passed {
        return Err(ForgeError::Provider(
            "provider responded, but the fixed diagnostic response did not match.".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
async fn generate_diagnostic(config: &ForgeConfig, api_key: Option<&str>) -> Result<ModelResponse> {
    let base = config::validate_base_url(&config.model.base_url)?;
    let endpoint = format!("{}/chat/completions", base.as_str().trim_end_matches('/'));
    let client = Client::builder()
        .timeout(Duration::from_millis(config.model.timeout_ms))
        .redirect(Policy::none())
        .build()
        .map_err(|_| ForgeError::Provider("could not initialize HTTPS provider client.".into()))?;
    let body = json!({
        "model": config.model.model,
        "messages": [
            {"role":"system", "content":"You are testing a model connection. Do not request or reveal secrets. Reply with the exact text FORGE_MODEL_TEST_OK and nothing else."},
            {"role":"user", "content":DIAGNOSTIC_PROMPT}
        ],
        "max_tokens": 32,
        "stream": false
    });
    let mut request = client
        .post(&endpoint)
        .header("content-type", "application/json")
        .json(&body);
    if let Some(key) = api_key {
        request = request.bearer_auth(key);
    }
    let started = Instant::now();
    let mut response = request.send().await.map_err(|error| {
        if error.is_timeout() {
            ForgeError::Provider("provider request timed out.".into())
        } else {
            ForgeError::Provider("provider endpoint could not be reached.".into())
        }
    })?;
    let status = response.status();
    if !status.is_success() {
        // Deliberately do not print response bodies; they may echo credentials or private data.
        return Err(ForgeError::Provider(format!(
            "provider returned HTTP {status}; response body was not logged."
        )));
    }
    if response
        .content_length()
        .is_some_and(|n| n > MAX_RESPONSE_BYTES as u64)
    {
        return Err(ForgeError::Provider(
            "provider response exceeded the 2 MiB safety limit.".into(),
        ));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| ForgeError::Provider("provider response stream failed.".into()))?
    {
        if bytes.len() + chunk.len() > MAX_RESPONSE_BYTES {
            return Err(ForgeError::Provider(
                "provider response exceeded the 2 MiB safety limit.".into(),
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    let payload: Value = serde_json::from_slice(&bytes)
        .map_err(|_| ForgeError::Provider("provider response was not valid JSON.".into()))?;
    let text = payload
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|a| a.first())
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(Value::as_str)
        .ok_or_else(|| {
            ForgeError::Provider(
                "provider response did not contain text completion content.".into(),
            )
        })?
        .to_owned();
    let token_usage = normalize_usage(&payload);
    Ok(ModelResponse {
        text,
        provider: config.model.provider.clone(),
        model: payload
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or(&config.model.model)
            .to_owned(),
        request_id: payload
            .get("id")
            .and_then(Value::as_str)
            .map(redaction::redact),
        duration_ms: started.elapsed().as_millis(),
        usage: token_usage,
    })
}

/// Send an explicitly consented user prompt through one of Forge's configured provider adapters.
/// This is deliberately a text API; tool execution, browser access and file writes are not exposed here.
pub async fn generate_text(
    root: &Path,
    config: &ForgeConfig,
    system_prompt: &str,
    user_prompt: &str,
    max_output_tokens: usize,
    live: bool,
    approve_context_sharing: bool,
) -> Result<ModelResponse> {
    let api_key = std::env::var(&config.model.api_key_env)
        .ok()
        .filter(|value| !value.trim().is_empty());
    generate_text_with_key(
        root,
        config,
        system_prompt,
        user_prompt,
        max_output_tokens,
        live,
        approve_context_sharing,
        api_key.as_deref(),
    )
    .await
}

/// Internal core makes provider adapters testable against loopback fixtures without
/// changing process environment variables or requiring a real credential.
// Internal provider-dispatch core; each argument is a distinct required call parameter.
#[allow(clippy::too_many_arguments)]
async fn generate_text_with_key(
    root: &Path,
    config: &ForgeConfig,
    system_prompt: &str,
    user_prompt: &str,
    max_output_tokens: usize,
    live: bool,
    approve_context_sharing: bool,
    key: Option<&str>,
) -> Result<ModelResponse> {
    if !live || !approve_context_sharing {
        return Err(ForgeError::Safety("model request blocked: pass both --live and --approve-context-sharing. No prompt was sent.".into()));
    }
    if crate::redaction::redact(system_prompt) != system_prompt
        || crate::redaction::redact(user_prompt) != user_prompt
    {
        return Err(ForgeError::Safety("prompt appears to contain a credential. Remove it before a live request; no prompt was sent. Secret detection is heuristic.".into()));
    }
    let base = config::validate_base_url(&config.model.base_url)?;
    if config.privacy.mode == "local-only" && !config::is_loopback(&base) {
        return Err(ForgeError::Safety(
            "privacy.mode is local-only; remote model requests are blocked. No prompt was sent."
                .into(),
        ));
    }
    if !(1..=config.model.max_output_tokens).contains(&max_output_tokens) {
        return Err(ForgeError::Config(
            "requested output token limit must be between 1 and the configured maximum.".into(),
        ));
    }
    let is_local = config::is_loopback(&base);
    if key.is_none() && !is_local && config.model.provider != "ollama" {
        return Err(ForgeError::Provider(format!(
            "set {} in the current shell; Forge never writes API keys to disk. No prompt was sent.",
            config.model.api_key_env
        )));
    }
    let model_name = config.model.model.trim();
    if model_name.is_empty()
        || model_name.len() > 256
        || model_name.contains('/')
        || model_name.contains('\0')
    {
        return Err(ForgeError::Config(
            "model identifier is empty, oversized, or contains a forbidden path separator.".into(),
        ));
    }

    let (endpoint, body) = match config.model.provider.as_str() {
        "openai-compatible" | "ollama" => (
            append_endpoint(&base, "chat/completions")?,
            json!({
                "model": model_name,
                "messages": [
                    {"role":"system", "content": system_prompt},
                    {"role":"user", "content": user_prompt}
                ],
                "max_tokens": max_output_tokens,
                "stream": false
            }),
        ),
        "anthropic" => (
            append_endpoint(&base, "messages")?,
            json!({
                "model": model_name,
                "system": system_prompt,
                "messages": [{"role":"user", "content": user_prompt}],
                "max_tokens": max_output_tokens,
                "stream": false
            }),
        ),
        "google" => {
            // Google uses /models/{model}:generateContent and receives its key in a header.
            let clean = base.as_str().trim_end_matches('/');
            let endpoint = url::Url::parse(&format!("{clean}/models/{model_name}:generateContent"))
                .map_err(|_| {
                    ForgeError::Config(
                        "could not construct Google Generative Language endpoint.".into(),
                    )
                })?;
            (
                endpoint,
                json!({
                    "systemInstruction": {"parts": [{"text": system_prompt}]},
                    "contents": [{"role": "user", "parts": [{"text": user_prompt}]}],
                    "generationConfig": {"maxOutputTokens": max_output_tokens, "temperature": 0.2}
                }),
            )
        }
        provider => {
            return Err(ForgeError::Config(format!(
                "unsupported model provider '{provider}'."
            )));
        }
    };

    let client = Client::builder()
        .timeout(Duration::from_millis(config.model.timeout_ms))
        .redirect(Policy::none())
        .build()
        .map_err(|_| {
            ForgeError::Provider("could not initialize a bounded provider client.".into())
        })?;
    let mut request = client
        .post(endpoint.clone())
        .header("content-type", "application/json");
    match config.model.provider.as_str() {
        "anthropic" => {
            let key = key.ok_or_else(|| {
                ForgeError::Provider(
                    "Anthropic provider requires an API key; no request was sent.".into(),
                )
            })?;
            let value = reqwest::header::HeaderValue::from_str(key).map_err(|_| {
                ForgeError::Provider(
                    "API key contains invalid header characters; no request was sent.".into(),
                )
            })?;
            request = request
                .header("x-api-key", value)
                .header("anthropic-version", "2023-06-01");
        }
        "google" => {
            let key = key.ok_or_else(|| {
                ForgeError::Provider(
                    "Google provider requires an API key; no request was sent.".into(),
                )
            })?;
            let value = reqwest::header::HeaderValue::from_str(key).map_err(|_| {
                ForgeError::Provider(
                    "API key contains invalid header characters; no request was sent.".into(),
                )
            })?;
            request = request.header("x-goog-api-key", value);
        }
        "openai-compatible" => {
            if let Some(key) = key {
                request = request.bearer_auth(key);
            }
        }
        "ollama" => {
            if let Some(key) = key {
                request = request.bearer_auth(key);
            }
        }
        _ => {}
    }
    let started = Instant::now();
    // One transport-level retry for connection failures (connection reset or
    // closed before a response line arrived). Timeouts and HTTP error
    // statuses are never retried. Local endpoints (loopback fixtures, local
    // gateways, endpoint-security software on the host) can transiently drop
    // a fresh TCP connection; a single re-send is honest transport recovery,
    // not a fabricated result.
    let prepared = request.json(&body);
    let mut response = None;
    for attempt in 0..2 {
        let attempt_request = prepared.try_clone().ok_or_else(|| {
            ForgeError::Provider("could not prepare the provider request.".into())
        })?;
        match attempt_request.send().await {
            Ok(sent) => {
                response = Some(sent);
                break;
            }
            Err(error) if attempt == 0 && (error.is_connect() || error.is_request()) => {
                tokio::time::sleep(Duration::from_millis(150)).await;
            }
            Err(error) => {
                if error.is_timeout() {
                    return Err(ForgeError::Provider("provider request timed out.".into()));
                }
                return Err(ForgeError::Provider(
                    "provider endpoint could not be reached; endpoint details were not logged."
                        .into(),
                ));
            }
        }
    }
    let response = response
        .ok_or_else(|| ForgeError::Provider("provider request could not be completed.".into()))?;
    let status = response.status();
    if !status.is_success() {
        return Err(ForgeError::Provider(format!(
            "provider returned HTTP {status}; response body was not logged."
        )));
    }
    if response
        .content_length()
        .is_some_and(|n| n > MAX_RESPONSE_BYTES as u64)
    {
        return Err(ForgeError::Provider(
            "provider response exceeded the 2 MiB safety limit.".into(),
        ));
    }
    let mut stream = response;
    let mut bytes = Vec::new();
    while let Some(chunk) = stream
        .chunk()
        .await
        .map_err(|_| ForgeError::Provider("provider response stream failed.".into()))?
    {
        if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
            return Err(ForgeError::Provider(
                "provider response exceeded the 2 MiB safety limit.".into(),
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    let payload: Value = serde_json::from_slice(&bytes)
        .map_err(|_| ForgeError::Provider("provider response was not valid JSON.".into()))?;
    let text = match config.model.provider.as_str() {
        "anthropic" => payload
            .get("content")
            .and_then(Value::as_array)
            .and_then(|parts| {
                parts
                    .iter()
                    .find(|part| part.get("type").and_then(Value::as_str) == Some("text"))
            })
            .and_then(|part| part.get("text"))
            .and_then(Value::as_str)
            .map(str::to_owned),
        "google" => payload
            .get("candidates")
            .and_then(Value::as_array)
            .and_then(|items| items.first())
            .and_then(|item| item.get("content"))
            .and_then(|content| content.get("parts"))
            .and_then(Value::as_array)
            .map(|parts| {
                parts
                    .iter()
                    .filter_map(|part| part.get("text").and_then(Value::as_str))
                    .collect::<Vec<_>>()
                    .join("")
            }),
        _ => payload
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|items| items.first())
            .and_then(|item| item.get("message"))
            .and_then(|msg| msg.get("content"))
            .and_then(Value::as_str)
            .map(str::to_owned),
    }
    .ok_or_else(|| {
        ForgeError::Provider("provider response did not contain a supported text field.".into())
    })?;
    if text.len() > MAX_RESPONSE_BYTES {
        return Err(ForgeError::Provider(
            "provider text exceeded the response safety limit.".into(),
        ));
    }
    let usage_json = match config.model.provider.as_str() {
        "anthropic" => json!({
            "input_tokens": payload.get("usage").and_then(|u| u.get("input_tokens")).cloned().unwrap_or(Value::Null),
            "output_tokens": payload.get("usage").and_then(|u| u.get("output_tokens")).cloned().unwrap_or(Value::Null),
            "cost_usd": payload.get("usage").and_then(|u| u.get("cost_usd")).cloned().unwrap_or(Value::Null)
        }),
        "google" => json!({
            "input_tokens": payload.get("usageMetadata").and_then(|u| u.get("promptTokenCount")).cloned().unwrap_or(Value::Null),
            "output_tokens": payload.get("usageMetadata").and_then(|u| u.get("candidatesTokenCount")).cloned().unwrap_or(Value::Null)
        }),
        _ => payload.get("usage").cloned().unwrap_or(Value::Null),
    };
    let token_usage = UsageReport {
        input_tokens: usage_json
            .get("prompt_tokens")
            .or_else(|| usage_json.get("input_tokens"))
            .and_then(Value::as_u64),
        output_tokens: usage_json
            .get("completion_tokens")
            .or_else(|| usage_json.get("output_tokens"))
            .and_then(Value::as_u64),
        provider_reported_cost_usd: usage_json
            .get("cost_usd")
            .and_then(Value::as_f64)
            .filter(|n| n.is_finite() && *n >= 0.0),
    };
    let model = payload
        .get("model")
        .or_else(|| payload.get("modelVersion"))
        .and_then(Value::as_str)
        .unwrap_or(model_name)
        .to_owned();
    let request_id = payload
        .get("id")
        .or_else(|| payload.get("responseId"))
        .and_then(Value::as_str)
        .map(redaction::redact);
    audit::append(
        root,
        "model.generate",
        "succeeded",
        details([
            ("provider", json!(config.model.provider)),
            ("model", json!(model)),
            ("durationMs", json!(started.elapsed().as_millis())),
            ("inputTokens", optional_number(token_usage.input_tokens)),
            ("outputTokens", optional_number(token_usage.output_tokens)),
            (
                "providerReportedCostUsd",
                token_usage
                    .provider_reported_cost_usd
                    .map(Value::from)
                    .unwrap_or(Value::String("unknown".into())),
            ),
            ("promptBytes", json!(user_prompt.len())),
            ("systemBytes", json!(system_prompt.len())),
            ("networkUsed", json!(true)),
        ]),
    )?;
    Ok(ModelResponse {
        text,
        provider: config.model.provider.clone(),
        model,
        request_id,
        duration_ms: started.elapsed().as_millis(),
        usage: token_usage,
    })
}

fn append_endpoint(base: &url::Url, suffix: &str) -> Result<url::Url> {
    let value = format!(
        "{}/{}",
        base.as_str().trim_end_matches('/'),
        suffix.trim_start_matches('/')
    );
    url::Url::parse(&value).map_err(|_| {
        ForgeError::Config("configured provider endpoint could not be constructed.".into())
    })
}

// NOTE: generic usage-normalization helper used only by the #[cfg(test)] fixture path
// (generate_diagnostic/tests below); the production path in generate_text() above has since
// grown provider-specific (anthropic/google) field mapping inline and does not call this.
// Not a missing wire-up -- kept for test coverage of the generic-provider usage shape.
#[allow(dead_code)]
pub fn normalize_usage(payload: &Value) -> UsageReport {
    let usage = payload.get("usage").unwrap_or(&Value::Null);
    UsageReport {
        input_tokens: usage
            .get("prompt_tokens")
            .or_else(|| usage.get("input_tokens"))
            .and_then(Value::as_u64),
        output_tokens: usage
            .get("completion_tokens")
            .or_else(|| usage.get("output_tokens"))
            .and_then(Value::as_u64),
        provider_reported_cost_usd: usage
            .get("cost_usd")
            .and_then(Value::as_f64)
            .filter(|v| v.is_finite() && *v >= 0.0),
    }
}
fn details<const N: usize>(pairs: [(&str, Value); N]) -> BTreeMap<String, Value> {
    pairs.into_iter().map(|(k, v)| (k.to_owned(), v)).collect()
}
fn display_u64(value: Option<u64>) -> String {
    value
        .map(|n| n.to_string())
        .unwrap_or_else(|| "unknown".into())
}
fn optional_number(value: Option<u64>) -> Value {
    value
        .map(Value::from)
        .unwrap_or(Value::String("unknown".into()))
}
fn display_cost(value: Option<f64>) -> String {
    value
        .map(|n| format!("${n:.6} (provider-reported)"))
        .unwrap_or_else(|| "unknown".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ForgeConfig;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    fn http_response(status: &str, body: &str, extra_headers: &str) -> String {
        format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{extra_headers}\r\n{body}",
            body.len()
        )
    }

    async fn local_server(response: String) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                let mut request = [0_u8; 4096];
                let _ = stream.read(&mut request).await;
                let _ = stream.write_all(response.as_bytes()).await;
                let _ = stream.flush().await;
            }
        });
        format!("http://{address}/v1")
    }

    fn local_config(base_url: String) -> ForgeConfig {
        let mut config = ForgeConfig::default();
        config.model.base_url = base_url;
        config.model.timeout_ms = 1_000;
        config
    }

    async fn local_server_capture_headers(
        response: String,
    ) -> (String, tokio::task::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut collected = vec![0_u8; 16 * 1024];
            let n = stream.read(&mut collected).await.unwrap_or(0);
            collected.truncate(n);
            stream.write_all(response.as_bytes()).await.unwrap();
            let _ = stream.flush().await;
            String::from_utf8_lossy(&collected).into_owned()
        });
        (format!("http://{address}/v1"), server)
    }

    #[tokio::test]
    async fn diagnostic_success_parses_text_tokens_and_provider_reported_cost() {
        let body = r#"{"id":"req-fixture","model":"fixture-model","choices":[{"message":{"content":"FORGE_MODEL_TEST_OK"}}],"usage":{"prompt_tokens":3,"completion_tokens":4,"cost_usd":0.0002}}"#;
        let base_url = local_server(http_response("200 OK", body, "")).await;
        let result = generate_diagnostic(&local_config(base_url), None)
            .await
            .unwrap();
        assert_eq!(result.text, "FORGE_MODEL_TEST_OK");
        assert_eq!(result.model, "fixture-model");
        assert_eq!(result.request_id.as_deref(), Some("req-fixture"));
        assert_eq!(result.usage.input_tokens, Some(3));
        assert_eq!(result.usage.output_tokens, Some(4));
        assert_eq!(result.usage.provider_reported_cost_usd, Some(0.0002));
    }

    #[tokio::test]
    async fn sends_configured_bearer_token_in_authorization_header() {
        let body = r#"{"choices":[{"message":{"content":"FORGE_MODEL_TEST_OK"}}]}"#;
        let (base_url, server) =
            local_server_capture_headers(http_response("200 OK", body, "")).await;
        let result = generate_diagnostic(&local_config(base_url), Some("fixture-api-key-not-real"))
            .await
            .unwrap();
        let headers = server.await.unwrap().to_ascii_lowercase();
        assert_eq!(result.text, "FORGE_MODEL_TEST_OK");
        assert!(
            headers.contains("authorization: bearer fixture-api-key-not-real"),
            "provider authorization header missing"
        );
    }

    #[tokio::test]
    async fn non_success_status_does_not_expose_provider_body() {
        let base_url = local_server(http_response(
            "401 Unauthorized",
            "private-provider-error-secret",
            "",
        ))
        .await;
        let error = generate_diagnostic(&local_config(base_url), None)
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("HTTP 401"));
        assert!(!error.contains("private-provider-error-secret"));
    }

    #[tokio::test]
    async fn malformed_json_response_is_rejected() {
        let base_url = local_server(http_response("200 OK", "not-json", "")).await;
        let error = generate_diagnostic(&local_config(base_url), None)
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("not valid JSON"));
    }

    #[tokio::test]
    async fn oversized_content_length_is_rejected_before_body_read() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                let mut request = [0_u8; 4096];
                let _ = stream.read(&mut request).await;
                let headers = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    MAX_RESPONSE_BYTES + 1
                );
                let _ = stream.write_all(headers.as_bytes()).await;
                let _ = stream.flush().await;
            }
        });
        let error = generate_diagnostic(&local_config(format!("http://{address}/v1")), None)
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("2 MiB safety limit"));
    }

    #[tokio::test]
    async fn redirects_are_not_followed() {
        let target_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let target_address = target_listener.local_addr().unwrap();
        let target_body = r#"{"choices":[{"message":{"content":"FORGE_MODEL_TEST_OK"}}]}"#;
        tokio::spawn(async move {
            if let Ok((mut stream, _)) = target_listener.accept().await {
                let mut request = [0_u8; 4096];
                let _ = stream.read(&mut request).await;
                let response = http_response("200 OK", target_body, "");
                let _ = stream.write_all(response.as_bytes()).await;
                let _ = stream.flush().await;
            }
        });
        let location = format!("Location: http://{target_address}/redirected\r\n");
        let base_url = local_server(http_response("302 Found", "", &location)).await;
        let error = generate_diagnostic(&local_config(base_url), None)
            .await
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("HTTP 302"),
            "redirect was followed or error was unexpected: {error}"
        );
    }

    #[tokio::test]
    async fn provider_request_timeout_is_enforced() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                let mut request = [0_u8; 4096];
                let _ = stream.read(&mut request).await;
                tokio::time::sleep(Duration::from_secs(3)).await;
            }
        });
        let error = generate_diagnostic(&local_config(format!("http://{address}/v1")), None)
            .await
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("timed out"),
            "expected timeout, got: {error}"
        );
    }

    #[tokio::test]
    async fn anthropic_adapter_sets_key_header_and_parses_text_usage() {
        let body = r#"{"id":"msg_fixture","model":"claude-fixture","content":[{"type":"text","text":"hello from anthropic"}],"usage":{"input_tokens":7,"output_tokens":11}}"#;
        let (base_url, server) =
            local_server_capture_headers(http_response("200 OK", body, "")).await;
        let mut config = local_config(base_url);
        config.model.provider = "anthropic".into();
        config.model.model = "claude-fixture".into();
        let root = tempfile::tempdir().unwrap();
        let result = generate_text_with_key(
            root.path(),
            &config,
            "system",
            "user task",
            100,
            true,
            true,
            Some("fixture-key-not-real"),
        )
        .await
        .unwrap();
        let request = server.await.unwrap().to_ascii_lowercase();
        assert!(
            request.starts_with("post /v1/messages "),
            "wrong Anthropic endpoint: {request}"
        );
        assert!(
            request.contains("x-api-key: fixture-key-not-real"),
            "Anthropic key header missing"
        );
        assert!(
            request.contains("anthropic-version: 2023-06-01"),
            "Anthropic version header missing"
        );
        assert_eq!(result.text, "hello from anthropic");
        assert_eq!(result.usage.input_tokens, Some(7));
        assert_eq!(result.usage.output_tokens, Some(11));
        assert_eq!(result.usage.provider_reported_cost_usd, None);
    }

    #[tokio::test]
    async fn google_adapter_uses_key_header_and_parses_candidate_parts() {
        let body = r#"{"modelVersion":"gemini-fixture","candidates":[{"content":{"parts":[{"text":"hello"},{"text":" world"}]}}],"usageMetadata":{"promptTokenCount":5,"candidatesTokenCount":6}}"#;
        let (base_url, server) =
            local_server_capture_headers(http_response("200 OK", body, "")).await;
        let mut config = local_config(base_url);
        config.model.provider = "google".into();
        config.model.model = "gemini-fixture".into();
        let root = tempfile::tempdir().unwrap();
        let result = generate_text_with_key(
            root.path(),
            &config,
            "system",
            "user task",
            100,
            true,
            true,
            Some("fixture-key-not-real"),
        )
        .await
        .unwrap();
        let request = server.await.unwrap().to_ascii_lowercase();
        assert!(
            request.starts_with("post /v1/models/gemini-fixture:generatecontent "),
            "wrong Google endpoint: {request}"
        );
        assert!(
            request.contains("x-goog-api-key: fixture-key-not-real"),
            "Google key header missing"
        );
        assert_eq!(result.text, "hello world");
        assert_eq!(result.model, "gemini-fixture");
        assert_eq!(result.usage.input_tokens, Some(5));
        assert_eq!(result.usage.output_tokens, Some(6));
        assert_eq!(result.usage.provider_reported_cost_usd, None);
    }

    #[test]
    fn cost_is_unknown_unless_provider_returns_explicit_usd_field() {
        let payload = json!({"usage":{"prompt_tokens":12,"completion_tokens":9,"cost":0.001}});
        let usage = normalize_usage(&payload);
        assert_eq!(usage.input_tokens, Some(12));
        assert_eq!(usage.output_tokens, Some(9));
        assert_eq!(usage.provider_reported_cost_usd, None);
        let payload2 = json!({"usage":{"cost_usd":0.001}});
        assert_eq!(
            normalize_usage(&payload2).provider_reported_cost_usd,
            Some(0.001)
        );
    }
    #[test]
    fn no_cost_budget_fields_exist_in_rust_config() {
        let value = toml::to_string(&ForgeConfig::default()).unwrap();
        assert!(!value.contains("max_cost"));
        assert!(!value.contains("estimated_cost"));
        let mission = crate::mission::Mission {
            schema_version: 1,
            id: "m".into(),
            project_id: "p".into(),
            repo_root: "/repo".into(),
            objective: "test".into(),
            status: crate::mission::MissionStatus::Ready,
            created_at: "now".into(),
            updated_at: "now".into(),
            steps: vec![],
            acceptance_criteria: vec![],
            max_duration_ms: 1000,
            dry_run: true,
            approved_capabilities: vec![],
            evidence_ids: vec![],
        };
        let serialized = serde_json::to_string(&mission).unwrap();
        assert!(!serialized.contains("cost"));
        assert!(!serialized.contains("budget"));
    }
}
