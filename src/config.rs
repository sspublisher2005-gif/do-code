use crate::error::{ForgeError, Result, io_error};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::Read,
    path::{Path, PathBuf},
};
use url::Url;
use uuid::Uuid;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ForgeConfig {
    pub project: ProjectConfig,
    pub model: ModelConfig,
    pub privacy: PrivacyConfig,
    pub limits: LimitsConfig,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ProjectConfig {
    pub name: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ModelConfig {
    pub provider: String,
    pub model: String,
    pub base_url: String,
    pub api_key_env: String,
    pub max_output_tokens: usize,
    pub timeout_ms: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PrivacyConfig {
    pub mode: String,
    pub telemetry: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LimitsConfig {
    pub max_read_bytes: u64,
    pub max_files: usize,
    pub max_search_matches: usize,
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            name: "Forge workspace".into(),
        }
    }
}
impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            provider: "openai-compatible".into(),
            model: "gpt-4.1-mini".into(),
            base_url: "https://api.openai.com/v1".into(),
            api_key_env: "OPENAI_API_KEY".into(),
            max_output_tokens: 4096,
            timeout_ms: 30_000,
        }
    }
}
impl Default for PrivacyConfig {
    fn default() -> Self {
        Self {
            mode: "ask".into(),
            telemetry: false,
        }
    }
}
impl Default for LimitsConfig {
    fn default() -> Self {
        Self {
            max_read_bytes: 262_144,
            max_files: 1_000,
            max_search_matches: 200,
        }
    }
}

pub struct LoadedConfig {
    pub config: ForgeConfig,
    pub exists: bool,
    pub path: PathBuf,
}

pub fn load(root: &Path) -> Result<LoadedConfig> {
    let canonical_root = fs::canonicalize(root).map_err(|e| io_error(root, e))?;
    let path = canonical_root.join("forge.config.toml");
    let metadata = match fs::symlink_metadata(&path) {
        Ok(meta) => {
            if meta.file_type().is_symlink() || !meta.is_file() {
                return Err(ForgeError::Safety(
                    "forge.config.toml must be a regular file, not a symlink or special file."
                        .into(),
                ));
            }
            if meta.len() > 128 * 1024 {
                return Err(ForgeError::Config(
                    "forge.config.toml exceeds 128 KiB.".into(),
                ));
            }
            Some(meta)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(io_error(&path, e)),
    };
    let config = if metadata.is_some() {
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        }
        let mut file = options.open(&path).map_err(|e| io_error(&path, e))?;
        let opened_meta = file.metadata().map_err(|e| io_error(&path, e))?;
        if !opened_meta.is_file() || opened_meta.len() > 128 * 1024 {
            return Err(ForgeError::Safety(
                "opened forge.config.toml is not a bounded regular file.".into(),
            ));
        }
        crate::storage::reject_hardlink_handle(&file, &path)?;
        let mut source = String::with_capacity(opened_meta.len() as usize);
        (&mut file)
            .take(128 * 1024 + 1)
            .read_to_string(&mut source)
            .map_err(|e| io_error(&path, e))?;
        if source.len() > 128 * 1024 {
            return Err(ForgeError::Config(
                "forge.config.toml exceeded 128 KiB while being read.".into(),
            ));
        }
        toml::from_str::<ForgeConfig>(&source)
            .map_err(|e| ForgeError::Config(format!("invalid forge.config.toml: {e}")))?
    } else {
        ForgeConfig::default()
    };
    validate(&config)?;
    Ok(LoadedConfig {
        config,
        exists: metadata.is_some(),
        path,
    })
}

