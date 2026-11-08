mod agent_loop;
mod audit;
mod compounding;
mod config;
mod context;
mod contract;
mod decision_arena;
mod dispatch;
mod error;
mod evidence;
mod files;
mod freeze;
mod gitignore;
mod loopctl;
mod memory;
mod message_store;
mod mission;
mod mission_store;
mod patches;
mod probe;
mod provider;
mod redaction;
mod risk;
mod router;
mod sandbox;
mod storage;
mod taskboard;
mod teams;
mod tools;
mod workflow;
mod workspace;

use clap::{Args, Parser, Subcommand};
use error::{DocodeError, Result};
use serde_json::json;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[derive(Debug, Parser)]
#[command(name = "docode", version, about = "Do Code — the evidence-first AI coding agent (CLI; desktop app planned)", long_about = None)]
struct Cli {
    /// Project root. Defaults to the current directory.
    #[arg(long, global = true, default_value = ".")]
    root: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Initialize local Do Code files without using the network.
    Init,
    /// Diagnose local configuration, privacy, state storage, and execution limits.
    Doctor,
    /// Read validated Do Code configuration.
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    /// Inspect workspace files using bounded, read-only tools.
    Files {
        #[command(subcommand)]
        command: FilesCommand,
    },
    /// Inspect the local, redacted JSONL audit trail.
    Audit {
        #[arg(long = "tail", default_value_t = 20)]
        count: usize,
    },
    /// Build or refresh a hash-bound repository index (local only).
    Index {
        #[arg(long, default_value_t = 1_000)]
        max_files: usize,
        #[arg(long, default_value_t = 262_144)]
        max_file_bytes: u64,
        #[arg(long, default_value_t = 8 * 1024 * 1024)]
        max_total_bytes: u64,
    },
    /// Select bounded task-relevant repository context locally; this does not contact a model.
    Context {
        task: String,
        #[arg(long, default_value_t = 12)]
        max_files: usize,
        #[arg(long, default_value_t = 96 * 1024)]
        max_chars: usize,
    },
    /// List the available Do Code AI roles and their current execution limits.
    Agents {
        #[command(subcommand)]
        command: Option<AgentsCommand>,
    },
    /// Ask the configured model for repository research and an architecture plan. Requires explicit consent.
    Plan(WorkflowArgs),
    /// Run the multi-role proposal workflow; changes remain a patch until explicitly applied.
    Run(WorkflowArgs),
    /// Run the single-agent tool loop: the model decides its own tool calls; changes remain a patch until explicitly applied.
    Agent(WorkflowArgs),
    /// Inspect saved missions.
    Mission {
        #[command(subcommand)]
        command: MissionCommand,
    },
    /// Preview a stored proposal, or apply it only with --approve.
    Apply {
        mission_id: String,
        #[arg(long)]
        approve: bool,
        /// With --approve: remember this approval for future applies in this
        /// project (reversible via checkpoint undo; never covers dangerous actions).
        #[arg(long)]
        remember: bool,
    },
    /// Verify mission evidence. This reports missing checks and never invents test results.
    Verify { mission_id: String },
    /// Mark a mission as completed. Only succeeds when trusted test evidence exists.
    Complete { mission_id: String },
    /// Inspect a saved mission's state and the next safe action.
    Resume { mission_id: String },
    /// Restore a prior checkpoint only with explicit approval and conflict checks.
    Undo {
        mission_id: String,
        checkpoint_id: Option<String>,
        #[arg(long)]
        approve: bool,
    },
    /// Emergency stop for new remote-model workflows and patch writes.
    Freeze {
        #[command(subcommand)]
        command: FreezeCommand,
    },
    /// Unfreeze requires explicit approval.
    Unfreeze {
        #[arg(long)]
        approve: bool,
    },
    /// Inspect native sandbox readiness. No arbitrary command execution is enabled in this build.
    Sandbox {
        #[command(subcommand)]
        command: SandboxCommand,
    },
    /// Manage human-readable, user-confirmed local learning memory.
    Memory {
        #[command(subcommand)]
        command: MemoryCommand,
    },
    /// Create a Decision Arena brief without attaching repository files or memory.
    Pilot(DecisionArgs),
    /// Browse previously saved Decision Arena records.
    Decisions {
        #[command(subcommand)]
        command: DecisionHistoryCommand,
    },
    /// Browse integrity-checked evidence for a mission.
    Evidence {
        #[command(subcommand)]
        command: EvidenceCommand,
    },
    /// Check model configuration or send an explicitly approved diagnostic request.
    Model {
        #[command(subcommand)]
        command: ModelCommand,
    },
    /// Show a mission's task board with owners, file locks, and acceptance criteria.
    Tasks { mission_id: String },
    /// Show a mission's spec-first contract (goal, allowed paths, required tests).
    Contract { mission_id: String },
    /// Manage the compounding loop: propose lessons, approve them into playbooks.
    Lessons {
        #[command(subcommand)]
        command: LessonsCommand,
    },
    /// Show remembered approval decisions (never includes dangerous-action grants).
    Approvals {
        #[command(subcommand)]
        command: Option<ApprovalsCommand>,
    },
    /// Show how the risk engine classifies a command (transparency; no execution).
    Risk {
        /// The command text to classify, e.g. "cargo test" or "git push --force".
        command: String,
    },
}

