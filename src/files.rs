use crate::error::{ForgeError, Result, io_error};
use std::{
    collections::{BTreeSet, VecDeque},
    ffi::OsString,
    fs::{self, OpenOptions},
    io::Read,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, serde::Serialize)]
pub struct FileEntry {
    pub path: String,
    pub kind: &'static str,
    pub size_bytes: u64,
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct SearchMatch {
    pub path: String,
    pub line: usize,
    pub text: String,
}

const BINARY_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "ico", "pdf", "zip", "gz", "tgz", "7z", "exe", "dll",
    "so", "dylib", "class", "jar", "woff", "woff2", "ttf", "otf", "mp4", "mov", "mp3", "wav",
    "sqlite", "db",
];
const SKIP_DIRECTORIES: &[&str] = &[
    ".git",
    ".forge",
    "node_modules",
    "dist",
    "coverage",
    "vendor",
    ".next",
    ".nuxt",
    ".venv",
    "venv",
    "__pycache__",
    ".pytest_cache",
    ".mypy_cache",
    "target",
    "build",
];
const MAX_SEARCH_QUERY: usize = 500;
const MAX_LINE_OUTPUT: usize = 500;
const MAX_SEARCH_TOTAL_BYTES: u64 = 16 * 1024 * 1024;
const MAX_DIRECTORY_ENTRIES_SCANNED: usize = 100_000;

pub fn list(
    root: &Path,
    requested: &str,
    recursive: bool,
    max_files: usize,
) -> Result<Vec<FileEntry>> {
    if !(1..=100_000).contains(&max_files) {
        return Err(ForgeError::Safety(
            "file listing limit must be between 1 and 100000 entries.".into(),
        ));
    }
    let root = fs::canonicalize(root).map_err(|e| io_error(root, e))?;
    let start = resolve_existing(&root, requested, Expected::Directory)?;
    let start_rel = relative_label(&root, &start);
    let mut queue = VecDeque::from([(start, start_rel)]);
    let mut output = Vec::new();
    // Directory-scoped .gitignore accumulation: each entry is the relative path
    // of the directory (as "/" path) and the rules that apply to it.
    let mut ignore_stack: Vec<(String, crate::gitignore::GitignoreSet)> = Vec::new();
    while let Some((directory, rel_directory)) = queue.pop_front() {
        // Load .gitignore rules for this directory before scanning it. Rules
        // from parent directories still apply; local rules are appended so the
        // last-matching-rule-wins behavior is preserved.
        let gitignore_path = directory.join(".gitignore");
        if let Ok(contents) = fs::read_to_string(&gitignore_path) {
            let mut local = crate::gitignore::GitignoreSet::default();
            local.parse(&contents);
            if !local.is_empty() {
                // Replace any previous rules for the same directory, append new.
                ignore_stack.retain(|(p, _)| p != &rel_directory);
                ignore_stack.push((rel_directory.clone(), local));
            }
        }
        let read_dir = match fs::read_dir(&directory) {
            Ok(items) => items,
            Err(_) => continue,
        };
        // Do not collect an arbitrarily large directory into memory. Retain only the
        // lexically earliest max_files names while scanning the whole directory.
        let mut selected = BTreeSet::<(String, OsString)>::new();
        let mut scanned_entries = 0_usize;
        for item in read_dir.filter_map(std::result::Result::ok) {
            scanned_entries += 1;
            if scanned_entries > MAX_DIRECTORY_ENTRIES_SCANNED {
                return Err(ForgeError::Safety(
                    "directory scan exceeded the 100000-entry limit; narrow the directory.".into(),
                ));
            }
            let name = item.file_name();
            let name_text = name.to_string_lossy();
            if should_skip_directory(&name_text) || is_sensitive_path(&name_text) {
                continue;
            }
            // Do not let symlinks/special files crowd out ordinary source files in the capped set.
            let file_type = match item.file_type() {
                Ok(kind) => kind,
                Err(_) => continue,
            };
            if file_type.is_symlink() || (!file_type.is_file() && !file_type.is_dir()) {
                continue;
            }
            let sort_key = name_text.to_lowercase();
            selected.insert((sort_key, name));
            if selected.len() > max_files {
                selected.pop_last();
            }
        }
        let names = selected
            .into_iter()
            .map(|(_, name)| name)
            .collect::<Vec<_>>();
        for name in names {
            let name_str = name.to_string_lossy().to_string();
            if should_skip_directory(&name_str) {
                continue;
            }
            let child_rel = if rel_directory == "." {
                name_str.clone()
            } else {
                format!("{rel_directory}/{name_str}")
            };
            if is_sensitive_path(&child_rel) {
                continue;
            }
            // Apply accumulated .gitignore rules (best-effort parity). The
            // sensitive-path floor above is never bypassed by ignore rules.
            let child_is_dir_candidate = {
                let child = match resolve_existing(&root, &child_rel, Expected::Any) {
                    Ok(path) => path,
                    Err(_) => continue,
                };
                let metadata = match fs::symlink_metadata(&child) {
                    Ok(m) => m,
                    Err(_) => continue,
                };
                metadata.is_dir()
            };
            let ignored = ignore_stack
                .iter()
                .any(|(_, rules)| rules.is_ignored(&child_rel, child_is_dir_candidate));
            if ignored {
                continue;
            }
            let child = match resolve_existing(&root, &child_rel, Expected::Any) {
                Ok(path) => path,
                Err(_) => continue,
            };
            let metadata = match fs::symlink_metadata(&child) {
                Ok(m) => m,
                Err(_) => continue,
            };
            if metadata.is_dir() {
                output.push(FileEntry {
                    path: child_rel.clone(),
                    kind: "directory",
                    size_bytes: 0,
                });
                if recursive {
                    queue.push_back((child, child_rel));
                }
            } else if metadata.is_file() {
                output.push(FileEntry {
                    path: child_rel,
                    kind: "file",
                    size_bytes: metadata.len(),
                });
            }
            if output.len() >= max_files {
                return Ok(output);
            }
        }
    }
    Ok(output)
}