pub fn validate(config: &ForgeConfig) -> Result<()> {
    if !["openai-compatible", "ollama", "anthropic", "google"]
        .contains(&config.model.provider.as_str())
    {
        return Err(ForgeError::Config(
            "model.provider must be openai-compatible, ollama, anthropic, or google.".into(),
        ));
    }
    if config.model.model.trim().is_empty() || config.model.model.len() > 256 {
        return Err(ForgeError::Config(
            "model.model must be a non-empty identifier up to 256 bytes.".into(),
        ));
    }
    if !(1..=128_000).contains(&config.model.max_output_tokens) {
        return Err(ForgeError::Config(
            "model.max_output_tokens must be between 1 and 128000.".into(),
        ));
    }
    if !(1_000..=300_000).contains(&config.model.timeout_ms) {
        return Err(ForgeError::Config(
            "model.timeout_ms must be between 1000 and 300000.".into(),
        ));
    }
    if config.privacy.telemetry {
        return Err(ForgeError::Config(
            "telemetry is disabled by the Forge product policy in this version.".into(),
        ));
    }
    if !["ask", "local-only"].contains(&config.privacy.mode.as_str()) {
        return Err(ForgeError::Config(
            "privacy.mode must be ask or local-only.".into(),
        ));
    }
    if !(1024..=16 * 1024 * 1024).contains(&config.limits.max_read_bytes) {
        return Err(ForgeError::Config(
            "limits.max_read_bytes must be 1024..=16777216.".into(),
        ));
    }
    if !(1..=100_000).contains(&config.limits.max_files) {
        return Err(ForgeError::Config(
            "limits.max_files must be 1..=100000.".into(),
        ));
    }
    if !(1..=10_000).contains(&config.limits.max_search_matches) {
        return Err(ForgeError::Config(
            "limits.max_search_matches must be 1..=10000.".into(),
        ));
    }
    let key_allowlist: &[&str] = match config.model.provider.as_str() {
        "openai-compatible" => &[
            "FORGE_MODEL_API_KEY",
            "OPENAI_API_KEY",
            "OPENAI_COMPATIBLE_API_KEY",
            "OPENROUTER_API_KEY",
            "DEEPSEEK_API_KEY",
        ],
        "anthropic" => &["FORGE_MODEL_API_KEY", "ANTHROPIC_API_KEY"],
        "google" => &["FORGE_MODEL_API_KEY", "GOOGLE_API_KEY", "GEMINI_API_KEY"],
        "ollama" => &["FORGE_MODEL_API_KEY", "OLLAMA_API_KEY"],
        _ => &[],
    };
    if !key_allowlist.contains(&config.model.api_key_env.as_str()) {
        return Err(ForgeError::Config("model.api_key_env must be a provider-allowlisted credential variable; arbitrary environment variables are rejected.".into()));
    }
    validate_base_url(&config.model.base_url)?;
    Ok(())
}

pub fn validate_base_url(raw: &str) -> Result<Url> {
    let url = Url::parse(raw)
        .map_err(|_| ForgeError::Config("model.base_url is not a valid URL.".into()))?;
    let local_http = url.scheme() == "http" && is_loopback(&url);
    if url.scheme() != "https" && !local_http {
        return Err(ForgeError::Safety(
            "provider endpoint must use HTTPS; HTTP is allowed only for loopback endpoints.".into(),
        ));
    }
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(ForgeError::Safety(
            "provider URL must not contain userinfo, query parameters, or a fragment.".into(),
        ));
    }
    if url
        .host_str()
        .and_then(parse_ip_host)
        .is_some_and(|ip| ip.is_unspecified())
    {
        return Err(ForgeError::Safety(
            "unspecified-address provider endpoints are not allowed.".into(),
        ));
    }
    let path = url.path().trim_end_matches('/');
    if path.ends_with("/chat/completions") || path.ends_with("/models") {
        return Err(ForgeError::Config(
            "configure the API base path, not an individual API endpoint.".into(),
        ));
    }
    Ok(url)
}

fn parse_ip_host(host: &str) -> Option<std::net::IpAddr> {
    host.trim_start_matches('[')
        .trim_end_matches(']')
        .parse::<std::net::IpAddr>()
        .ok()
}

pub fn is_loopback(url: &Url) -> bool {
    match url.host_str() {
        Some(host) if host.eq_ignore_ascii_case("localhost") => true,
        Some(host) => parse_ip_host(host).is_some_and(|ip| ip.is_loopback()),
        None => false,
    }
}

pub fn initialize(root: &Path) -> Result<(bool, bool)> {
    let canonical_root = fs::canonicalize(root).map_err(|e| io_error(root, e))?;
    let home = canonical_root.join(".forge");
    match fs::symlink_metadata(&home) {
        Ok(meta) if meta.file_type().is_symlink() || !meta.is_dir() => {
            return Err(ForgeError::Safety(
                ".forge must be a real directory, not a symlink or special file.".into(),
            ));
        }
        Ok(_) => {
            set_private_permissions(&home)?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            match fs::create_dir(&home) {
                Ok(()) => {}
                Err(create_error) if create_error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(create_error) => return Err(io_error(&home, create_error)),
            }
            let after = fs::symlink_metadata(&home).map_err(|e| io_error(&home, e))?;
            if after.file_type().is_symlink() || !after.is_dir() {
                return Err(ForgeError::Safety(
                    ".forge must be a real directory, not a symlink or special file.".into(),
                ));
            }
            set_private_permissions(&home)?;
        }
        Err(e) => return Err(io_error(&home, e)),
    }
    let config_path = canonical_root.join("forge.config.toml");
    let config_contents = toml::to_string_pretty(&ForgeConfig::default())
        .map_err(|e| ForgeError::Config(e.to_string()))?;
    let config_created = create_file_exclusive(&config_path, config_contents.as_bytes(), 0o600)?;
    let memory_path = canonical_root.join("FORGE.md");
    let memory_created = create_file_exclusive(&memory_path, DEFAULT_MEMORY.as_bytes(), 0o600)?;
    Ok((config_created, memory_created))
}

