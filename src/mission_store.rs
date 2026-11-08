//! Hash-bound, project-bound local mission persistence.
use crate::{
    error::{DocodeError, Result, io_error},
    mission::{
        self, AcceptanceCriterion, AgentRole, Capability, Mission, MissionStatus, Step, StepStatus,
        VerificationKind,
    },
    storage,
};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
};
use uuid::Uuid;

const STORE_VERSION: u8 = 1;
const MAX_MISSION_BYTES: u64 = 8 * 1024 * 1024;
const MAX_OBJECTIVE_BYTES: usize = 20_000;

#[derive(Clone, Debug, Serialize, Deserialize)]
struct MissionEnvelope {
    store_version: u8,
    sha256: String,
    mission: Mission,
}

pub struct MissionStore {
    root: PathBuf,
    home: PathBuf,
    project_id: String,
}

impl MissionStore {
    pub fn open(root: &Path) -> Result<Self> {
        let root = fs::canonicalize(root).map_err(|e| io_error(root, e))?;
        let home = storage::ensure_project_home(&root)?;
        let project_id = storage::project_id(&root)?;
        Ok(Self {
            root,
            home,
            project_id,
        })
    }

    pub fn create(&self, objective: &str) -> Result<Mission> {
        let objective = objective.trim();
        if objective.len() < 8 || objective.len() > MAX_OBJECTIVE_BYTES {
            return Err(DocodeError::Message(
                "task description must be 8..20000 bytes.".into(),
            ));
        }
        if crate::redaction::redact(objective) != objective {
            return Err(DocodeError::Safety("task description appears to contain a credential. Remove it before creating a mission.".into()));
        }
        let now = storage::timestamp();
        let id = Uuid::new_v4().to_string();
        let defs: [(&str, AgentRole); 7] = [
            ("research", AgentRole::Researcher),
            ("plan", AgentRole::Architect),
            ("implement", AgentRole::Coder),
            ("test", AgentRole::Tester),
            ("review", AgentRole::Reviewer),
            ("security", AgentRole::Security),
            ("devops", AgentRole::Devops),
        ];
        let mut steps = Vec::with_capacity(defs.len());
        let mut previous: Option<String> = None;
        for (ordinal, (kind, role)) in defs.iter().enumerate() {
            let step_id = Uuid::new_v4().to_string();
            steps.push(Step {
                id: step_id.clone(),
                mission_id: id.clone(),
                ordinal: ordinal as u32,
                kind: (*kind).to_owned(),
                assigned_role: *role,
                status: StepStatus::Pending,
                depends_on: previous.iter().cloned().collect(),
                attempt: 0,
                input_evidence_ids: Vec::new(),
                output_evidence_ids: Vec::new(),
                tool_call_ids: Vec::new(),
                created_at: now.clone(),
                started_at: None,
                finished_at: None,
                failure_code: None,
            });
            previous = Some(step_id);
        }
        let mission = Mission {
            schema_version: 1, id, project_id: self.project_id.clone(), repo_root: self.root.to_string_lossy().into_owned(),
            objective: objective.to_owned(), status: MissionStatus::Draft, created_at: now.clone(), updated_at: now,
            steps,
            acceptance_criteria: vec![AcceptanceCriterion { id: Uuid::new_v4().to_string(),
                description: "Required checks must actually run and pass with attached evidence before completion.".into(),
                required: true, verification_kinds: vec![VerificationKind::Test, VerificationKind::Review, VerificationKind::Security] }],
            max_duration_ms: 30 * 60 * 1000, dry_run: true,
            approved_capabilities: vec![Capability::ReadWorkspace], evidence_ids: Vec::new(),
        };
        self.validate(&mission)?;
        self.write(&mission, false)?;
        Ok(mission)
    }