/// Resolve an existing, ordinary workspace file without following symlinks.
/// This is public so patch application can reuse exactly the same path policy as read-only tools.
pub fn resolve_workspace_file(root: &Path, requested: &str) -> Result<PathBuf> {
    let canonical_root = fs::canonicalize(root).map_err(|e| io_error(root, e))?;
    resolve_existing(&canonical_root, requested, Expected::File)
}

pub fn read(root: &Path, requested: &str, max_bytes: u64) -> Result<String> {
    let root = fs::canonicalize(root).map_err(|e| io_error(root, e))?;
    let path = resolve_existing(&root, requested, Expected::File)?;
    let metadata = fs::symlink_metadata(&path).map_err(|e| io_error(&path, e))?;
    if metadata.len() > max_bytes {
        return Err(ForgeError::Safety(format!(
            "file exceeds configured {}-byte read limit.",
            max_bytes
        )));
    }
    if is_binary_extension(&path) {
        return Err(ForgeError::Safety(
            "binary files are not rendered by text file tools.".into(),
        ));
    }
    reject_hardlink(&metadata, &path)?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options.open(&path).map_err(|e| io_error(&path, e))?;
    let after = file.metadata().map_err(|e| io_error(&path, e))?;
    if !after.is_file() || after.len() > max_bytes {
        return Err(ForgeError::Safety(
            "file changed during validation; refusing to read.".into(),
        ));
    }
    // Re-check the opened handle, not only the path metadata captured earlier.
    // The opened-handle check catches hard links on Unix and has a Win32 path; it does not prove all path-race defenses.
    reject_hardlink(&after, &path)?;
    crate::storage::reject_hardlink_handle(&file, &path)?;
    let mut bytes = Vec::with_capacity(after.len() as usize);
    file.take(max_bytes + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| io_error(&path, e))?;
    if bytes.len() as u64 > max_bytes {
        return Err(ForgeError::Safety(
            "file exceeded the bounded read size.".into(),
        ));
    }
    if bytes.contains(&0) {
        return Err(ForgeError::Safety(
            "binary files are not rendered by text file tools.".into(),
        ));
    }
    String::from_utf8(bytes).map_err(|_| ForgeError::Safety("file is not valid UTF-8 text.".into()))
}

pub fn search(
    root: &Path,
    query: &str,
    requested: &str,
    max_bytes: u64,
    max_files: usize,
    max_matches: usize,
) -> Result<Vec<SearchMatch>> {
    if query.is_empty() {
        return Err(ForgeError::Message(
            "search query must not be empty.".into(),
        ));
    }
    if query.len() > MAX_SEARCH_QUERY {
        return Err(ForgeError::Message(
            "search query exceeds 500 bytes.".into(),
        ));
    }
    if !(1..=10_000).contains(&max_matches) {
        return Err(ForgeError::Safety(
            "search match limit must be between 1 and 10000.".into(),
        ));
    }
    let entries = list(root, requested, true, max_files)?;
    let mut found = Vec::new();
    let mut total_bytes_scanned = 0_u64;
    for entry in entries
        .iter()
        .filter(|e| e.kind == "file" && !is_binary_extension(Path::new(&e.path)))
    {
        let content = match read(root, &entry.path, max_bytes) {
            Ok(value) => value,
            Err(_) => continue,
        };
        total_bytes_scanned = total_bytes_scanned.saturating_add(content.len() as u64);
        if total_bytes_scanned > MAX_SEARCH_TOTAL_BYTES {
            return Err(ForgeError::Safety(
                "search exceeded the 16 MiB aggregate scan limit; narrow the directory or query."
                    .into(),
            ));
        }
        for (index, line) in content.lines().enumerate() {
            if line.contains(query) {
                found.push(SearchMatch {
                    path: entry.path.clone(),
                    line: index + 1,
                    text: safe_line(line, MAX_LINE_OUTPUT),
                });
                if found.len() >= max_matches {
                    return Ok(found);
                }
            }
        }
    }
    Ok(found)
}