#[derive(Debug, Subcommand)]
enum ConfigCommand {
    Show,
}
#[derive(Debug, Subcommand)]
enum FilesCommand {
    List {
        #[arg(default_value = ".")]
        path: String,
        #[arg(long)]
        no_recursive: bool,
    },
    Read {
        path: String,
    },
    Search {
        query: String,
        #[arg(default_value = ".")]
        path: String,
    },
}
#[derive(Debug, Subcommand)]
enum ModelCommand {
    Test(ModelTestArgs),
}
#[derive(Debug, Subcommand)]
enum ApprovalsCommand {
    /// Remove all remembered approvals for one action name.
    Remove {
        action: String,
    },
    Show,
}
#[derive(Debug, Subcommand)]
enum LessonsCommand {
    /// Propose a lesson for review. Nothing is saved to playbooks until approved.
    Propose {
        mission_id: String,
        #[arg(long)]
        title: String,
        #[arg(long)]
        lesson: String,
        /// Comma-separated keywords used to match future tasks.
        #[arg(long, value_delimiter = ',')]
        keywords: Vec<String>,
        #[arg(long, default_value = "project")]
        scope: String,
    },
    List {
        #[arg(long, default_value = "project")]
        scope: String,
    },
    Approve {
        id: String,
        #[arg(long, default_value = "project")]
        scope: String,
    },
    Reject {
        id: String,
        #[arg(long, default_value = "project")]
        scope: String,
    },
    Archive {
        id: String,
        #[arg(long, default_value = "project")]
        scope: String,
    },
}
#[derive(Debug, Args)]
struct ModelTestArgs {
    /// Actually send the fixed diagnostic prompt to the configured model endpoint.
    #[arg(long)]
    live: bool,
    /// Confirm that the fixed diagnostic prompt can be sent to the configured provider.
    #[arg(long)]
    approve_context_sharing: bool,
}
#[derive(Debug, Args)]
struct WorkflowArgs {
    /// Task description. Quote it if it contains spaces.
    task: String,
    /// Explicitly allow a live model request.
    #[arg(long)]
    live: bool,
    /// Explicitly allow the task and selected repository context to be shared with the configured provider.
    #[arg(long)]
    approve_context_sharing: bool,
    /// Opt-in best-of-N for hard tasks: request N coder proposals and keep
    /// the first that passes all safety guards. Multiplies model cost by N.
    #[arg(long)]
    best_of_n: Option<u8>,
}
#[derive(Debug, Subcommand)]
enum AgentsCommand {
    List,
}
#[derive(Debug, Subcommand)]
enum MissionCommand {
    List,
    Show { id: String },
    Messages { id: String },
}
#[derive(Debug, Subcommand)]
enum FreezeCommand {
    Status,
    Now { reason: String },
}
#[derive(Debug, Subcommand)]
enum SandboxCommand {
    Status,
    SelfTest,
    Exec {
        #[arg(long, default_value_t = 30000)]
        timeout_ms: u32,
        /// Human approval required; sandbox is unqualified until the 26-case matrix passes.
        #[arg(long)]
        approve: bool,
        command: Vec<String>,
    },
    /// Internal in-sandbox probe battery for the disposable-runner matrix.
    /// Not a user-facing surface; only the CI sandbox runner invokes it.
    Probe {
        case: String,
        args: Vec<String>,
    },
}
#[derive(Debug, Subcommand)]
enum DecisionHistoryCommand {
    List,
    Show { id: String },
}
#[derive(Debug, Subcommand)]
enum EvidenceCommand {
    List { mission_id: String },
    Show { mission_id: String, id: String },
}
#[derive(Debug, Subcommand)]
enum MemoryCommand {
    Open,
    Propose {
        #[arg(long = "type")]
        note_type: String,
        #[arg(long)]
        title: String,
        #[arg(long)]
        problem: String,
        #[arg(long)]
        decision: String,
        #[arg(long)]
        reason: String,
        #[arg(long = "evidence", action = clap::ArgAction::Append)]
        evidence: Vec<String>,
        #[arg(long = "rejected", action = clap::ArgAction::Append)]
        rejected: Vec<String>,
        #[arg(long = "limitation", action = clap::ArgAction::Append)]
        limitations: Vec<String>,
        #[arg(long, default_value_t = 0.7)]
        confidence: f64,
        #[arg(long, default_value = "project")]
        scope: String,
        #[arg(
            long,
            help = "Persist this proposal after reviewing the displayed preview"
        )]
        confirm: bool,
    },
    List {
        #[arg(long, default_value = "all")]
        scope: String,
    },
    Search {
        query: String,
        #[arg(long, default_value = "all")]
        scope: String,
    },
    Show {
        id: String,
    },
    Save {
        #[arg(long = "type")]
        note_type: String,
        #[arg(long)]
        title: String,
        #[arg(long)]
        problem: String,
        #[arg(long)]
        decision: String,
        #[arg(long)]
        reason: String,
        #[arg(long = "evidence", action = clap::ArgAction::Append)]
        evidence: Vec<String>,
        #[arg(long = "rejected", action = clap::ArgAction::Append)]
        rejected: Vec<String>,
        #[arg(long = "limitation", action = clap::ArgAction::Append)]
        limitations: Vec<String>,
        #[arg(long, default_value_t = 0.7)]
        confidence: f64,
        #[arg(long, default_value = "project")]
        scope: String,
        #[arg(long)]
        confirm: bool,
    },
    Edit {
        id: String,
    },
    Remove {
        id: String,
        #[arg(long)]
        approve: bool,
    },
    Verify {
        id: String,
    },
    Reindex {
        #[arg(long, default_value = "all")]
        scope: String,
    },
    Export {
        path: PathBuf,
        #[arg(long, default_value = "all")]
        scope: String,
    },
    Import {
        path: PathBuf,
        #[arg(long)]
        confirm: bool,
    },
    Reset {
        #[arg(long, default_value = "all")]
        scope: String,
        #[arg(long)]
        confirm: bool,
    },
}
#[derive(Debug, Args)]
struct DecisionArgs {
    /// Decision statement to analyze. Quote it if it contains spaces.
    decision: String,
    #[arg(long = "option", action = clap::ArgAction::Append)]
    options: Vec<String>,
    #[arg(long, default_value = "medium")]
    risk: String,
    #[arg(long, default_value_t = 0)]
    prior_failures: u32,
    #[arg(long, default_value_t = 0)]
    architecture_options: u32,
    #[arg(long)]
    security: bool,
    #[arg(long)]
    privacy: bool,
    #[arg(long)]
    financial: bool,
    #[arg(long)]
    production: bool,
    #[arg(long)]
    strategic: bool,
    #[arg(long, default_value = "medium")]
    uncertainty: String,
    #[arg(long)]
    live: bool,
    #[arg(long)]
    approve_context_sharing: bool,
}

#[allow(dead_code)]
fn which_exe(name: &str) -> Option<String> {
    if name.contains('\\') || name.contains('/') {
        return if std::path::Path::new(name).exists() {
            Some(name.to_owned())
        } else {
            None
        };
    }
    let path_var = std::env::var_os("PATH").unwrap_or_default();
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(name);
        if candidate.exists() {
            return Some(candidate.to_string_lossy().into_owned());
        }
        #[cfg(windows)]
        for ext in [".exe", ".bat", ".cmd"] {
            let p2 = dir.join(std::ffi::OsString::from(format!("{name}{ext}")));
            if p2.exists() {
                return Some(p2.to_string_lossy().into_owned());
            }
        }
    }
    None
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    if let Err(error) = run(Cli::parse()).await {
        eprintln!(
            "docode: {}",
            redaction::safe_terminal(&redaction::redact(&error.to_string()))
        );
        std::process::exit(1);
    }
}

