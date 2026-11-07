//! Shared storage primitives for Do Code's local project state.
//! These routines fail closed on symlinks and publish same-directory temporary files.
use crate::error::{DocodeError, Result, io_error};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};
use uuid::Uuid;

pub const MAX_STATE_FILE_BYTES: u64 = 20 * 1024 * 1024;

pub fn timestamp() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_owned())
}

pub fn sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn ensure_private_dir(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() || !meta.is_dir() => {
            return Err(DocodeError::Safety(format!(
                "state directory '{}' must be a real directory, not a symlink or special file.",
                path.display()
            )));
        }
        Ok(_) => {
            // Directory exists: still enforce private permissions.
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(path)
                .or_else(|create_error| {
                    if create_error.kind() == std::io::ErrorKind::AlreadyExists {
                        Ok(())
                    } else {
                        Err(create_error)
                    }
                })
                .map_err(|e| io_error(path, e))?;
            let meta = fs::symlink_metadata(path).map_err(|e| io_error(path, e))?;
            if meta.file_type().is_symlink() || !meta.is_dir() {
                return Err(DocodeError::Safety(format!(
                    "state directory '{}' changed during creation.",
                    path.display()
                )));
            }
        }
        Err(error) => return Err(io_error(path, error)),
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|e| io_error(path, e))?;
    }
    #[cfg(windows)]
    {
        crate::config::set_private_permissions(path)?;
    }
    Ok(())
}

pub fn ensure_project_home(root: &Path) -> Result<PathBuf> {
    let canonical = fs::canonicalize(root).map_err(|e| io_error(root, e))?;
    let home = canonical.join(".docode");
    ensure_private_dir(&home)?;
    for name in [
        "missions",
        "locks",
        "patches",
        "checkpoints",
        "evidence",
        "memory",
        "index",
        "decisions",
        "transcripts",
    ] {
        ensure_private_dir(&home.join(name))?;
    }
    Ok(home)
}

pub fn project_id(root: &Path) -> Result<String> {
    let home = ensure_project_home(root)?;
    let path = home.join("project-id");
    match read_bounded_regular(&path, 128) {
        Ok(value) => {
            let text = std::str::from_utf8(&value).map_err(|_| {
                DocodeError::Safety(
                    ".docode/project-id is invalid; refusing to rebind existing state.".into(),
                )
            })?;
            let id = text.trim();
            let parsed = Uuid::parse_str(id).map_err(|_| {
                DocodeError::Safety(
                    ".docode/project-id is invalid; refusing to rebind existing state.".into(),
                )
            })?;
            if parsed.to_string() != id {
                return Err(DocodeError::Safety(
                    ".docode/project-id is not canonical; refusing to rebind existing state."
                        .into(),
                ));
            }
            Ok(id.to_owned())
        }
        Err(DocodeError::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
            let id = Uuid::new_v4().to_string();
            match write_atomic(&path, format!("{id}\n").as_bytes(), false) {
                Ok(()) => Ok(id),
                Err(_) => {
                    let value = read_bounded_regular(&path, 128)?;
                    let text = std::str::from_utf8(&value).map_err(|_| {
                        DocodeError::Safety(
                            "project ID publication raced with invalid state.".into(),
                        )
                    })?;
                    let existing = text.trim();
                    let parsed = Uuid::parse_str(existing).map_err(|_| {
                        DocodeError::Safety(
                            "project ID publication raced with invalid state.".into(),
                        )
                    })?;
                    if parsed.to_string() != existing {
                        return Err(DocodeError::Safety(
                            "project ID publication raced with non-canonical state.".into(),
                        ));
                    }
                    Ok(existing.to_owned())
                }
            }
        }
        Err(error) => Err(error),
    }
}

