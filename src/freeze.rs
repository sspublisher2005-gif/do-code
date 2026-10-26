//! Explicit emergency freeze gate for new model requests and patch writes.
use crate::{
    error::{ForgeError, Result, io_error},
    storage,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FreezeState {
    pub schema_version: u8,
    pub root: String,
    pub frozen: bool,
    pub reason: String,
    pub updated_at: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Envelope {
    schema_version: u8,
    sha256: String,
    state: FreezeState,
}

pub fn ensure_not_frozen(root: &Path, action: &str) -> Result<()> {
    let Some(state) = load(root)? else {
        return Ok(());
    };
    if state.frozen {
        return Err(ForgeError::Safety(format!(
            "FORGE_FROZEN: {action} is blocked. Reason: {}. Use 'forge unfreeze --approve' after reviewing the risk.",
            state.reason
        )));
    }
    Ok(())
}

pub fn status(root: &Path) -> Result<Option<FreezeState>> {
    load(root)
}

pub fn freeze(root: &Path, reason: &str) -> Result<FreezeState> {
    let root = fs::canonicalize(root).map_err(|e| io_error(root, e))?;
    let reason = reason.trim();
    if reason.len() < 3 || reason.len() > 2_000 {
        return Err(ForgeError::Message(
            "freeze reason must be 3..2000 bytes.".into(),
        ));
    }
    if crate::redaction::redact(reason) != reason {
        return Err(ForgeError::Safety(
            "freeze reason appears to contain a credential.".into(),
        ));
    }
    let state = FreezeState {
        schema_version: 1,
        root: root.to_string_lossy().into_owned(),
        frozen: true,
        reason: crate::redaction::safe_terminal(reason),
        updated_at: storage::timestamp(),
    };
    save(&root, &state)?;
    Ok(state)
}

pub fn unfreeze(root: &Path, approved: bool) -> Result<FreezeState> {
    if !approved {
        return Err(ForgeError::Safety(
            "unfreezing requires explicit --approve. No change was made.".into(),
        ));
    }
    let root = fs::canonicalize(root).map_err(|e| io_error(root, e))?;
    let mut state = load(&root)?
        .ok_or_else(|| ForgeError::Message("Forge is not frozen; no change was needed.".into()))?;
    state.frozen = false;
    state.updated_at = storage::timestamp();
    save(&root, &state)?;
    Ok(state)
}

fn load(root: &Path) -> Result<Option<FreezeState>> {
    let root = fs::canonicalize(root).map_err(|e| io_error(root, e))?;
    let home = storage::ensure_project_home(&root)?;
    let path = home.join("freeze.json");
    let bytes = match storage::read_bounded_regular(&path, 64 * 1024) {
        Ok(bytes) => bytes,
        Err(ForgeError::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
            return Ok(None);
        }
        Err(error) => return Err(error),
    };
    let envelope: Envelope = serde_json::from_slice(&bytes).map_err(|_| ForgeError::Safety("freeze record is corrupt; live requests and patch application remain blocked until repaired.".into()))?;
    let payload =
        serde_json::to_vec(&envelope.state).map_err(|e| ForgeError::Message(e.to_string()))?;
    if envelope.schema_version != 1
        || envelope.state.schema_version != 1
        || hash(&payload) != envelope.sha256
        || envelope.state.root != root.to_string_lossy()
    {
        return Err(ForgeError::Safety("freeze record integrity or project binding failed; live requests and patch application remain blocked.".into()));
    }
    Ok(Some(envelope.state))
}
fn save(root: &Path, state: &FreezeState) -> Result<()> {
    let home = storage::ensure_project_home(root)?;
    let payload = serde_json::to_vec(state).map_err(|e| ForgeError::Message(e.to_string()))?;
    let envelope = Envelope {
        schema_version: 1,
        sha256: hash(&payload),
        state: state.clone(),
    };
    let bytes = serde_json::to_vec(&envelope).map_err(|e| ForgeError::Message(e.to_string()))?;
    let path = home.join("freeze.json");
    storage::write_atomic(&path, &bytes, path.exists())
}
fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    #[test]
    fn freeze_blocks_until_explicitly_unfrozen() {
        let dir = tempdir().unwrap();
        assert!(ensure_not_frozen(dir.path(), "model request").is_ok());
        freeze(dir.path(), "incident investigation").unwrap();
        assert!(ensure_not_frozen(dir.path(), "model request").is_err());
        assert!(unfreeze(dir.path(), false).is_err());
        unfreeze(dir.path(), true).unwrap();
        assert!(ensure_not_frozen(dir.path(), "model request").is_ok());
    }
}