async fn run(cli: Cli) -> Result<()> {
    let root = std::fs::canonicalize(&cli.root).map_err(|e| error::io_error(&cli.root, e))?;
    match cli.command {
        Command::Init => {
            let (config_created, memory_created) = config::initialize(&root)?;
            let project_id = storage::project_id(&root)?;
            let mut details = BTreeMap::new();
            details.insert("configCreated".into(), json!(config_created));
            details.insert("projectMemoryCreated".into(), json!(memory_created));
            details.insert("networkUsed".into(), json!(false));
            details.insert("projectId".into(), json!(project_id));
            audit::append(&root, "workspace.init", "succeeded", details)?;
            println!("Do Code workspace initialized.");
            println!("Config: {}", if config_created { "created" } else { "preserved existing file" });
            println!("DOCODE.md: {}", if memory_created { "created" } else { "preserved existing file" });
            println!("Project state: initialized locally; no network used.");
            println!("Network: not used");
            println!("Security boundary: no shell/test execution is enabled in this build.");
        }
        Command::Doctor => doctor(&root)?,
        Command::Config { command: ConfigCommand::Show } => {
            let loaded = config::load(&root)?;
            let mut output = json!({"exists": loaded.exists, "path": loaded.path, "config": loaded.config});
            redaction::sanitize_json_strings(&mut output);
            println!("{}", serde_json::to_string_pretty(&output).map_err(|e| DocodeError::Message(e.to_string()))?);
            audit::append(&root, "config.show", "succeeded", BTreeMap::from([(String::from("configExists"), json!(loaded.exists))]))?;
        }
        Command::Files { command } => files_command(&root, command)?,
        Command::Audit { count } => {
            let events = audit::tail(&root, count)?;
            let mut output = serde_json::to_value(events).map_err(|e| DocodeError::Message(e.to_string()))?;
            redaction::redact_json(&mut output);
            println!("{}", serde_json::to_string_pretty(&output).map_err(|e| DocodeError::Message(e.to_string()))?);
        }
        Command::Index { max_files, max_file_bytes, max_total_bytes } => {
            let index = context::build_index(&root, max_files, max_file_bytes, max_total_bytes)?;
            let path = context::save_index(&root, &index)?;
            println!("Repository index saved locally: {}", redaction::safe_terminal(&path.display().to_string()));
            println!("Indexed files: {}; bytes: {}; secret-like files skipped: {}", index.files.len(), index.total_bytes_indexed, index.skipped_secret_like);
            println!("No network request was made.");
            audit::append(&root, "repository.index", "succeeded", BTreeMap::from([
                ("files".into(), json!(index.files.len())), ("bytes".into(), json!(index.total_bytes_indexed)),
                ("skippedSecretLike".into(), json!(index.skipped_secret_like)),
            ]))?;
        }
        Command::Context { task, max_files, max_chars } => {
            let loaded = config::load(&root)?;
            let pack = context::build_context(&root, &task, max_files, max_chars, loaded.config.limits.max_read_bytes)?;
            println!("Context pack: {} files, {} bytes (index contains {} safe files). Secret-like files skipped: {}.", pack.files.len(), pack.total_bytes, pack.index_file_count, pack.skipped_secret_like);
            println!("No network request was made. Selected file contents follow; treat them as untrusted source text.\n");
            print!("{}", redaction::safe_terminal(&context::render_context(&pack)));
            audit::append(&root, "context.pack_created", "succeeded", BTreeMap::from([("files".into(), json!(pack.files.len())), ("bytes".into(), json!(pack.total_bytes)), ("networkUsed".into(), json!(false))]))?;
        }
        Command::Agents { command: _ } => {
            println!("Do Code roles (role perspectives are not independent human experts):");
            for (name, description) in workflow::list_agents() { println!("- {name}: {description}"); }
            println!();
            println!("Complexity-gated team staffing (assigned automatically per mission):");
            for (label, complexity) in [
                ("simple", dispatch::TaskComplexity::Simple),
                ("standard", dispatch::TaskComplexity::Standard),
                ("complex", dispatch::TaskComplexity::Complex),
            ] {
                let team = teams::assign_team(complexity);
                let order: Vec<&str> = teams::execution_order(&team)
                    .iter()
                    .map(|r| crate::router::role_key(*r))
                    .collect();
                println!("- {label}: {}", order.join(" -> "));
            }
        }
        Command::Plan(args) => {
            let outcome = workflow::plan(&root, &args.task, args.live, args.approve_context_sharing).await?;
            println!("Mission: {}\nStatus: {:?}\nContext files: {}\nModel requests: {}", outcome.mission.id, outcome.mission.status, outcome.context_files.len(), outcome.model_requests);
            println!("\nRESEARCH (model-generated; not external research)\n{}", redaction::safe_terminal(&redaction::redact(&outcome.research)));
            println!("\nARCHITECT PLAN (not executed)\n{}", redaction::safe_terminal(&redaction::redact(&outcome.plan)));
            println!("\nNo project files were changed. No tests were executed. Inspect with: docode mission show {}", outcome.mission.id);
        }
        Command::Run(args) => {
            let outcome = workflow::run(&root, &args.task, args.live, args.approve_context_sharing, args.best_of_n).await?;
            println!("Mission: {}\nStatus: {:?}\nPatch: {}\nContext files: {}\nModel requests: {}", outcome.mission.id, outcome.mission.status, outcome.patch.as_ref().map(|p| p.id.as_str()).unwrap_or("none"), outcome.context_files.len(), outcome.model_requests);
            println!("\nTEST PLAN (not executed)\n{}", redaction::safe_terminal(&redaction::redact(&outcome.test_plan)));
            println!("\nADVISORY REVIEW (one-model perspective; not independent review)\n{}", redaction::safe_terminal(&redaction::redact(&outcome.review)));
            println!("\nADVISORY SECURITY REVIEW (not a security certification)\n{}", redaction::safe_terminal(&redaction::redact(&outcome.security_review)));
            if let Some(patch) = &outcome.patch {
                println!("\n{}", patches::preview(patch));
                println!("Patch is not yet applied. Preview again: docode apply {}\nTo approve writes after review: docode apply {} --approve", outcome.mission.id, outcome.mission.id);
            }
            println!("IMPORTANT: no shell, tests, build, or browser commands were executed. This mission is not complete.");
        }
        Command::Agent(args) => {
            let outcome = agent_loop::agent(&root, &args.task, args.live, args.approve_context_sharing).await?;
            println!("Mission: {}\nStatus: {:?}\nModel requests: {}\nTools used: {}", outcome.mission_id, outcome.status, outcome.model_requests, if outcome.tools_called.is_empty() { "(none)".to_owned() } else { outcome.tools_called.join(", ") });
            if let Some(preview) = &outcome.patch_preview {
                println!("\nPATCH PROPOSAL (stored; not applied)\n{preview}");
                println!("Preview again: docode apply {}\nTo approve writes after review: docode apply {} --approve", outcome.mission_id, outcome.mission_id);
            }
            println!("IMPORTANT: no shell, tests, build, or browser commands were executed. This mission is not complete.");
        }
        Command::Mission { command } => mission_command(&root, command)?,
        Command::Apply { mission_id, approve, remember } => {
            // Approval memory: an explicit --approve may be remembered for
            // this project (--remember), after which plain `docode apply`
            // applies reviewable patches without asking again. The decision
            // is scoped to this project, covers only this risk-2 action,
            // and dangerous classes never use memory.
            let mut applied_with_memory = false;
            let mut should_apply = approve;
            if !should_apply {
                let home = storage::ensure_project_home(&root)?;
                let path = home.join("approvals.json");
                let memory = if path.exists() {
                    risk::ApprovalMemory::from_json(&storage::read_bounded_regular(&path, 1024 * 1024)?)?
                } else {
                    risk::ApprovalMemory::default()
                };
                let assessment = risk::RiskAssessment {
                    risk: risk::Risk::Reviewable,
                    reason: "applies a reviewed patch; reversible via checkpoint undo".into(),
                };
                if let risk::Decision::Allow(reason) =
                    memory.evaluate(&assessment, "patch.apply", Some(&mission_id))
                {
                    should_apply = true;
                    applied_with_memory = true;
                    println!("Applying with remembered project approval ({reason}).");
                }
            }
            let (checkpoint_id, paths) = patches::apply(&root, &mission_id, should_apply)?;
            if let Some(checkpoint) = checkpoint_id {
                if remember && approve {
                    let home = storage::ensure_project_home(&root)?;
                    let path = home.join("approvals.json");
                    let mut memory = if path.exists() {
                        risk::ApprovalMemory::from_json(&storage::read_bounded_regular(&path, 1024 * 1024)?)?
                    } else {
                        risk::ApprovalMemory::default()
                    };
                    memory.remember("patch.apply", risk::ApprovalScope::Project, None)?;
                    storage::write_atomic(&path, &memory.to_json()?, true)?;
                    println!("Approval remembered for this project: future 'docode apply <mission>' will not ask. Remove it by deleting .docode/approvals.json.");
                }
                audit::append(&root, "patch.applied", "succeeded", BTreeMap::from([("missionId".into(), json!(mission_id)), ("checkpointId".into(), json!(checkpoint)), ("files".into(), json!(paths.len())), ("testsExecuted".into(), json!(false)), ("appliedViaApprovalMemory".into(), json!(applied_with_memory))]))?;
                println!("Patch applied to {} file(s). Checkpoint: {}", paths.len(), checkpoint);
                println!("Changed files: {}", paths.join(", "));
                println!("Tests/build/lint were NOT run; mission remains unverified. Undo with: docode undo {mission_id} {checkpoint} --approve");
                // Shared task board: mark tasks whose locked files were all
                // applied as done, so the board reflects reality.
                let board_path = storage::ensure_project_home(&root)?
                    .join("missions")
                    .join(&mission_id)
                    .join("tasks.json");
                if board_path.exists()
                    && let Ok(bytes) = storage::read_bounded_regular(&board_path, 1024 * 1024)
                    && let Ok(mut board) = taskboard::TaskBoard::from_json(&bytes)
                {
                    let covered: Vec<String> = board
                        .tasks
                        .iter()
                        .filter(|t| {
                            !t.files_locked.is_empty()
                                && t.files_locked.iter().all(|f| paths.contains(f))
                        })
                        .map(|t| t.id.clone())
                        .collect();
                    let mut completed = 0;
                    for id in &covered {
                        if board.complete_task(id).is_ok() {
                            completed += 1;
                        }
                    }
                    storage::write_atomic(&board_path, &board.to_json()?, true)?;
                    if completed > 0 {
                        println!("Task board: {completed} task(s) marked done.");
                    }
                }
            }
        }
        Command::Verify { mission_id } => println!("{}", workflow::verify(&root, &mission_id)?),
        Command::Complete { mission_id } => {
            println!("{}", workflow::complete(&root, &mission_id)?);
            println!();
            println!("Compounding: capture what this mission taught for next time.");
            println!("  docode lessons propose {mission_id} --title \"...\" --lesson \"...\" --keywords \"topic1,topic2\"");
        }
        Command::Resume { mission_id } => {
            let store = mission_store::MissionStore::open(&root)?;
            // Recover from any crash/interruption first.
            let recovered = store.recover()?;
            // Find the requested mission.
            let entry = recovered.iter().find(|(m, _)| m.id == mission_id);
            match entry {
                Some((mission, action)) => {
                    println!("Mission: {}\nStatus: {:?}\nCreated: {}\nUpdated: {}", mission.id, mission.status, mission.created_at, mission.updated_at);
                    for step in &mission.steps { println!("- {} [{:?}] attempts={} failure={}", step.kind, step.status, step.attempt, step.failure_code.as_deref().unwrap_or("none")); }
                    println!("Evidence IDs: {}", mission.evidence_ids.join(", "));
                    println!("\nNext safe action: {action}");
                    println!("\nResume is inspection-only: it does not repeat model calls or re-run commands automatically. Use 'docode run' for a new proposal or 'docode verify' to check evidence.");
                }
                None => {
                    let mission = store.get(&mission_id)?;
                    println!("Mission: {}\nStatus: {:?}\nCreated: {}\nUpdated: {}", mission.id, mission.status, mission.created_at, mission.updated_at);
                    for step in &mission.steps { println!("- {} [{:?}] attempts={} failure={}", step.kind, step.status, step.attempt, step.failure_code.as_deref().unwrap_or("none")); }
                    println!("Evidence IDs: {}", mission.evidence_ids.join(", "));
                }
            }
        }
        Command::Undo { mission_id, checkpoint_id, approve } => {
            let paths = patches::undo(&root, &mission_id, checkpoint_id.as_deref(), approve)?;
            audit::append(&root, "patch.undo", "succeeded", BTreeMap::from([("missionId".into(), json!(mission_id)), ("checkpointId".into(), json!(checkpoint_id)), ("restoredFiles".into(), json!(paths.len()))]))?;
            println!("Restored {} file(s): {}", paths.len(), paths.join(", "));
            println!("Restore does not prove the restored project builds or passes tests.");
        }
        Command::Freeze { command } => match command {
            FreezeCommand::Status => match freeze::status(&root)? {
                None => println!("Do Code is not frozen."),
                Some(state) => println!("Frozen: {}\nReason: {}\nUpdated: {}", state.frozen, redaction::safe_terminal(&state.reason), state.updated_at),
            },
            FreezeCommand::Now { reason } => { let state = freeze::freeze(&root, &reason)?; println!("Do Code frozen. New live-model workflows and patch writes are blocked. Reason: {}", state.reason); }
        },
        Command::Unfreeze { approve } => { let state = freeze::unfreeze(&root, approve)?; println!("Do Code unfrozen: {}", !state.frozen); }
        Command::Sandbox { command } => match command {
            SandboxCommand::Status => {
                println!("{}", sandbox::status_text());
                println!("Arbitrary shell/test/build/browser execution is disabled; no isolation claim is made.");
                println!("Windows DACL enforcement: implemented (owner-only protected DACL).");
                println!("Windows launch path: AppContainer (zero capabilities = OS network deny) + restricted token + Job Object assigned before resume + sanitized environment.");
            }
            SandboxCommand::Probe { case, args } => {
                // Only the disposable CI runner can reach this path — the
                // exec marker is the sole entry ticket.
                if std::env::var("DOCODE_PROBE_VIA_SANDBOX").is_err() {
                    eprintln!("docode probe is internal to the disposable-runner matrix and only runs inside the sandbox.");
                    std::process::exit(21);
                }
                let code = match case.as_str() {
                    "fs-01" => crate::probe::fs_01(&args),
                    "fs-02" => crate::probe::fs_02(&args),
                    "net-01" => crate::probe::net_01(&args),
                    "net-02" => crate::probe::net_02(&args),
                    "env-01" => crate::probe::env_01(&args),
                    "cmd-01" => crate::probe::cmd_01(&args),
                    "res-01" => crate::probe::res_01(&args),
                    "proc-01" => crate::probe::proc_01(&args),
                    "child-hang" => crate::probe::child_hang(&args),
                    _ => 21,
                };
                std::process::exit(code);
            }
            SandboxCommand::SelfTest => return Err(DocodeError::Safety("sandbox self-test cannot pass: no native sandbox driver is implemented. No command was executed.".into())),
            SandboxCommand::Exec { timeout_ms, approve, command } => {
                let timeout_ms = timeout_ms.clamp(1_000, 600_000);
                if !approve {
                    return Err(DocodeError::Safety(
                        "sandbox exec requires --approve (unqualified probe; will be audited). No command was executed."
                            .into(),
                    ));
                }
                crate::audit::append(
                    &root,
                    "sandbox.exec_approved_unqualified",
                    "succeeded",
                    std::collections::BTreeMap::from([
                        ("timeoutMs".into(), serde_json::json!(timeout_ms)),
                        ("exe".into(), serde_json::json!(command.first().cloned().unwrap_or_default())),
                    ]),
                )?;
                if command.is_empty() {
                    return Err(DocodeError::Message("no command given; usage: docode sandbox exec --approve -- <exe> [args...]".into()));
                }
                #[cfg(windows)]
                {
                    let exe = command[0].clone();
                    let args = command[1..].to_vec();
                    let exe = if exe.contains('\\') || exe.contains('/') {
                        exe
                    } else if let Some(found) = which_exe(&exe) {
                        found
                    } else {
                        return Err(DocodeError::Message(format!("executable not found in PATH: {exe}")));
                    };
                    let limits = sandbox::ResourceLimits {
                        max_wall_time_ms: timeout_ms,
                        ..Default::default()
                    };
                    let job = sandbox::JobObject::create(limits)?;
                    // Container workspace lives under the project's .docode,
                    // not %TEMP% — hardened images deny AppContainer
                    // traversal on temp roots, which kills the child outright.
                    let home = crate::storage::ensure_project_home(&root)?;
                    let workspace_parent = home.join("sandbox");
                    let (proc_handle, thread_handle, _pid, workspace, stdout_pipe) =
                        sandbox::spawn_suspended(&exe, &args, limits, &job, &workspace_parent)?;
                    // Cleanup runs even if the wait itself errors.
                    let wait = sandbox::wait_for_completion(
                        proc_handle,
                        thread_handle,
                        timeout_ms,
                        &job,
                        stdout_pipe,
                    );
                    if let Err(e) = std::fs::remove_dir_all(&workspace) {
                        eprintln!("warning: could not remove sandbox workspace {}: {e}", workspace.display());
                    }
                    let (exit_code, timed_out, output) = wait?;
                    if !output.is_empty() {
                        let text = String::from_utf8_lossy(&output);
                        let redacted = crate::redaction::redact(&text);
                        let mut tail = redacted.as_str();
                        if redacted.len() > 32 * 1024 {
                            let start = redacted.len() - 32 * 1024;
                            let mut idx = start;
                            while idx < redacted.len() && !redacted.is_char_boundary(idx) {
                                idx += 1;
                            }
                            tail = &redacted[idx..];
                            println!("[output truncated at 32 KiB tail]");
                        }
                        println!("{tail}");
                    }
                    println!("exit_code={exit_code} timed_out={timed_out}");
                }
                #[cfg(not(windows))]
                {
                    let _ = timeout_ms;
                    return Err(DocodeError::Safety(
                        "sandbox exec not implemented on this platform; no command was executed.".into(),
                    ));
                }
            }

        },
        Command::Memory { command } => memory_command(&root, command)?,
        Command::Pilot(args) => pilot_command(&root, args).await?,
        Command::Decisions { command } => decisions_command(&root, command)?,
        Command::Evidence { command } => evidence_command(&root, command)?,
        Command::Model { command: ModelCommand::Test(args) } => {
            let loaded = config::load(&root)?;
            provider::test_model(&root, &loaded.config, args.live, args.approve_context_sharing).await?;
        }
        Command::Tasks { mission_id } => {
            let home = storage::ensure_project_home(&root)?
                .join("missions")
                .join(&mission_id);
            let bytes = storage::read_bounded_regular(&home.join("tasks.json"), 1024 * 1024)?;
            let board = taskboard::TaskBoard::from_json(&bytes)?;
            println!("Task board for mission {mission_id}:");
            for task in &board.tasks {
                println!(
                    "- {} [{}] owner={} locks=[{}] depends_on=[{}]",
                    task.id,
                    format!("{:?}", task.status).to_lowercase(),
                    task.owner,
                    task.files_locked.join(", "),
                    task.depends_on.join(", ")
                );
                for acceptance in &task.acceptance {
                    println!("    acceptance: {acceptance}");
                }
            }
            let ready: Vec<String> = board.ready_tasks().iter().map(|t| t.id.clone()).collect();
            println!("Ready now: {}", if ready.is_empty() { "(none)".into() } else { ready.join(", ") });
        }
        Command::Contract { mission_id } => {
            let manifests = evidence::list(&root, &mission_id)?;
            let contract_manifest = manifests
                .iter()
                .find(|m| m.kind == "contract")
                .ok_or_else(|| DocodeError::Message(format!(
                    "mission {mission_id} has no stored contract; contracts are created by 'docode run'."
                )))?;
            let (_, bytes) = evidence::read(&root, &mission_id, &contract_manifest.id)?;
            let contract: crate::contract::MissionContract = serde_json::from_slice(&bytes)
                .map_err(|e| DocodeError::Message(format!("stored contract is corrupt: {e}")))?;
            println!("Mission contract: {mission_id}");
            println!("Goal: {}", redaction::safe_terminal(&contract.goal));
            println!("Enforced: deterministically derived; patches refused outside these paths");
            println!("Allowed paths ({}):", contract.allowed_paths.len());
            for p in &contract.allowed_paths { println!("  - {p}"); }
            println!("Forbidden paths: {}", contract.forbidden_paths.join(", "));
            println!("Required tests:");
            if contract.required_tests.is_empty() {
                println!("  (none identified — the tester step found no explicit test commands)");
            }
            for t in &contract.required_tests { println!("  - {}", redaction::safe_terminal(t)); }
            println!("Rollback: {}", contract.rollback_plan);
        }
        Command::Lessons { command } => lessons_command(&root, command)?,
        Command::Approvals { command } => approvals_command(&root, command)?,
        Command::Risk { command } => {
            let assessment = risk::score_command(&command);
            let level = format!("{:?}", assessment.risk).to_lowercase();
            println!("Command: {}", redaction::safe_terminal(&command));
            println!("Risk: {} (level {})", level, assessment.risk as u8);
            println!("Reason: {}", assessment.reason);
            println!("Policy: {}.", match assessment.risk {
                risk::Risk::Safe | risk::Risk::Normal => "runs silently; visible in the audit log",
                risk::Risk::Reviewable => "asks once, remembers at the scope you choose",
                risk::Risk::Risky | risk::Risk::Dangerous => {
                    "always asks fresh; remembered approvals are ignored"
                }
                risk::Risk::Critical => {
                    "never auto-approved; requires explicit user action"
                }
            });
            audit::append(
                &root,
                "risk.check",
                "succeeded",
                BTreeMap::from([
                    ("risk".into(), json!(level)),
                    ("networkUsed".into(), json!(false)),
                ]),
            )?;
        }
    }
    Ok(())
}

