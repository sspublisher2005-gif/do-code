use regex::Regex;
use serde_json::Value;
use std::sync::LazyLock;

static RULES: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    vec![
    (Regex::new(r"\bsk-[A-Za-z0-9_-]{12,}\b").expect("valid regex"), "[REDACTED_OPENAI_KEY]"),
    (Regex::new(r"\bAIza[0-9A-Za-z_-]{20,}\b").expect("valid regex"), "[REDACTED_GOOGLE_KEY]"),
    (Regex::new(r"\bgh[pousr]_[A-Za-z0-9]{20,}\b").expect("valid regex"), "[REDACTED_GITHUB_TOKEN]"),
    (Regex::new(r"\bxox[baprs]-[A-Za-z0-9-]{10,}\b").expect("valid regex"), "[REDACTED_SLACK_TOKEN]"),
    (Regex::new(r"(?i)(Bearer\s+)[A-Za-z0-9._~+/=-]+").expect("valid regex"), "$1[REDACTED]"),
    // Mask complete quoted values first, including spaces and punctuation, while
    // preserving the surrounding JSON/YAML/dotenv quote so common lines remain valid.
    (Regex::new(r#"(?i)(["']?[A-Za-z0-9_-]*(?:api[_-]?key|access[_-]?token|refresh[_-]?token|password|client[_-]?secret|secret|token|authorization|cookie|private[_-]?key|access[_-]?key|aws[_-]?secret[_-]?access[_-]?key)["']?\s*[:=]\s*")(?:\\.|[^"\\\r\n])*""#).expect("valid regex"), "$1[REDACTED]\"") ,
    (Regex::new(r#"(?i)(["']?[A-Za-z0-9_-]*(?:api[_-]?key|access[_-]?token|refresh[_-]?token|password|client[_-]?secret|secret|token|authorization|cookie|private[_-]?key|access[_-]?key|aws[_-]?secret[_-]?access[_-]?key)["']?\s*[:=]\s*')(?:\\.|[^'\\\r\n])*'"#).expect("valid regex"), "$1[REDACTED]'"),
    // For unquoted assignments, redact the full value through end-of-line (or a
    // semicolon delimiter) so a secret containing spaces cannot leak its suffix.
    (Regex::new(r#"(?im)(["']?[A-Za-z0-9_-]*(?:api[_-]?key|access[_-]?token|refresh[_-]?token|password|client[_-]?secret|secret|token|authorization|cookie|private[_-]?key|access[_-]?key|aws[_-]?secret[_-]?access[_-]?key)["']?\s*[:=]\s*)([^\s"'\r\n;][^\r\n;]*)"#).expect("valid regex"), "$1[REDACTED]"),
]
});

pub fn redact(input: &str) -> String {
    let mut output = input.to_owned();
    for (pattern, replacement) in RULES.iter() {
        output = pattern.replace_all(&output, *replacement).into_owned();
    }
    output
}

pub fn redact_json(value: &mut Value) {
    match value {
        Value::String(text) => *text = safe_terminal(&redact(text)),
        Value::Array(items) => {
            for item in items {
                redact_json(item);
            }
        }
        Value::Object(map) => {
            // JSON serializers escape ASCII control characters but may preserve Unicode
            // bidi/formatting controls in object keys. Rebuild the map so keys are safe too.
            let original = std::mem::take(map);
            for (key, mut value) in original {
                if is_secret_key(&key) {
                    value = Value::String("[REDACTED]".into());
                } else {
                    redact_json(&mut value);
                }
                map.insert(safe_terminal(&key), value);
            }
        }
        _ => {}
    }
}

pub fn is_secret_key(key: &str) -> bool {
    // Normalize to ASCII alphanumerics so inserted punctuation, spaces, or bidi
    // format controls cannot disguise common credential-bearing field names.
    let normalized = key
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase();
    matches!(
        normalized.as_str(),
        "apikey" | "password" | "secret" | "token" | "authorization" | "cookie"
    ) || normalized.contains("apikey")
        || normalized.contains("secret")
        || normalized.contains("password")
        || normalized.contains("authorization")
        || normalized.contains("cookie")
        || normalized.contains("privatekey")
        || normalized.ends_with("token")
        || normalized.ends_with("credential")
        || normalized.contains("accesskey")
}

pub fn sanitize_json_strings(value: &mut Value) {
    match value {
        Value::String(text) => *text = safe_terminal(&redact(text)),
        Value::Array(items) => {
            for item in items {
                sanitize_json_strings(item);
            }
        }
        Value::Object(map) => {
            let original = std::mem::take(map);
            for (key, mut value) in original {
                sanitize_json_strings(&mut value);
                map.insert(safe_terminal(&key), value);
            }
        }
        _ => {}
    }
}

pub fn safe_terminal(input: &str) -> String {
    input
        .chars()
        .map(|ch| {
            let cp = ch as u32;
            // Escape terminal control bytes and Unicode bidi/zero-width formatting controls
            // that can make paths or diagnostics visually misleading. Leave ZWJ/ZWNJ alone
            // because they are required for many scripts and emoji sequences.
            let terminal_control = (cp <= 0x08)
                || (0x0b..=0x1f).contains(&cp)
                || (0x7f..=0x9f).contains(&cp)
                || cp == 0x061c
                || (0x200b..=0x200f).contains(&cp) && cp != 0x200c && cp != 0x200d
                || (0x202a..=0x202e).contains(&cp)
                || (0x2066..=0x206f).contains(&cp)
                || (cp == 0x2028 || cp == 0x2029 || cp == 0xfeff);
            if terminal_control {
                format!("\\u{{{cp:04x}}}")
            } else {
                ch.to_string()
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn redacts_common_keys_and_bearer_headers() {
        let value = redact("key=abc123; Bearer tokenvalue1234567890; sk-123456789012345678901234");
        assert!(!value.contains("tokenvalue1234567890"));
        assert!(!value.contains("sk-123456789012345678901234"));
        assert!(value.contains("REDACTED"));
    }
    #[test]
    fn secret_fields_are_redacted_recursively() {
        let mut value = serde_json::json!({"api_key":"secret", "nested":{"token":"sensitive","client_secret":"secret2","access-token":"secret3","note":"safe"}});
        redact_json(&mut value);
        assert_eq!(value["api_key"], "[REDACTED]");
        assert_eq!(value["nested"]["token"], "[REDACTED]");
        assert_eq!(value["nested"]["client_secret"], "[REDACTED]");
        assert_eq!(value["nested"]["access-token"], "[REDACTED]");
        assert_eq!(value["nested"]["note"], "safe");
    }

    #[test]
    fn redacts_quoted_json_keys_and_generic_secret_assignments() {
        let json_like =
            r#"{"api_key":"json-secret-value-123", "access_token": "token-secret-value-123"}"#;
        let output = redact(json_like);
        assert!(!output.contains("json-secret-value-123"));
        assert!(!output.contains("token-secret-value-123"));
        assert!(output.contains("[REDACTED]"));
        assert!(
            serde_json::from_str::<serde_json::Value>(&output).is_ok(),
            "redaction should preserve common JSON structure: {output}"
        );
        let escaped_json = r#"{"password":"quote\"inside-secret", "ok": "safe-value"}"#;
        let escaped_output = redact(escaped_json);
        assert!(!escaped_output.contains("inside-secret"));
        assert!(
            serde_json::from_str::<serde_json::Value>(&escaped_output).is_ok(),
            "escaped quoted values should remain valid JSON: {escaped_output}"
        );
        let named_json = r#"{"session_access_token":"token with spaces, too", "ok":"safe"}"#;
        let named_output = redact(named_json);
        assert!(!named_output.contains("token with spaces"));
        assert!(
            serde_json::from_str::<serde_json::Value>(&named_output).is_ok(),
            "prefixed JSON secret fields should stay valid: {named_output}"
        );
        let dotenv =
            "SECRET=secret-value-123; PRIVATE_KEY=private-value-123; COOKIE=session-cookie-123";
        let output = redact(dotenv);
        assert!(!output.contains("secret-value-123"));
        assert!(!output.contains("private-value-123"));
        assert!(!output.contains("session-cookie-123"));
        let spaced = "PASSWORD=\"sensitive value, with spaces\"\nSECRET=long secret with spaces; SAFE=keep-this";
        let output = redact(spaced);
        assert!(!output.contains("sensitive value"));
        assert!(!output.contains("long secret with spaces"));
        assert!(output.contains("SAFE=keep-this"));
        let prefixed = "OPENAI_API_KEY=multi word secret; DATABASE_PASSWORD=private text; KEEP=yes";
        let prefixed_output = redact(prefixed);
        assert!(!prefixed_output.contains("multi word secret"));
        assert!(!prefixed_output.contains("private text"));
        assert!(prefixed_output.contains("KEEP=yes"));
    }

    #[test]
    fn sanitizes_untrusted_json_object_keys_as_well_as_values() {
        let hostile_key = format!("hostile{}[31m", '\u{001b}');
        let mut value = serde_json::Map::new();
        value.insert(hostile_key, Value::String("ordinary text".into()));
        value.insert("api_key".into(), Value::String("untrusted-secret".into()));
        value.insert(
            "api\u{202e}_key".into(),
            Value::String("bidi-hidden-secret".into()),
        );
        let mut value = Value::Object(value);
        redact_json(&mut value);
        let object = value.as_object().unwrap();
        assert!(object.contains_key("hostile\\u{001b}[31m"));
        assert_eq!(
            object.get("api_key"),
            Some(&Value::String("[REDACTED]".into()))
        );
        assert!(
            object
                .iter()
                .any(|(key, value)| key.contains("202e")
                    && value == &Value::String("[REDACTED]".into())),
            "bidi-obfuscated secret field was not redacted: {object:?}"
        );
    }

    #[test]
    fn escapes_bidi_and_terminal_controls_without_destroying_zwj() {
        let rendered = safe_terminal("a\u{001b}[31m\u{202e}txt\u{200d}👩");
        assert!(rendered.contains("\\u{001b}"));
        assert!(rendered.contains("\\u{202e}"));
        assert!(rendered.contains('\u{200d}'));
    }
}