pub fn read_bounded_regular(path: &Path, max_bytes: u64) -> Result<Vec<u8>> {
    let before = fs::symlink_metadata(path).map_err(|e| io_error(path, e))?;
    if before.file_type().is_symlink() || !before.is_file() || before.len() > max_bytes {
        return Err(DocodeError::Safety(format!(
            "'{}' is not a bounded regular file.",
            path.display()
        )));
    }
    reject_hardlink(&before, path)?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options.open(path).map_err(|e| io_error(path, e))?;
    let opened = file.metadata().map_err(|e| io_error(path, e))?;
    if !opened.is_file() || opened.len() > max_bytes {
        return Err(DocodeError::Safety(
            "opened state file changed or exceeds its size limit.".into(),
        ));
    }
    reject_hardlink(&opened, path)?;
    reject_hardlink_handle(&file, path)?;
    let mut bytes = Vec::with_capacity(opened.len() as usize);
    file.take(max_bytes + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| io_error(path, e))?;
    if bytes.len() as u64 > max_bytes {
        return Err(DocodeError::Safety(
            "state file exceeded its bounded read size.".into(),
        ));
    }
    Ok(bytes)
}

pub fn write_atomic(path: &Path, bytes: &[u8], replace: bool) -> Result<()> {
    write_atomic_with_mode(path, bytes, replace, 0o600)
}