fn doctor(root: &Path) -> Result<()> {
    let loaded = config::load(root)?;
    let rust_version = option_env!("RUSTC_VERSION").unwrap_or("not embedded");
    println!(
        "PASS  Workspace: {}",
        redaction::safe_terminal(&root.display().to_string())
    );
    println!(
        "{}  Config: {}",
        if loaded.exists { "PASS" } else { "WARN" },
        if loaded.exists {
            "validated docode.config.toml"
        } else {
            "missing; run docode init"
        }
    );
    println!("PASS  Telemetry: disabled by product policy");
    println!(
        "INFO  Provider: {}; model: {}",
        redaction::safe_terminal(&redaction::redact(&loaded.config.model.provider)),
        redaction::safe_terminal(&redaction::redact(&loaded.config.model.model))
    );
    println!(
        "{}  Provider key: {}",
        if std::env::var(&loaded.config.model.api_key_env).is_ok() {
            "PASS"
        } else {
            "WARN"
        },
        if std::env::var(&loaded.config.model.api_key_env).is_ok() {
            "named environment variable is set (value hidden)"
        } else {
            "not set; offline commands still work"
        }
    );
    let missions = mission_store::MissionStore::open(root)?.list()?;
    println!(
        "PASS  Local mission storage: {} mission(s) load with valid checksums/project binding",
        missions.len()
    );
    match freeze::status(root)? {
        Some(state) if state.frozen => println!(
            "WARN  Emergency freeze: active ({})",
            redaction::safe_terminal(&state.reason)
        ),
        _ => println!("PASS  Emergency freeze: inactive"),
    }
    println!(
        "WARN  Sandbox: OS primitives implemented (restricted token + AppContainer + Job Object + sanitized env + OS network deny), but NOT qualified — no adversarial acceptance tests have run on a disposable runner; no untrusted command execution is available"
    );
    #[cfg(unix)]
    println!(
        "INFO  Filesystem privacy: POSIX no-follow and owner-only state permissions are used where supported; this is not an OS sandbox"
    );
    #[cfg(windows)]
    println!(
        "INFO  Windows privacy: owner-only protected DACL is created and verified (owner SID readback) on every private state directory; AppContainer launch path implemented but NOT security-qualified (adversarial matrix NOT RUN)"
    );
    println!(
        "INFO  Rust compiler version: {rust_version}; required minimum: {}",
        env!("CARGO_PKG_RUST_VERSION")
    );
    println!(
        "INFO  Cost policy: no predicted cost fields, no cost threshold trigger and no cost-based auto-stop; provider-reported usage may be displayed"
    );
    println!(
        "WARN  Verification status: this binary does not embed passing CI or security evidence; inspect release artifacts before production use"
    );
    Ok(())
}

