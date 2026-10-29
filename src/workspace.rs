//! Code-over-API agent workspace.
//!
//! Agents work like developers: they produce real file contents for real
//! paths inside the workspace, not a scatter of bespoke tool calls. Forge
//! records every proposed mutation in a journal — mutation id, mission,
//! kind, path, before/after hashes, checkpoint reference, reversibility —
//! so the whole session is auditable and every applied change is undoable.
//! The journal is append-only JSONL bound to the mission; the patch applier
//! remains the only writer, so the workspace never mutates files directly.

use crate::error::{ForgeError, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkspaceMutation {
    pub mutation_id: String,
    pub mission_id: String,
    pub agent: String,
    pub kind: String,
    pub path: String,
    pub before_hash: String,
    pub after_hash: String,
    pub checkpoint: String,
    pub reversible: bool,
    pub recorded_at: String,
}

/// Open (or resume) the mutation journal for a mission.
pub struct MutationJournal {
    path: std::path::PathBuf,
    count: usize,
}

impl MutationJournal {
    pub fn for_mission(root: &Path, mission_id: &str) -> Result<Self> {
        let base = crate::storage::ensure_project_home(root)?;
        let dir = base.join("missions").join(mission_id);
        crate::storage::ensure_private_dir(&dir)?;
        Ok(Self {
            path: dir.join("mutations.jsonl"),
            count: 0,
        })
    }

    /// Append one mutation record. The record is integrity-bound: the
    /// caller must supply the real before/after SHA-256 of the file bytes.
    // Each argument is a distinct ledger field; grouping them would hide
    // which hashes are required from the caller.
    #[allow(clippy::too_many_arguments)]
    pub fn record(
        &mut self,
        mission_id: &str,
        agent: &str,
        kind: &str,
        path: &str,
        before_hash: &str,
        after_hash: &str,
        checkpoint: &str,
        reversible: bool,
    ) -> Result<WorkspaceMutation> {
        if before_hash == after_hash {
            return Err(ForgeError::Safety(
                "workspace mutation rejected: before and after hashes are identical; nothing changed."
                    .into(),
            ));
        }
        let mutation = WorkspaceMutation {
            mutation_id: format!("mut_{:04}", self.count + 1),
            mission_id: mission_id.to_string(),
            agent: agent.to_string(),
            kind: kind.to_string(),
            path: path.to_string(),
            before_hash: before_hash.to_string(),
            after_hash: after_hash.to_string(),
            checkpoint: checkpoint.to_string(),
            reversible,
            recorded_at: crate::storage::timestamp(),
        };
        let mut line =
            serde_json::to_vec(&mutation).map_err(|e| ForgeError::Message(e.to_string()))?;
        line.push(b'\n');
        self.append(line)?;
        self.count += 1;
        Ok(mutation)
    }

    fn append(&self, line: Vec<u8>) -> Result<()> {
        use std::{fs::OpenOptions, io::Write};
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|e| crate::error::io_error(&self.path, e))?;
        crate::storage::reject_hardlink_handle(&file, &self.path)?;
        file.write_all(&line)
            .map_err(|e| crate::error::io_error(&self.path, e))?;
        Ok(())
    }

    pub fn read_all(&self) -> Result<Vec<WorkspaceMutation>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let bytes = crate::storage::read_bounded_regular(&self.path, 8 * 1024 * 1024)?;
        let mut out = Vec::new();
        for line in bytes.split(|b| *b == b'\n') {
            if line.is_empty() {
                continue;
            }
            out.push(
                serde_json::from_slice(line)
                    .map_err(|e| ForgeError::Message(format!("corrupt mutation journal: {e}")))?,
            );
        }
        Ok(out)
    }
}

/// Describe the workspace contract shown to the agent: what it may write,
/// how it works, and what Forge records. This is the "workspace-style
/// interface" — files in, real code out, everything journaled.
pub fn workspace_brief(allowed_paths: &[String]) -> String {
    format!(
        "Workspace contract: you work by writing complete, real file contents (code-over-API). \
Paths allowed in this mission: {}. \
Forge records every mutation (before/after hashes, checkpoint, rollback) and refuses any \
write outside this list. Do not claim tests ran; Forge executes verification separately.",
        if allowed_paths.is_empty() {
            "(none — nothing may be written)".to_string()
        } else {
            allowed_paths.join(", ")
        }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn journal_records_and_rereads_mutations() {
        let root = tempfile::tempdir().unwrap();
        let mut journal = MutationJournal::for_mission(root.path(), "msn_1").unwrap();
        let m = journal
            .record(
                "msn_1",
                "coder",
                "file_edit",
                "src/a.rs",
                "aaa",
                "bbb",
                "cp_1",
                true,
            )
            .unwrap();
        assert_eq!(m.mutation_id, "mut_0001");
        let all = journal.read_all().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].path, "src/a.rs");
        assert!(all[0].reversible);
    }

    #[test]
    fn identical_hashes_are_rejected() {
        let root = tempfile::tempdir().unwrap();
        let mut journal = MutationJournal::for_mission(root.path(), "msn_1").unwrap();
        let err = journal
            .record(
                "msn_1",
                "coder",
                "file_edit",
                "src/a.rs",
                "same",
                "same",
                "cp",
                true,
            )
            .unwrap_err();
        assert!(err.to_string().contains("nothing changed"));
    }

    #[test]
    fn mutation_ids_increment_across_appends() {
        let root = tempfile::tempdir().unwrap();
        let mut journal = MutationJournal::for_mission(root.path(), "msn_1").unwrap();
        journal
            .record("msn_1", "c", "file_edit", "p", "a", "b", "cp", true)
            .unwrap();
        let m2 = journal
            .record("msn_1", "c", "file_edit", "q", "c", "d", "cp", true)
            .unwrap();
        assert_eq!(m2.mutation_id, "mut_0002");
        assert_eq!(journal.read_all().unwrap().len(), 2);
    }

    #[test]
    fn empty_journal_reads_as_empty() {
        let root = tempfile::tempdir().unwrap();
        let journal = MutationJournal::for_mission(root.path(), "msn_1").unwrap();
        assert!(journal.read_all().unwrap().is_empty());
    }

    #[test]
    fn workspace_brief_lists_allowed_paths() {
        let brief = workspace_brief(&["src/a.rs".into(), "src/b.rs".into()]);
        assert!(brief.contains("src/a.rs, src/b.rs"));
        assert!(brief.contains("Do not claim tests ran"));
        let empty = workspace_brief(&[]);
        assert!(empty.contains("nothing may be written"));
    }
}