pub fn write_atomic_with_mode(path: &Path, bytes: &[u8], replace: bool, mode: u32) -> Result<()> {
    #[cfg(not(unix))]
    let _ = mode;
    if bytes.len() as u64 > MAX_STATE_FILE_BYTES {
        return Err(DocodeError::Safety(
            "state write exceeds the 20 MiB limit.".into(),
        ));
    }
    let parent = path
        .parent()
        .ok_or_else(|| DocodeError::Safety("state file path has no parent directory.".into()))?;
    let parent_meta = fs::symlink_metadata(parent).map_err(|e| io_error(parent, e))?;
    if parent_meta.file_type().is_symlink() || !parent_meta.is_dir() {
        return Err(DocodeError::Safety(
            "state file parent must be a real directory, not a symlink or special file.".into(),
        ));
    }
    if path.exists() {
        let meta = fs::symlink_metadata(path).map_err(|e| io_error(path, e))?;
        if meta.file_type().is_symlink() || !meta.is_file() {
            return Err(DocodeError::Safety(
                "refusing to replace a symlink or special state file.".into(),
            ));
        }
        reject_hardlink(&meta, path)?;
        if !replace {
            return Err(DocodeError::Safety(format!(
                "state file '{}' already exists; refusing to overwrite it.",
                path.display()
            )));
        }
    }
    let temp = parent.join(format!(".docode-tmp-{}.tmp", Uuid::new_v4()));
    let result = (|| -> Result<()> {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temp).map_err(|e| io_error(&temp, e))?;
        file.write_all(bytes).map_err(|e| io_error(&temp, e))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(fs::Permissions::from_mode(mode & 0o777))
                .map_err(|e| io_error(&temp, e))?;
        }
        file.sync_all().map_err(|e| io_error(&temp, e))?;
        drop(file);
        if replace {
            atomic_replace(&temp, path)?;
        } else {
            fs::hard_link(&temp, path).map_err(|e| io_error(path, e))?;
            fs::remove_file(&temp).map_err(|e| io_error(&temp, e))?;
        }
        #[cfg(unix)]
        if let Ok(dir) = File::open(parent) {
            let _ = dir.sync_all();
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

fn atomic_replace(from: &Path, to: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        fs::rename(from, to).map_err(|e| io_error(to, e))
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{
            MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
        };
        let from_wide: Vec<u16> = from
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let to_wide: Vec<u16> = to
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        // SAFETY: both paths are valid NUL-terminated UTF-16 strings alive for the duration of the call.
        let ok = unsafe {
            MoveFileExW(
                from_wide.as_ptr(),
                to_wide.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        };
        if ok == 0 {
            return Err(io_error(to, std::io::Error::last_os_error()));
        }
        Ok(())
    }
    #[cfg(not(any(unix, windows)))]
    {
        fs::rename(from, to).map_err(|e| io_error(to, e))
    }
}

/// Reject an opened file whose inode/file record has more than one hard link.
/// On Windows this uses the stable Win32 handle API because stable `std::fs::Metadata`
/// does not expose the link count. A failure to query the count fails closed.
pub fn reject_hardlink_handle(file: &File, path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let metadata = file.metadata().map_err(|e| io_error(path, e))?;
        if metadata.is_file() && metadata.nlink() > 1 {
            return Err(DocodeError::Safety(format!(
                "hard-linked file '{}' is denied.",
                path.display()
            )));
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{
            BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
        };
        let mut info = std::mem::MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::uninit();
        // SAFETY: the file owns a valid handle for the duration of this synchronous call,
        // and `info` points to writable storage for the documented output structure.
        let ok = unsafe { GetFileInformationByHandle(file.as_raw_handle(), info.as_mut_ptr()) };
        if ok == 0 {
            return Err(io_error(path, std::io::Error::last_os_error()));
        }
        // SAFETY: a nonzero return indicates that Windows filled the information structure.
        let info = unsafe { info.assume_init() };
        if info.nNumberOfLinks > 1 {
            return Err(DocodeError::Safety(format!(
                "hard-linked file '{}' is denied.",
                path.display()
            )));
        }
    }
    Ok(())
}

fn reject_hardlink(meta: &fs::Metadata, path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if meta.nlink() > 1 {
            return Err(DocodeError::Safety(format!(
                "hard-linked state file '{}' is denied.",
                path.display()
            )));
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (meta, path);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn project_identity_is_stable_and_state_directory_is_private_on_unix() {
        let root = tempdir().unwrap();
        let first = project_id(root.path()).unwrap();
        let second = project_id(root.path()).unwrap();
        assert_eq!(first, second);
        assert_eq!(Uuid::parse_str(&first).unwrap().to_string(), first);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(root.path().join(".docode"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o700);
        }
    }

    #[test]
    fn atomic_write_refuses_overwrite_when_not_requested_and_replaces_when_approved() {
        let root = tempdir().unwrap();
        let home = ensure_project_home(root.path()).unwrap();
        let target = home.join("sample.json");
        write_atomic(&target, b"first", false).unwrap();
        assert!(write_atomic(&target, b"second", false).is_err());
        assert_eq!(read_bounded_regular(&target, 64).unwrap(), b"first");
        write_atomic(&target, b"second", true).unwrap();
        assert_eq!(read_bounded_regular(&target, 64).unwrap(), b"second");
    }

    #[test]
    fn bounded_reader_rejects_oversize_and_symlink_files() {
        let root = tempdir().unwrap();
        let target = root.path().join("data.txt");
        fs::write(&target, b"0123456789").unwrap();
        assert!(read_bounded_regular(&target, 4).is_err());
        #[cfg(unix)]
        {
            let alias = root.path().join("alias.txt");
            std::os::unix::fs::symlink(&target, &alias).unwrap();
            assert!(read_bounded_regular(&alias, 64).is_err());
        }
    }

    #[test]
    fn hardlinked_state_file_is_rejected_on_unix() {
        #[cfg(unix)]
        {
            let root = tempdir().unwrap();
            let source = root.path().join("source.txt");
            let alias = root.path().join("alias.txt");
            fs::write(&source, b"same inode").unwrap();
            fs::hard_link(&source, &alias).unwrap();
            assert!(read_bounded_regular(&alias, 64).is_err());
        }
    }
}

#[cfg(all(test, windows))]
mod windows_hardlink_tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn opened_handle_rejects_hardlinked_state_file_on_windows() {
        let root = tempdir().unwrap();
        let source = root.path().join("source.txt");
        let alias = root.path().join("alias.txt");
        fs::write(&source, b"same file record").unwrap();
        fs::hard_link(&source, &alias).unwrap();
        assert!(read_bounded_regular(&alias, 64).is_err());
    }
}