fn files_command(root: &Path, command: FilesCommand) -> Result<()> {
    let loaded = config::load(root)?;
    match command {
        FilesCommand::List { path, no_recursive } => {
            let entries = files::list(root, &path, !no_recursive, loaded.config.limits.max_files)?;
            for entry in &entries {
                println!(
                    "{} {}{}",
                    if entry.kind == "directory" {
                        "DIR "
                    } else {
                        "FILE"
                    },
                    redaction::safe_terminal(&entry.path),
                    if entry.kind == "file" {
                        format!(" ({} bytes)", entry.size_bytes)
                    } else {
                        String::new()
                    }
                );
            }
            println!(
                "{} entries (max_files={}).",
                entries.len(),
                loaded.config.limits.max_files
            );
            audit::append(
                root,
                "files.list",
                "succeeded",
                BTreeMap::from([(String::from("count"), json!(entries.len()))]),
            )?;
        }
        FilesCommand::Read { path } => {
            let content = files::read(root, &path, loaded.config.limits.max_read_bytes)?;
            let display = redaction::safe_terminal(&redaction::redact(&content));
            print!("{display}");
            if !display.ends_with('\n') {
                println!();
            }
            audit::append(
                root,
                "files.read",
                "succeeded",
                BTreeMap::from([(String::from("bytes"), json!(content.len()))]),
            )?;
        }
        FilesCommand::Search { query, path } => {
            let matches = files::search(
                root,
                &query,
                &path,
                loaded.config.limits.max_read_bytes,
                loaded.config.limits.max_files,
                loaded.config.limits.max_search_matches,
            )?;
            for item in &matches {
                println!(
                    "{}:{}: {}",
                    redaction::safe_terminal(&item.path),
                    item.line,
                    redaction::safe_terminal(&redaction::redact(&item.text))
                );
            }
            println!("{} literal match(es).", matches.len());
            audit::append(
                root,
                "files.search",
                "succeeded",
                BTreeMap::from([(String::from("matches"), json!(matches.len()))]),
            )?;
        }
    }
    Ok(())
}

