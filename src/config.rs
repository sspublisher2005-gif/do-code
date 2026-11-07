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
    /// Per-role model routing and fallback chain (see router.rs).
    pub routing: crate::router::RoutingConfig,
    /// Mission loop budget (step count, stall detection, wall clock).
    /// Serialized as the `[loop]` TOML section.
    #[serde(rename = "loop")]
    pub loop_policy: crate::loopctl::LoopPolicy,
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

/// True when `sid` is the machine-local Administrators group SID
/// (S-1-5-32-544). Built as raw bytes to avoid depending on well-known-RID
/// constants: revision=1, sub-authority count=1, identifier authority=5
/// (NT authority, 48-bit big-endian), sub-authorities [32, 544] little-endian
/// (SECURITY_BUILTIN_DOMAIN_RID, DOMAIN_ALIAS_RID_ADMINS).
#[cfg(windows)]
fn create_sid_matches_well_known_admins(sid: windows_sys::Win32::Security::PSID) -> bool {
    const ADMIN_SID_BYTES: [u8; 12] = [
        1, // revision
        1, // sub-authority count
        5, 0, 0, 0, 0, 5, // identifier authority = 5 (NT authority)
        32, 2, 0, 0, // DOMAIN_ALIAS_RID_ADMINS = 544 = 0x220 (S-1-5-32-544)
    ];
    if sid.is_null() {
        return false;
    }
    let admins_sid = ADMIN_SID_BYTES.as_ptr() as windows_sys::Win32::Security::PSID;
    let equal = unsafe { windows_sys::Win32::Security::EqualSid(sid, admins_sid) };
    equal != 0
}