fn create_file_exclusive(path: &Path, bytes: &[u8], mode: u32) -> Result<bool> {
    use std::io::Write;
    #[cfg(not(unix))]
    let _ = mode;
    let parent = path
        .parent()
        .ok_or_else(|| ForgeError::Safety("target file has no parent directory.".into()))?;
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| ForgeError::Safety("target file name is not valid Unicode.".into()))?;
    let temp_path = parent.join(format!(".{file_name}.{}.tmp", Uuid::new_v4()));
    let result = (|| {
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(mode);
        }
        let mut file = options
            .open(&temp_path)
            .map_err(|e| io_error(&temp_path, e))?;
        file.write_all(bytes).map_err(|e| io_error(&temp_path, e))?;
        file.sync_all().map_err(|e| io_error(&temp_path, e))?;
        drop(file);
        match fs::hard_link(&temp_path, path) {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                let meta = fs::symlink_metadata(path).map_err(|e| io_error(path, e))?;
                if meta.file_type().is_symlink() || !meta.is_file() {
                    return Err(ForgeError::Safety(format!(
                        "{} exists but is not a regular file; refusing to overwrite it.",
                        path.display()
                    )));
                }
                Ok(false)
            }
            Err(e) => Err(io_error(path, e)),
        }
    })();
    let cleanup_result = fs::remove_file(&temp_path);
    match result {
        Ok(created) => {
            if let Err(error) = cleanup_result
                && error.kind() != std::io::ErrorKind::NotFound
            {
                return Err(io_error(&temp_path, error));
            }
            Ok(created)
        }
        Err(error) => {
            let _ = cleanup_result;
            Err(error)
        }
    }
}

pub fn set_private_permissions(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|e| io_error(path, e))?;
        Ok(())
    }
    #[cfg(windows)]
    {
        Err(ForgeError::Safety(format!(
            "Windows private ACL enforcement is not implemented; refusing to use state path '{}'.",
            path.display()
        )))
    }
    #[cfg(not(any(unix, windows)))]
    {
        Err(ForgeError::Safety(
            "private state permissions are unsupported on this platform.".into(),
        ))
    }
}

const DEFAULT_MEMORY: &str = "# Project memory\n\nThis file is untrusted project context, not a security policy. Never store credentials here.\n\n## Goal\nDescribe the project and task here.\n\n## Conventions\n- Record actual build and test commands.\n- Record architecture decisions and constraints.\n- Do not claim tests passed unless they were run.\n";

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn windows_private_permissions_fail_closed_until_acl_enforcement_exists() {
        let directory = tempfile::tempdir().unwrap();
        let error = set_private_permissions(directory.path()).unwrap_err();
        assert!(matches!(error, ForgeError::Safety(_)));
        assert!(
            error
                .to_string()
                .contains("Windows private ACL enforcement is not implemented")
        );
    }

    #[test]
    fn rejects_unknown_config_keys_and_arbitrary_key_sources() {
        let unknown = "[model]\nunknown_key = true\n";
        assert!(toml::from_str::<ForgeConfig>(unknown).is_err());
        let arbitrary = "[model]\napi_key_env = \"AWS_SECRET_ACCESS_KEY\"\n";
        let parsed = toml::from_str::<ForgeConfig>(arbitrary).unwrap();
        assert!(validate(&parsed).is_err());
    }

    #[test]
    fn rejects_remote_http_and_credential_bearing_urls() {
        assert!(validate_base_url("http://provider.example/v1").is_err());
        assert!(validate_base_url("https://user:pass@provider.example/v1").is_err());
        assert!(validate_base_url("https://provider.example/v1?secret=yes").is_err());
        assert!(validate_base_url("https://provider.example/v1/chat/completions").is_err());
        assert!(validate_base_url("https://0.0.0.0/v1").is_err());
        assert!(validate_base_url("https://[::]/v1").is_err());
        assert!(validate_base_url("http://127.0.0.1:11434/v1").is_ok());
        assert!(validate_base_url("http://[::1]:11434/v1").is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn init_preflights_unsafe_forge_directory_before_writing_project_files() {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        symlink(outside.path(), root.path().join(".forge")).unwrap();
        assert!(initialize(root.path()).is_err());
        assert!(!root.path().join("forge.config.toml").exists());
        assert!(!root.path().join("FORGE.md").exists());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinked_config_file() {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let external = outside.path().join("external-config.toml");
        fs::write(&external, toml::to_string(&ForgeConfig::default()).unwrap()).unwrap();
        symlink(&external, root.path().join("forge.config.toml")).unwrap();
        assert!(load(root.path()).is_err());
    }

    #[test]
    fn rejects_oversized_config_before_parsing() {
        let root = tempfile::tempdir().unwrap();
        fs::write(
            root.path().join("forge.config.toml"),
            "x".repeat(128 * 1024 + 1),
        )
        .unwrap();
        assert!(load(root.path()).is_err());
    }

    #[test]
    fn missing_sections_get_safe_defaults() {
        let parsed = toml::from_str::<ForgeConfig>("[project]\nname = \"test\"\n").unwrap();
        validate(&parsed).unwrap();
        assert_eq!(parsed.project.name, "test");
        assert!(!parsed.privacy.telemetry);
    }
}