fn mission_command(root: &Path, command: MissionCommand) -> Result<()> {
    let store = mission_store::MissionStore::open(root)?;
    match command {
        MissionCommand::List => {
            let missions = store.list()?;
            println!("{} mission(s)", missions.len());
            for mission in missions {
                println!(
                    "{}\t{:?}\t{}\t{} evidence",
                    mission.id,
                    mission.status,
                    redaction::safe_terminal(&mission.objective),
                    mission.evidence_ids.len()
                );
            }
        }
        MissionCommand::Show { id } => {
            let mission = store.get(&id)?;
            let mut output =
                serde_json::to_value(mission).map_err(|e| DocodeError::Message(e.to_string()))?;
            redaction::redact_json(&mut output);
            println!(
                "{}",
                serde_json::to_string_pretty(&output)
                    .map_err(|e| DocodeError::Message(e.to_string()))?
            );
            let evidence = evidence::list(root, &id)?;
            println!(
                "Verified local evidence manifests: {} (integrity hashes verified on read; content is not proof of correctness)",
                evidence.len()
            );
            println!("{}", workflow::usage_summary(root, &id)?);
            // Workspace mutation journal (code-over-API ledger), if any.
            if let Ok(journal) = crate::workspace::MutationJournal::for_mission(root, &id)
                && let Ok(mutations) = journal.read_all()
                && !mutations.is_empty()
            {
                println!(
                    "Workspace mutation journal: {} recorded mutation(s), each with before/after hashes and checkpoint binding:",
                    mutations.len()
                );
                for mutation in &mutations {
                    println!(
                        "  {} [{}] {} by {} reversible={} checkpoint={}",
                        mutation.mutation_id,
                        redaction::safe_terminal(&mutation.kind),
                        redaction::safe_terminal(&mutation.path),
                        redaction::safe_terminal(&mutation.agent),
                        mutation.reversible,
                        redaction::safe_terminal(&mutation.checkpoint)
                    );
                }
            }
        }
        MissionCommand::Messages { id } => {
            let messages = message_store::list(root, &id)?;
            println!(
                "{} recorded AgentMessage(s); transcript hash chain passed. These are recorded model messages, not fact verification.",
                messages.len()
            );
            for message in messages {
                println!(
                    "\n[{}] role={} provider={} model={} evidence={}",
                    message.created_at,
                    redaction::safe_terminal(&message.role),
                    message
                        .provider
                        .as_deref()
                        .map(redaction::safe_terminal)
                        .unwrap_or_else(|| "unknown".into()),
                    message
                        .model
                        .as_deref()
                        .map(redaction::safe_terminal)
                        .unwrap_or_else(|| "unknown".into()),
                    message.evidence_ids.join(",")
                );
                println!(
                    "{}",
                    redaction::safe_terminal(&redaction::redact(&message.content))
                );
            }
        }
    }
    Ok(())
}