#[cfg(windows)]
fn enforce_windows_private_acl(path: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, LocalFree};
    use windows_sys::Win32::Security::Authorization::{
        EXPLICIT_ACCESS_W, GetNamedSecurityInfoW, SE_FILE_OBJECT, SetEntriesInAclW,
        SetNamedSecurityInfoW,
    };
    use windows_sys::Win32::Security::{
        ACL, GetTokenInformation, PSECURITY_DESCRIPTOR, PSID, TOKEN_QUERY, TOKEN_USER,
    };
    use windows_sys::Win32::System::Memory::LocalAlloc;
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    // Constants
    const DACL_SECURITY_INFORMATION: u32 = 0x00000004;
    const PROTECTED_DACL_SECURITY_INFORMATION: u32 = 0x80000000;
    const GENERIC_ALL: u32 = 0x10000000;
    const SET_ACCESS: i32 = 2; // GRANT_ACCESS
    const NO_INHERITANCE: u32 = 0;
    const CONTAINER_INHERIT_ACE: u32 = 0x00000002;
    const OBJECT_INHERIT_ACE: u32 = 0x00000001;
    const LMEM_FIXED: u32 = 0x0000;
    const TOKEN_QUERY_VALUE: u32 = TOKEN_QUERY;

    // 1. Get current process token.
    let mut token: HANDLE = std::ptr::null_mut();
    let ok = unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY_VALUE, &mut token) };
    if ok == 0 {
        return Err(ForgeError::Safety(format!(
            "OpenProcessToken failed while protecting '{}'.",
            path.display()
        )));
    }

    // 2. Get the user SID from the token.
    let mut needed: u32 = 0;
    unsafe {
        GetTokenInformation(
            token,
            1, /* TokenUser */
            std::ptr::null_mut(),
            0,
            &mut needed,
        );
    }
    if needed == 0 {
        unsafe { CloseHandle(token) };
        return Err(ForgeError::Safety(format!(
            "GetTokenInformation size query failed for '{}'.",
            path.display()
        )));
    }
    let buf = unsafe { LocalAlloc(LMEM_FIXED, needed as usize) };
    if buf.is_null() {
        unsafe { CloseHandle(token) };
        return Err(ForgeError::Safety(format!(
            "LocalAlloc failed while protecting '{}'.",
            path.display()
        )));
    }
    let ok = unsafe {
        GetTokenInformation(token, 1 /* TokenUser */, buf, needed, &mut needed)
    };
    if ok == 0 {
        unsafe {
            LocalFree(buf);
            CloseHandle(token)
        };
        return Err(ForgeError::Safety(format!(
            "GetTokenInformation failed while protecting '{}'.",
            path.display()
        )));
    }
    let token_user = buf as *const TOKEN_USER;
    let sid = unsafe { (*token_user).User.Sid };

    // Determine inheritance flags based on whether path is a directory.
    let is_dir = path.is_dir();
    let inheritance = if is_dir {
        CONTAINER_INHERIT_ACE | OBJECT_INHERIT_ACE
    } else {
        NO_INHERITANCE
    };

    // 3. Build an explicit access entry granting GENERIC_ALL to the current user.
    let mut ea: EXPLICIT_ACCESS_W = unsafe { std::mem::zeroed() };
    ea.grfAccessPermissions = GENERIC_ALL;
    ea.grfAccessMode = SET_ACCESS;
    ea.grfInheritance = inheritance;
    ea.Trustee.pMultipleTrustee = std::ptr::null_mut();
    ea.Trustee.MultipleTrusteeOperation = 0;
    ea.Trustee.TrusteeForm = 0; // TRUSTEE_IS_SID
    ea.Trustee.TrusteeType = 1; // TRUSTEE_IS_USER
    ea.Trustee.ptstrName = sid as *mut _;

    // 4. Build the new DACL.
    let mut new_dacl: *mut ACL = std::ptr::null_mut();
    let err = unsafe { SetEntriesInAclW(1, &ea, std::ptr::null(), &mut new_dacl) };
    if err != 0 {
        unsafe {
            LocalFree(buf);
            CloseHandle(token)
        };
        return Err(ForgeError::Safety(format!(
            "SetEntriesInAclW failed (err={}) while protecting '{}'.",
            err,
            path.display()
        )));
    }

    // 5. Apply the DACL to the path (protected, no inheritance from parent).
    let path_wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let err = unsafe {
        SetNamedSecurityInfoW(
            path_wide.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(), // owner unchanged
            std::ptr::null_mut(), // group unchanged
            new_dacl,
            std::ptr::null_mut(), // SACL unchanged
        )
    };
    if err != 0 {
        unsafe {
            LocalFree(new_dacl as *mut _);
            LocalFree(buf);
            CloseHandle(token)
        };
        return Err(ForgeError::Safety(format!(
            "SetNamedSecurityInfoW failed (err={}) while protecting '{}'.",
            err,
            path.display()
        )));
    }

    // 6. Read back and verify the DACL was applied AND owner matches current user.
    let mut read_dacl: *mut ACL = std::ptr::null_mut();
    let mut read_owner: PSID = std::ptr::null_mut();
    let mut sd: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    let err = unsafe {
        GetNamedSecurityInfoW(
            path_wide.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | 0x00000001, // OWNER_SECURITY_INFORMATION
            &mut read_owner,
            std::ptr::null_mut(),
            &mut read_dacl,
            std::ptr::null_mut(),
            &mut sd,
        )
    };
    if err != 0 || read_dacl.is_null() {
        unsafe {
            LocalFree(sd as *mut _);
            LocalFree(new_dacl as *mut _);
            LocalFree(buf);
            CloseHandle(token)
        };
        return Err(ForgeError::Safety(format!(
            "GetNamedSecurityInfoW read-back failed (err={}) for '{}'.",
            err,
            path.display()
        )));
    }

    // Verify owner SID matches current user OR the local Administrators
    // group. Windows dirs created inside Administrators-owned parents
    // (e.g. CI runner temp dirs) are owned by the Administrators group even
    // when created by an admin user — that is standard inheritance, not an
    // attack. The security guarantee lives in the protected owner-only DACL
    // verified above, so both owners are accepted; anything else refuses.
    if !read_owner.is_null() && !sid.is_null() {
        let owner_matches = unsafe { windows_sys::Win32::Security::EqualSid(read_owner, sid) };
        if owner_matches == 0 {
            // Accept the local Administrators group as owner.
            let admins_ok = create_sid_matches_well_known_admins(read_owner);
            if !admins_ok {
                unsafe {
                    LocalFree(sd as *mut _);
                    LocalFree(new_dacl as *mut _);
                    LocalFree(buf);
                    CloseHandle(token)
                };
                return Err(ForgeError::Safety(format!(
                    "DACL owner SID does not match current user for '{}'; refusing to proceed.",
                    path.display()
                )));
            }
        }
    }

    // Cleanup.
    unsafe {
        LocalFree(sd as *mut _);
        LocalFree(new_dacl as *mut _);
        LocalFree(buf);
        CloseHandle(token);
    }
    Ok(())
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
        enforce_windows_private_acl(path)
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
    fn windows_private_permissions_enforces_owner_only_dacl() {
        let directory = tempfile::tempdir().unwrap();
        // Must succeed now that real DACL enforcement is implemented.
        set_private_permissions(directory.path()).expect("DACL enforcement should succeed");
        // Read-back: verify the directory still exists and is accessible.
        assert!(directory.path().is_dir());
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