pub fn is_sensitive_path(requested: &str) -> bool {
    requested
        .split(['/', '\\'])
        .filter(|p| !p.is_empty())
        .any(|segment| {
            let lower = segment.to_ascii_lowercase();
            lower == ".env"
                || lower.starts_with(".env.")
                || matches!(
                    lower.as_str(),
                    ".envrc"
                        | ".forge"
                        | ".git"
                        | "node_modules"
                        | "target"
                        | "dist"
                        | "coverage"
                        | ".venv"
                        | "venv"
                        | ".ssh"
                        | ".aws"
                        | ".azure"
                        | ".gnupg"
                        | ".kube"
                        | ".npmrc"
                        | ".netrc"
                        | ".pypirc"
                        | "credentials"
                        | "credentials.json"
                        | "secrets"
                        | "secrets.json"
                        | "secrets.yaml"
                        | "secrets.yml"
                        | "id_rsa"
                        | "id_ed25519"
                        | "id_ecdsa"
                        | "id_dsa"
                        | "known_hosts"
                        | "cookies"
                        | "cookies.sqlite"
                        | "logins.json"
                        | "key4.db"
                )
                || lower.starts_with("id_rsa.")
                || lower.starts_with("id_ed25519.")
                || lower.contains("private-key")
                || lower.contains("private_key")
                || [
                    ".pem",
                    ".key",
                    ".p12",
                    ".pfx",
                    ".jks",
                    ".keystore",
                    ".mobileprovision",
                ]
                .iter()
                .any(|suffix| lower.ends_with(suffix))
        })
}

fn resolve_existing(root: &Path, requested: &str, expected: Expected) -> Result<PathBuf> {
    if requested.contains('\0') {
        return Err(ForgeError::Safety("path contains a NUL byte.".into()));
    }
    if Path::new(requested).is_absolute()
        || requested.starts_with('\\')
        || requested.as_bytes().get(1) == Some(&b':')
    {
        return Err(ForgeError::Safety(
            "absolute paths are not allowed; use a workspace-relative path.".into(),
        ));
    }
    let components = requested
        .split(['/', '\\'])
        .filter(|part| !part.is_empty() && *part != ".")
        .collect::<Vec<_>>();
    if components.contains(&"..") {
        return Err(ForgeError::Safety(
            "parent-directory traversal is not allowed.".into(),
        ));
    }
    if components.is_empty() {
        if matches!(expected, Expected::File) {
            return Err(ForgeError::Safety(
                "a regular workspace file path is required.".into(),
            ));
        }
        return Ok(root.to_path_buf());
    }
    let normalized = components.join("/");
    if is_sensitive_path(&normalized) {
        return Err(ForgeError::Safety(
            "credential or private-state paths are denied.".into(),
        ));
    }
    let mut current = root.to_path_buf();
    for (index, segment) in components.iter().enumerate() {
        if is_sensitive_path(segment) {
            return Err(ForgeError::Safety(
                "credential or private-state paths are denied.".into(),
            ));
        }
        current.push(segment);
        let meta = fs::symlink_metadata(&current).map_err(|e| io_error(&current, e))?;
        if meta.file_type().is_symlink() {
            return Err(ForgeError::Safety(
                "symlink paths are not followed by Forge.".into(),
            ));
        }
        if index + 1 < components.len() {
            if !meta.is_dir() {
                return Err(ForgeError::Safety(
                    "a parent path component is not a directory.".into(),
                ));
            }
            if should_skip_directory(segment) {
                return Err(ForgeError::Safety(
                    "private or generated directories are not readable by file tools.".into(),
                ));
            }
        }
    }
    let final_meta = fs::symlink_metadata(&current).map_err(|e| io_error(&current, e))?;
    if !(final_meta.is_file() || final_meta.is_dir()) {
        return Err(ForgeError::Safety("special files are denied.".into()));
    }
    match expected {
        Expected::File if !final_meta.is_file() => {
            return Err(ForgeError::Safety(
                "requested path is not a regular file.".into(),
            ));
        }
        Expected::Directory if !final_meta.is_dir() => {
            return Err(ForgeError::Safety(
                "requested path is not a directory.".into(),
            ));
        }
        _ => {}
    }
    reject_hardlink(&final_meta, &current)?;
    let canonical = fs::canonicalize(&current).map_err(|e| io_error(&current, e))?;
    if !canonical.starts_with(root) {
        return Err(ForgeError::Safety(
            "resolved path escapes the workspace root.".into(),
        ));
    }
    Ok(canonical)
}