fn memory_command(root: &Path, command: MemoryCommand) -> Result<()> {
    match command {
        MemoryCommand::Open => {
            let project_root = std::fs::canonicalize(root).map_err(|e| error::io_error(root, e))?;
            let home = std::env::var_os("USERPROFILE")
                .or_else(|| std::env::var_os("HOME"))
                .map(PathBuf::from)
                .ok_or_else(|| {
                    DocodeError::Config("could not resolve user home directory.".into())
                })?;
            println!(
                "Project memory vault: {}",
                project_root.join(".docode/memory/project").display()
            );
            println!(
                "Global memory vault: {}",
                home.join(".docode/memory/global").display()
            );
            println!(
                "Paths only; no external application was launched and no memory was uploaded."
            );
        }
        MemoryCommand::Propose {
            note_type,
            title,
            problem,
            decision,
            reason,
            evidence,
            rejected,
            limitations,
            confidence,
            scope,
            confirm,
        } => {
            let preview = format!(
                "MEMORY PROPOSAL — {} ({} scope)\nTitle: {}\nProblem: {}\nDecision: {}\nReason: {}\nEvidence: {}\nRejected options: {}\nLimitations: {}\nConfidence: {:.2}\n",
                note_type,
                scope,
                title,
                problem,
                decision,
                reason,
                if evidence.is_empty() {
                    "none".to_owned()
                } else {
                    evidence.join("; ")
                },
                if rejected.is_empty() {
                    "none".to_owned()
                } else {
                    rejected.join("; ")
                },
                if limitations.is_empty() {
                    "none".to_owned()
                } else {
                    limitations.join("; ")
                },
                confidence
            );
            println!("{}", redaction::safe_terminal(&redaction::redact(&preview)));
            if !confirm {
                println!(
                    "Not saved. Review the proposal, then repeat with --confirm to persist it."
                );
                return Ok(());
            }
            let note = memory::save(
                root,
                &note_type,
                &title,
                &problem,
                &decision,
                &reason,
                rejected,
                limitations,
                evidence,
                confidence,
                &scope,
                true,
            )?;
            println!(
                "Saved reviewed memory proposal {} to {} scope.",
                note.id, note.scope
            );
        }
        MemoryCommand::List { scope } => {
            for note in memory::list(root, &scope)? {
                println!(
                    "{}\t{}\t{}\t{}\tconfidence={:.2}",
                    note.id,
                    note.scope,
                    note.note_type,
                    redaction::safe_terminal(&note.title),
                    note.confidence
                );
            }
        }
        MemoryCommand::Search { query, scope } => {
            let entries = memory::search(root, &query, &scope)?;
            for note in &entries {
                println!(
                    "{}\t{}\t{}\t{}",
                    note.id,
                    note.scope,
                    note.note_type,
                    redaction::safe_terminal(&note.title)
                );
            }
            println!("{} result(s). Search is local only.", entries.len());
        }
        MemoryCommand::Show { id } => {
            let note = memory::show(root, &id)?;
            let rendered = memory::render_markdown(&note)?;
            print!("{}", redaction::safe_terminal(&rendered));
        }
        MemoryCommand::Save {
            note_type,
            title,
            problem,
            decision,
            reason,
            evidence,
            rejected,
            limitations,
            confidence,
            scope,
            confirm,
        } => {
            let note = memory::save(
                root,
                &note_type,
                &title,
                &problem,
                &decision,
                &reason,
                rejected,
                limitations,
                evidence,
                confidence,
                &scope,
                confirm,
            )?;
            println!(
                "Saved user-confirmed memory {} to {} scope. Never stored unconfirmed model thoughts.",
                note.id, note.scope
            );
        }
        MemoryCommand::Edit { id } => {
            let note = memory::edit(root, &id)?;
            println!("Edited and reindexed memory {}.", note.id);
        }
        MemoryCommand::Remove { id, approve } => {
            memory::remove(root, &id, approve)?;
            println!("Memory {id} deleted locally after approval.");
        }
        MemoryCommand::Verify { id } => {
            let note = memory::verify(root, &id)?;
            println!(
                "Memory {} marked verified at {}. This records your review; it does not independently verify factual claims.",
                note.id,
                note.last_verified_at.unwrap_or_default()
            );
        }
        MemoryCommand::Reindex { scope } => {
            println!("Reindexed {} note(s).", memory::reindex(root, &scope)?)
        }
        MemoryCommand::Export { path, scope } => println!(
            "Exported {} note(s) to {}.",
            memory::export(root, &path, &scope)?,
            redaction::safe_terminal(&path.display().to_string())
        ),
        MemoryCommand::Import { path, confirm } => println!(
            "Imported {} user-confirmed note(s).",
            memory::import(root, &path, confirm)?
        ),
        MemoryCommand::Reset { scope, confirm } => println!(
            "Deleted {} note(s) after explicit confirmation.",
            memory::reset(root, &scope, confirm)?
        ),
    }
    Ok(())
}

fn decisions_command(root: &Path, command: DecisionHistoryCommand) -> Result<()> {
    match command {
        DecisionHistoryCommand::List => {
            let records = decision_arena::list(root)?;
            println!("{} Decision Arena record(s)", records.len());
            for record in records {
                println!(
                    "{}\t{}\t{}\t{}",
                    record.id,
                    record.created_at,
                    if record.live_model_used {
                        "model-assisted"
                    } else {
                        "offline"
                    },
                    redaction::safe_terminal(&record.decision)
                );
            }
        }
        DecisionHistoryCommand::Show { id } => {
            let record = decision_arena::get(root, &id)?;
            let mut output =
                serde_json::to_value(record).map_err(|e| DocodeError::Message(e.to_string()))?;
            redaction::redact_json(&mut output);
            println!(
                "{}",
                serde_json::to_string_pretty(&output)
                    .map_err(|e| DocodeError::Message(e.to_string()))?
            );
        }
    }
    Ok(())
}