    pub fn get(&self, id: &str) -> Result<Mission> {
        let path = self.mission_path(id)?;
        // Unknown missions are a normal lookup miss, not an I/O fault: callers
        // (and the acceptance matrix) must see why completion was refused.
        if !path.is_file() {
            return Err(DocodeError::Message(format!(
                "mission {id} does not exist in this project."
            )));
        }
        let bytes = storage::read_bounded_regular(&path, MAX_MISSION_BYTES)?;
        let envelope: MissionEnvelope = serde_json::from_slice(&bytes).map_err(|_| {
            DocodeError::Safety("mission state is not valid JSON; it was not modified.".into())
        })?;
        if envelope.store_version != STORE_VERSION || !is_sha256(&envelope.sha256) {
            return Err(DocodeError::Safety(
                "mission state envelope version or checksum field is invalid.".into(),
            ));
        }
        let canonical = serde_json::to_vec(&envelope.mission)
            .map_err(|e| DocodeError::Message(e.to_string()))?;
        let digest = Sha256::digest(&canonical)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        if digest != envelope.sha256 {
            return Err(DocodeError::Safety(
                "mission state checksum mismatch; no state was modified.".into(),
            ));
        }
        if envelope.mission.id != id {
            return Err(DocodeError::Safety(
                "mission filename and payload identity do not match.".into(),
            ));
        }
        self.validate(&envelope.mission)?;
        Ok(envelope.mission)
    }

    pub fn list(&self) -> Result<Vec<Mission>> {
        let dir = self.home.join("missions");
        let mut ids = Vec::new();
        let entries = match fs::read_dir(&dir) {
            Ok(e) => e,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(err) => return Err(io_error(&dir, err)),
        };
        for entry in entries {
            let entry = entry.map_err(|e| io_error(&dir, e))?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.ends_with(".json") {
                let id = name.trim_end_matches(".json");
                if Uuid::parse_str(id).is_ok() {
                    ids.push(id.to_owned());
                }
            }
        }
        ids.sort();
        ids.iter().map(|id| self.get(id)).collect()
    }