#[derive(Clone, Copy)]
enum Expected {
    Any,
    File,
    Directory,
}
fn should_skip_directory(name: &str) -> bool {
    SKIP_DIRECTORIES
        .iter()
        .any(|skip| name.eq_ignore_ascii_case(skip))
}
fn is_binary_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| BINARY_EXTENSIONS.iter().any(|b| e.eq_ignore_ascii_case(b)))
        .unwrap_or(false)
}
fn relative_label(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .ok()
        .and_then(|p| p.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(".")
        .replace('\\', "/")
}
fn safe_line(value: &str, max: usize) -> String {
    let cleaned = crate::redaction::safe_terminal(&crate::redaction::redact(value));
    if cleaned.len() <= max {
        cleaned
    } else {
        format!(
            "{}…",
            &cleaned[..floor_char_boundary(&cleaned, max.saturating_sub(3))]
        )
    }
}
fn floor_char_boundary(value: &str, mut index: usize) -> usize {
    while !value.is_char_boundary(index) {
        index = index.saturating_sub(1);
    }
    index
}
fn reject_hardlink(meta: &fs::Metadata, path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if meta.is_file() && meta.nlink() > 1 {
            return Err(ForgeError::Safety(format!(
                "hard-linked files are denied: {}",
                path.display()
            )));
        }
    }
    // Windows opened-handle hard-link enforcement is handled by storage::reject_hardlink_handle
    // after a file is opened. Path metadata alone is intentionally not treated as enough.
    #[cfg(windows)]
    {
        let _ = (meta, path);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::symlink;
    use tempfile::tempdir;
    #[test]
    fn list_respects_nested_gitignore_rules() {
        let temp = tempdir().unwrap();
        fs::create_dir_all(temp.path().join("src")).unwrap();
        fs::create_dir_all(temp.path().join("src/generated")).unwrap();
        fs::write(temp.path().join("src/generated/out.rs"), "// generated\n").unwrap();
        fs::write(temp.path().join("src/main.rs"), "fn main() {}\n").unwrap();
        fs::write(temp.path().join(".gitignore"), "generated/\n").unwrap();
        fs::write(temp.path().join("debug.log"), "x\n").unwrap();
        fs::write(temp.path().join(".gitignore2"), "*.log\n").unwrap();
        // Top-level .gitignore ignores the generated/ directory.
        let entries = list(temp.path(), ".", true, 100).unwrap();
        assert!(!entries.iter().any(|e| e.path.contains("generated")));
        assert!(entries.iter().any(|e| e.path == "src/main.rs"));
        assert!(entries.iter().any(|e| e.path == "src"));
        // Debug.log is NOT ignored at top level (no *.log rule at top level).
        assert!(entries.iter().any(|e| e.path == "debug.log"));
    }

    #[test]
    fn list_respects_nested_gitignore_in_subdirectory() {
        let temp = tempdir().unwrap();
        fs::create_dir_all(temp.path().join("pkg/cache")).unwrap();
        fs::write(temp.path().join("pkg/cache/data.tmp"), "x\n").unwrap();
        fs::write(temp.path().join("pkg/cache/keep.rs"), "fn keep() {}\n").unwrap();
        fs::write(temp.path().join("pkg/.gitignore"), "cache/\n*.tmp\n").unwrap();
        fs::write(temp.path().join("top.txt"), "ok\n").unwrap();
        let entries = list(temp.path(), ".", true, 100).unwrap();
        assert!(!entries.iter().any(|e| e.path.contains("cache")));
        assert!(!entries.iter().any(|e| e.path.ends_with(".tmp")));
        assert!(entries.iter().any(|e| e.path == "top.txt"));
    }

    #[test]
    fn denies_traversal_and_sensitive_paths() {
        assert!(resolve_existing(Path::new("/tmp"), "../etc/passwd", Expected::File).is_err());
        assert!(resolve_existing(Path::new("/tmp"), ".env", Expected::File).is_err());
        assert!(resolve_existing(Path::new("/tmp"), ".forge/audit.jsonl", Expected::File).is_err());
        assert!(is_sensitive_path("private.pem"));
        assert!(is_sensitive_path("certs/client.p12"));
        assert!(is_sensitive_path("certs/client.pfx"));
        assert!(is_sensitive_path("certs/private.key"));
        assert!(is_sensitive_path("app/mobile.mobileprovision"));
        assert!(is_sensitive_path(".envrc"));
    }
    #[test]
    fn list_read_search_work_and_hide_env_files() {
        let temp = tempdir().unwrap();
        fs::create_dir(temp.path().join("src")).unwrap();
        fs::write(temp.path().join("src/main.rs"), "fn main() {}\n// needle\n").unwrap();
        fs::write(temp.path().join(".env"), "SECRET=do-not-read").unwrap();
        assert!(read(temp.path(), ".env", 4096).is_err());
        assert!(
            read(temp.path(), "src/main.rs", 4096)
                .unwrap()
                .contains("fn main")
        );
        assert!(
            list(temp.path(), ".", true, 100)
                .unwrap()
                .iter()
                .all(|e| !e.path.contains(".env"))
        );
        assert_eq!(
            search(temp.path(), "needle", ".", 4096, 100, 10)
                .unwrap()
                .len(),
            1
        );
    }
    #[test]
    fn search_previews_redact_credentials_before_terminal_display() {
        let temp = tempdir().unwrap();
        fs::write(
            temp.path().join("notes.txt"),
            "api_key=search-secret-value-123\n",
        )
        .unwrap();
        let found = search(temp.path(), "api_key", ".", 4096, 100, 10).unwrap();
        assert_eq!(found.len(), 1);
        assert!(!found[0].text.contains("search-secret-value-123"));
        assert!(found[0].text.contains("[REDACTED]"));
    }

    #[test]
    fn rejects_oversized_non_utf8_and_nul_containing_text_reads() {
        let workspace = tempdir().unwrap();
        fs::write(workspace.path().join("large.txt"), "12345").unwrap();
        fs::write(workspace.path().join("invalid.txt"), [0xff, 0xfe]).unwrap();
        fs::write(workspace.path().join("nul.txt"), b"before\0after").unwrap();
        assert!(read(workspace.path(), "large.txt", 4).is_err());
        assert!(read(workspace.path(), "invalid.txt", 4096).is_err());
        assert!(read(workspace.path(), "nul.txt", 4096).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinked_workspace_file() {
        use std::os::unix::fs::symlink;
        let workspace = tempdir().unwrap();
        let outside = tempdir().unwrap();
        fs::write(outside.path().join("secret.txt"), "outside-secret").unwrap();
        symlink(
            outside.path().join("secret.txt"),
            workspace.path().join("alias.txt"),
        )
        .unwrap();
        assert!(read(workspace.path(), "alias.txt", 4096).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn refuses_hardlinked_workspace_files() {
        let workspace = tempdir().unwrap();
        let outside = tempdir().unwrap();
        let external = outside.path().join("secret.txt");
        fs::write(&external, "external-secret-value").unwrap();
        fs::hard_link(&external, workspace.path().join("alias.txt")).unwrap();
        assert!(read(workspace.path(), "alias.txt", 4096).is_err());
        assert_eq!(
            fs::read_to_string(&external).unwrap(),
            "external-secret-value"
        );
    }

    #[test]
    fn rejects_zero_limits_instead_of_returning_misleading_empty_results() {
        let temp = tempdir().unwrap();
        assert!(list(temp.path(), ".", true, 0).is_err());
        assert!(search(temp.path(), "x", ".", 4096, 10, 0).is_err());
    }
    #[test]
    fn list_is_case_insensitive_sorted_and_bounded() {
        let temp = tempdir().unwrap();
        for name in ["z.rs", "B.rs", "a.rs", "C.rs"] {
            fs::write(temp.path().join(name), "ok").unwrap();
        }
        let found = list(temp.path(), ".", false, 2).unwrap();
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].path, "a.rs");
        assert_eq!(found[1].path, "B.rs");
    }
    #[cfg(unix)]
    #[test]
    fn does_not_follow_symlink_outside_workspace() {
        let temp = tempdir().unwrap();
        let outside = tempdir().unwrap();
        fs::write(outside.path().join("secret.txt"), "outside").unwrap();
        symlink(
            outside.path().join("secret.txt"),
            temp.path().join("escape.txt"),
        )
        .unwrap();
        assert!(read(temp.path(), "escape.txt", 4096).is_err());
    }
}