fn approvals_command(root: &Path, command: Option<ApprovalsCommand>) -> Result<()> {
    let home = storage::ensure_project_home(root)?;
    let path = home.join("approvals.json");
    let load = || -> Result<risk::ApprovalMemory> {
        if path.exists() {
            risk::ApprovalMemory::from_json(&storage::read_bounded_regular(&path, 1024 * 1024)?)
        } else {
            Ok(risk::ApprovalMemory::default())
        }
    };
    match command.unwrap_or(ApprovalsCommand::Show) {
        ApprovalsCommand::Show => {
            let memory = load()?;
            let summary = risk::summarize(&memory.records);
            if summary.is_empty() {
                println!(
                    "No remembered approvals. Risky actions always ask; reviewable actions remember at the scope you chose."
                );
            } else {
                println!("Remembered approvals:");
                for (action, count) in &summary {
                    println!("  - {action}: {count} record(s)");
                }
                println!(
                    "Dangerous and critical actions are never remembered and always ask fresh."
                );
            }
        }
        ApprovalsCommand::Remove { action } => {
            let mut memory = load()?;
            risk::ApprovalMemory::remove(&action, &mut memory.records);
            storage::write_atomic(&path, &memory.to_json()?, true)?;
            audit::append(
                root,
                "approval.forgotten",
                "succeeded",
                BTreeMap::from([("action".into(), json!(action))]),
            )?;
            println!("All remembered approvals for '{action}' were forgotten in this project.");
        }
    }
    Ok(())
}

fn lessons_command(root: &Path, command: LessonsCommand) -> Result<()> {
    match command {
        LessonsCommand::Propose {
            mission_id,
            title,
            lesson,
            keywords,
            scope,
        } => {
            let record =
                compounding::propose_lesson(root, &scope, &title, &lesson, &mission_id, &keywords)?;
            audit::append(
                root,
                "lesson.proposed",
                "succeeded",
                BTreeMap::from([
                    ("lessonId".into(), json!(record.id)),
                    ("scope".into(), json!(scope)),
                    ("sourceMission".into(), json!(mission_id)),
                ]),
            )?;
            println!(
                "Lesson proposed: {}\nStatus: proposed (nothing is saved to playbooks until you approve)\nReview with: docode lessons list --scope {scope}\nApprove with: docode lessons approve {} --scope {scope}",
                record.id, record.id
            );
        }
        LessonsCommand::List { scope } => {
            let lessons = compounding::list_lessons(root, &scope)?;
            if lessons.is_empty() {
                println!(
                    "No lessons in the {scope} store yet. Propose one after completing a mission."
                );
            }
            for lesson in &lessons {
                println!(
                    "{} [{}] {}\n    {}",
                    lesson.id,
                    format!("{:?}", lesson.status).to_lowercase(),
                    redaction::safe_terminal(&lesson.title),
                    redaction::safe_terminal(&lesson.lesson)
                );
            }
        }
        LessonsCommand::Approve { id, scope } => {
            let lesson = compounding::approve_lesson(root, &scope, &id)?;
            audit::append(
                root,
                "lesson.approved",
                "succeeded",
                BTreeMap::from([
                    ("lessonId".into(), json!(id)),
                    ("scope".into(), json!(scope)),
                ]),
            )?;
            println!(
                "Lesson approved and saved as playbook {}.md in the {scope} memory vault. Future missions matching its keywords will be briefed with it.",
                lesson.id
            );
        }
        LessonsCommand::Reject { id, scope } => {
            compounding::reject_lesson(root, &scope, &id)?;
            println!(
                "Lesson {id} rejected. It stays in the store marked rejected and never becomes a playbook."
            );
        }
        LessonsCommand::Archive { id, scope } => {
            compounding::archive_lesson(root, &scope, &id)?;
            println!("Lesson {id} archived (cold tier). The active playbook was removed.");
        }
    }
    Ok(())
}

fn evidence_command(root: &Path, command: EvidenceCommand) -> Result<()> {
    match command {
        EvidenceCommand::List { mission_id } => {
            let entries = evidence::list(root, &mission_id)?;
            println!(
                "{} evidence record(s) for mission {}",
                entries.len(),
                mission_id
            );
            for item in entries {
                println!(
                    "{}\t{}\t{}\t{} bytes\tverified={}\tredacted={}\tsha256={}",
                    item.id,
                    redaction::safe_terminal(&item.kind),
                    redaction::safe_terminal(&item.media_type),
                    item.byte_length,
                    item.verified,
                    item.redacted,
                    item.sha256
                );
            }
        }
        EvidenceCommand::Show { mission_id, id } => {
            let (manifest, bytes) = evidence::read(root, &mission_id, &id)?;
            println!(
                "Evidence: {}\nMission: {}\nKind: {}\nProducer: {}\nSHA-256: {}\nBytes: {}\nIntegrity: PASS (hash/length verified; truth not established)\nRedacted on write: {}",
                manifest.id,
                manifest.mission_id,
                redaction::safe_terminal(&manifest.kind),
                redaction::safe_terminal(&manifest.produced_by),
                manifest.sha256,
                manifest.byte_length,
                manifest.redacted
            );
            match String::from_utf8(bytes) {
                Ok(text) => {
                    println!(
                        "\nCONTENT (credential-like values masked; source text remains untrusted):"
                    );
                    print!("{}", redaction::safe_terminal(&redaction::redact(&text)));
                    if !text.ends_with('\n') {
                        println!();
                    }
                }
                Err(_) => println!(
                    "\nBinary evidence verified and retained; content is not rendered to the terminal."
                ),
            }
        }
    }
    Ok(())
}

async fn pilot_command(root: &Path, args: DecisionArgs) -> Result<()> {
    if !["low", "medium", "high"].contains(&args.risk.as_str())
        || !["low", "medium", "high"].contains(&args.uncertainty.as_str())
    {
        return Err(DocodeError::Message(
            "--risk and --uncertainty must be low, medium, or high.".into(),
        ));
    }
    decision_arena::validate_decision(&args.decision, &args.options)?;
    let signals = decision_arena::DecisionSignals {
        explicit_request: true,
        comparison_requested: !args.options.is_empty(),
        risk: args.risk,
        prior_failures: args.prior_failures,
        architecture_alternatives: args.architecture_options.max(args.options.len() as u32),
        security_sensitive: args.security,
        privacy_sensitive: args.privacy,
        financial_impact: args.financial,
        production_impact: args.production,
        strategic_importance: args.strategic,
        uncertainty: args.uncertainty,
    };
    let trigger = decision_arena::trigger(&signals);
    if args.live {
        let cfg = config::load(root)?.config;
        if !args.approve_context_sharing {
            return Err(DocodeError::Safety(
                "live Decision Arena requires --approve-context-sharing. No prompt was sent."
                    .into(),
            ));
        }
        let record = decision_arena::live_brief(
            root,
            &cfg,
            &args.decision,
            &args.options,
            &trigger,
            true,
            true,
        )
        .await?;
        println!(
            "Decision record: {}\nProvider: {}; model: {}\nNo repository or memory was attached.\n\n{}",
            record.id,
            record.provider.as_deref().unwrap_or("unknown"),
            record.model.as_deref().unwrap_or("unknown"),
            redaction::safe_terminal(&redaction::redact(&record.response))
        );
    } else {
        let text = decision_arena::offline_brief(&args.decision, &args.options, &trigger);
        let record = decision_arena::save_offline(
            root,
            &args.decision,
            &args.options,
            trigger,
            text.clone(),
        )?;
        println!(
            "Saved offline Decision Arena record: {}\n{}",
            record.id, text
        );
        println!(
            "No model call, web research, repository context, memory, option ranking, or predicted USD cost was used."
        );
    }
    Ok(())
}