    /// Recover from crashes and interrupted operations.
    /// Cleans stale locks, leftover temp files, and validates all missions.
    /// Returns a list of (mission, next_safe_action) pairs.
    /// Never re-runs destructive actions; only reports state.
    pub fn recover(&self) -> Result<Vec<(Mission, String)>> {
        // 1. Clean stale locks (lock files held by dead processes).
        let lock_dir = self.home.join("locks");
        if let Ok(entries) = fs::read_dir(&lock_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "lock") {
                    // Try to acquire the lock; if we can, the previous holder is dead.
                    if let Ok(file) = OpenOptions::new().read(true).write(true).open(&path) {
                        // If lock_exclusive succeeds immediately, the lock was stale.
                        if FileExt::try_lock_exclusive(&file).is_ok() {
                            let _ = FileExt::unlock(&file);
                            let _ = fs::remove_file(&path);
                        }
                    }
                }
            }
        }

        // 2. Clean leftover temp files from interrupted atomic writes.
        let mission_dir = self.home.join("missions");
        if let Ok(entries) = fs::read_dir(&mission_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if name.starts_with('.') && name.ends_with(".tmp") {
                    let _ = fs::remove_file(entry.path());
                }
            }
        }

        // 3. Validate all missions and compute next safe action.
        let missions = self.list()?;
        let mut result = Vec::with_capacity(missions.len());
        for mission in missions {
            let action = match mission.status {
                MissionStatus::Draft => "Plan has not started. Run 'docode run' to begin.",
                MissionStatus::AwaitingApproval => {
                    "Awaiting approval. Review the proposed patch, then 'docode apply --approve'."
                }
                MissionStatus::Ready => "Ready to execute. Run 'docode run' to proceed.",
                MissionStatus::Running => {
                    "Interrupted while running. Review steps before resuming."
                }
                MissionStatus::Verifying => "Verifying. Run 'docode verify' to check evidence.",
                MissionStatus::NeedsReview => {
                    "Awaiting review. Run 'docode verify' to check evidence."
                }
                MissionStatus::Completed => "Mission completed. Run 'docode verify' to confirm.",
                MissionStatus::Failed => {
                    "Mission failed. Review failure codes, then retry or undo."
                }
                MissionStatus::Interrupted => {
                    "Mission interrupted. Review state, then retry or undo."
                }
                MissionStatus::Cancelled => "Mission cancelled. No further action needed.",
            };
            // Refine based on step states.
            let next_step = mission
                .steps
                .iter()
                .find(|s| matches!(s.status, StepStatus::Pending | StepStatus::Running));
            let action = if let Some(step) = next_step {
                format!("{} Next step: {} ({:?}).", action, step.kind, step.status)
            } else {
                action.to_owned()
            };
            result.push((mission, action));
        }
        Ok(result)
    }

    pub fn transition(&self, id: &str, next: MissionStatus) -> Result<Mission> {
        self.update(id, |mission| {
            if !mission::mission_transitions(&mission.status, &next) {
                return Err(DocodeError::Message(format!(
                    "mission transition {:?} -> {:?} is not allowed.",
                    mission.status, next
                )));
            }
            mission.status = next;
            Ok(())
        })
    }

    pub fn update<F>(&self, id: &str, mutate: F) -> Result<Mission>
    where
        F: FnOnce(&mut Mission) -> Result<()>,
    {
        let _lock = self.lock(id)?;
        let mut current = self.get(id)?;
        mutate(&mut current)?;
        if current.id != id
            || current.project_id != self.project_id
            || current.repo_root != self.root.to_string_lossy()
        {
            return Err(DocodeError::Safety(
                "mission identity/project binding cannot be changed.".into(),
            ));
        }
        current.updated_at = storage::timestamp();
        self.validate(&current)?;
        self.write(&current, true)?;
        Ok(current)
    }

    // KNOWN GAP: public accessor for a mission's on-disk path; not yet called by any CLI command.
    #[allow(dead_code)]
    pub fn path(&self, id: &str) -> Result<PathBuf> {
        self.mission_path(id)
    }

    fn mission_path(&self, id: &str) -> Result<PathBuf> {
        let parsed = Uuid::parse_str(id).map_err(|_| {
            DocodeError::Message("mission ID must be a Do Code-generated UUID.".into())
        })?;
        if parsed.to_string() != id {
            return Err(DocodeError::Message(
                "mission ID must be a canonical UUID.".into(),
            ));
        }
        Ok(self.home.join("missions").join(format!("{id}.json")))
    }

    fn lock(&self, id: &str) -> Result<File> {
        let path = self.home.join("locks").join(format!("{id}.lock"));
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        }
        let file = options.open(&path).map_err(|e| io_error(&path, e))?;
        let meta = file.metadata().map_err(|e| io_error(&path, e))?;
        if !meta.is_file() {
            return Err(DocodeError::Safety(
                "mission lock path is not a regular file.".into(),
            ));
        }
        storage::reject_hardlink_handle(&file, &path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if meta.nlink() > 1 {
                return Err(DocodeError::Safety(
                    "hard-linked mission lock files are denied.".into(),
                ));
            }
        }
        FileExt::lock_exclusive(&file).map_err(|e| io_error(&path, e))?;
        Ok(file)
    }

    fn write(&self, mission: &Mission, replace: bool) -> Result<()> {
        let path = self.mission_path(&mission.id)?;
        let payload =
            serde_json::to_vec(mission).map_err(|e| DocodeError::Message(e.to_string()))?;
        let digest = Sha256::digest(&payload)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let envelope = MissionEnvelope {
            store_version: STORE_VERSION,
            sha256: digest,
            mission: mission.clone(),
        };
        let mut bytes =
            serde_json::to_vec(&envelope).map_err(|e| DocodeError::Message(e.to_string()))?;
        bytes.push(b'\n');
        storage::write_atomic(&path, &bytes, replace)
    }

    fn validate(&self, mission: &Mission) -> Result<()> {
        if mission.schema_version != 1
            || mission.repo_root != self.root.to_string_lossy()
            || mission.project_id != self.project_id
        {
            return Err(DocodeError::Safety(
                "mission belongs to a different project or has an unsupported schema.".into(),
            ));
        }
        if mission.objective.trim().len() < 8
            || mission.objective.len() > MAX_OBJECTIVE_BYTES
            || mission.steps.len() > 500
            || mission.acceptance_criteria.len() > 500
            || mission.evidence_ids.len() > 10_000
        {
            return Err(DocodeError::Safety(
                "mission state violates persisted resource limits.".into(),
            ));
        }
        let mut ids = std::collections::BTreeSet::new();
        for step in &mission.steps {
            if step.mission_id != mission.id || !ids.insert(step.id.as_str()) {
                return Err(DocodeError::Safety(
                    "mission contains a duplicate step or a step bound to another mission.".into(),
                ));
            }
        }
        if mission
            .steps
            .iter()
            .any(|s| s.depends_on.iter().any(|d| !ids.contains(d.as_str())))
        {
            return Err(DocodeError::Safety(
                "mission contains a dependency that does not exist.".into(),
            ));
        }
        Ok(())
    }
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn mission_state_persists_with_project_binding_and_valid_transitions() {
        let root = tempdir().unwrap();
        let store = MissionStore::open(root.path()).unwrap();
        let mission = store.create("Fix the CLI help output").unwrap();
        assert_eq!(mission.status, MissionStatus::Draft);
        assert_eq!(
            store.get(&mission.id).unwrap().objective,
            "Fix the CLI help output"
        );
        assert!(
            store
                .transition(&mission.id, MissionStatus::Running)
                .is_err()
        );
        store.transition(&mission.id, MissionStatus::Ready).unwrap();
        store
            .transition(&mission.id, MissionStatus::Running)
            .unwrap();
        assert_eq!(
            store.get(&mission.id).unwrap().status,
            MissionStatus::Running
        );
        assert_eq!(store.list().unwrap().len(), 1);
    }

    #[test]
    fn mission_checksum_tampering_fails_closed() {
        let root = tempdir().unwrap();
        let store = MissionStore::open(root.path()).unwrap();
        let mission = store.create("Fix the CLI help output").unwrap();
        let path = store.path(&mission.id).unwrap();
        let bytes = storage::read_bounded_regular(&path, MAX_MISSION_BYTES).unwrap();
        let mut envelope: MissionEnvelope = serde_json::from_slice(&bytes).unwrap();
        envelope.mission.objective = "tampered objective with changed state".into();
        fs::write(path, serde_json::to_vec(&envelope).unwrap()).unwrap();
        assert!(store.get(&mission.id).is_err());
    }

    #[test]
    fn concurrent_mission_updates_do_not_lose_appends() {
        // Repeated contention, not one lucky run: the release gate expects
        // high iteration counts, so this stresses several rounds of 8 writers.
        // (A full 100-round soak belongs to CI/stress runs, not the default suite.)
        const ROUNDS: usize = 12;
        for round in 0..ROUNDS {
            let root = tempdir().unwrap();
            let store = MissionStore::open(root.path()).unwrap();
            let mission = store.create("Preserve every concurrent update").unwrap();
            let mut workers = Vec::new();
            for index in 0..8 {
                let root_path = root.path().to_path_buf();
                let mission_id = mission.id.clone();
                workers.push(std::thread::spawn(move || {
                    let store = MissionStore::open(&root_path).unwrap();
                    store
                        .update(&mission_id, |current| {
                            current
                                .evidence_ids
                                .push(format!("evidence-{index}-{round}"));
                            Ok(())
                        })
                        .unwrap();
                }));
            }
            for worker in workers {
                worker.join().unwrap();
            }
            let updated = store.get(&mission.id).unwrap();
            assert_eq!(
                updated.evidence_ids.len(),
                8,
                "round {round} lost concurrent appends"
            );
        }
    }
}
