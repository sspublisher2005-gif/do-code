use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    process::Command,
    sync::atomic::{AtomicBool, Ordering},
    thread,
};
use tempfile::tempdir;

fn forge() -> Command {
    Command::new(env!("CARGO_BIN_EXE_forge"))
}

#[test]
fn help_and_version_work() {
    let version = forge().arg("--version").output().expect("launch forge");
    assert!(version.status.success());
    assert!(String::from_utf8_lossy(&version.stdout).contains("0.7.0-rs.1"));
    let help = forge().arg("--help").output().expect("launch forge");
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("Forge CLI"));
}

#[test]
fn init_is_local_and_preserves_existing_files() {
    let root = tempdir().unwrap();
    let output = forge()
        .args(["--root", root.path().to_str().unwrap(), "init"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("Network: not used"));
    let config_before = fs::read(root.path().join("forge.config.toml")).unwrap();
    let memory_before = fs::read(root.path().join("FORGE.md")).unwrap();
    let second = forge()
        .args(["init", "--root", root.path().to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        second.status.success(),
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );
    assert_eq!(
        fs::read(root.path().join("forge.config.toml")).unwrap(),
        config_before
    );
    assert_eq!(
        fs::read(root.path().join("FORGE.md")).unwrap(),
        memory_before
    );
}

#[test]
fn files_are_read_only_and_sensitive_paths_are_denied() {
    let root = tempdir().unwrap();
    fs::write(
        root.path().join("hello.txt"),
        "hello marker\napi_key=read-secret-value-123\n",
    )
    .unwrap();
    fs::write(root.path().join(".env"), "API_KEY=never-read").unwrap();
    let read = forge()
        .args([
            "--root",
            root.path().to_str().unwrap(),
            "files",
            "read",
            "hello.txt",
        ])
        .output()
        .unwrap();
    assert!(read.status.success());
    let display = String::from_utf8_lossy(&read.stdout);
    assert!(display.contains("hello marker"));
    assert!(
        !display.contains("read-secret-value-123"),
        "terminal output must redact common secret assignments"
    );
    assert!(display.contains("[REDACTED]"));
    let denied = forge()
        .args([
            "--root",
            root.path().to_str().unwrap(),
            "files",
            "read",
            ".env",
        ])
        .output()
        .unwrap();
    assert!(!denied.status.success());
    assert!(!String::from_utf8_lossy(&denied.stdout).contains("never-read"));
}

#[test]
fn model_test_is_network_silent_without_live_flag() {
    let root = tempdir().unwrap();
    let init = forge()
        .args(["--root", root.path().to_str().unwrap(), "init"])
        .output()
        .unwrap();
    assert!(init.status.success());
    let model = forge()
        .args(["--root", root.path().to_str().unwrap(), "model", "test"])
        .output()
        .unwrap();
    assert!(model.status.success());
    assert!(String::from_utf8_lossy(&model.stdout).contains("no model request was sent"));
}

#[test]
fn live_request_requires_context_sharing_approval() {
    let root = tempdir().unwrap();
    let init = forge()
        .args(["--root", root.path().to_str().unwrap(), "init"])
        .output()
        .unwrap();
    assert!(init.status.success());
    let model = forge()
        .args([
            "--root",
            root.path().to_str().unwrap(),
            "model",
            "test",
            "--live",
        ])
        .output()
        .unwrap();
    assert!(!model.status.success());
    assert!(String::from_utf8_lossy(&model.stderr).contains("--approve-context-sharing"));
}

#[test]
fn cost_prediction_and_cost_budget_flags_are_not_exposed() {
    let help = forge().arg("--help").output().unwrap();
    let text = String::from_utf8_lossy(&help.stdout);
    assert!(!text.contains("--max-cost-usd"));
    assert!(!text.contains("--estimated-cost"));
}

#[test]
fn memory_propose_previews_without_writing_until_explicit_confirmation() {
    let root = tempdir().unwrap();
    let args = [
        "--root",
        root.path().to_str().unwrap(),
        "memory",
        "propose",
        "--type",
        "decision",
        "--title",
        "Use bounded parsing",
        "--problem",
        "Inputs can be large",
        "--decision",
        "Reject oversized input",
        "--reason",
        "Bounded parsing limits resource use",
        "--confidence",
        "0.9",
        "--scope",
        "project",
    ];
    let preview = forge().args(args).output().unwrap();
    assert!(
        preview.status.success(),
        "{}",
        String::from_utf8_lossy(&preview.stderr)
    );
    let text = String::from_utf8_lossy(&preview.stdout);
    assert!(text.contains("MEMORY PROPOSAL"));
    assert!(text.contains("Not saved"));
    assert!(
        !root.path().join(".forge").exists(),
        "preview must not create Forge state"
    );

    let saved = forge().args(args).arg("--confirm").output().unwrap();
    assert!(
        saved.status.success(),
        "{}",
        String::from_utf8_lossy(&saved.stderr)
    );
    assert!(String::from_utf8_lossy(&saved.stdout).contains("Saved reviewed memory proposal"));
    let notes = root.path().join(".forge/memory/project/notes");
    assert_eq!(fs::read_dir(notes).unwrap().count(), 1);
}

#[test]
fn memory_open_displays_vault_paths_without_creating_state() {
    let root = tempdir().unwrap();
    let output = forge()
        .args(["--root", root.path().to_str().unwrap(), "memory", "open"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("Project memory vault:"));
    assert!(text.contains("Global memory vault:"));
    assert!(text.contains("no external application was launched"));
    assert!(!root.path().join(".forge").exists());
}

/// Serves the same fixture OpenAI-compatible chat-completion response to every request it
/// receives, on a background thread, until `stop` is set. Returns the local base URL.
fn spawn_patch_fixture_server(stop: std::sync::Arc<AtomicBool>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let patch_json = serde_json::json!({
        "summary": "Implement add(a, b) in add.py",
        "files": [{
            "path": "add.py",
            "content": "def add(a, b):\n    \"\"\"Add two numbers.\"\"\"\n    return a + b\n",
            "reason": "Replace the TODO stub with a real implementation per the task."
        }]
    })
    .to_string();
    thread::spawn(move || {
        while !stop.load(Ordering::SeqCst) {
            let Ok((stream, _)) = listener.accept() else {
                thread::sleep(std::time::Duration::from_millis(5));
                continue;
            };
            let patch_json = patch_json.clone();
            // One thread per connection: a slow or partial request can never
            // delay or disturb the next response.
            thread::spawn(move || {
                let mut stream = stream;
                let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(5)));
                let mut received: Vec<u8> = Vec::new();
                let mut expected: Option<usize> = None;
                let mut chunk = [0_u8; 8192];
                loop {
                    if expected.is_some_and(|total| received.len() >= total) {
                        break;
                    }
                    let Ok(n) = stream.read(&mut chunk) else {
                        break;
                    };
                    if n == 0 {
                        break;
                    }
                    received.extend_from_slice(&chunk[..n]);
                    if expected.is_none()
                        && let Some(pos) = received.windows(4).position(|w| w == b"\r\n\r\n")
                    {
                        let headers = String::from_utf8_lossy(&received[..pos]);
                        expected = headers.lines().find_map(|line| {
                            let mut parts = line.splitn(2, ':');
                            match parts.next()?.trim().to_ascii_lowercase().as_str() {
                                "content-length" => {
                                    Some(pos + 4 + parts.next()?.trim().parse::<usize>().ok()?)
                                }
                                _ => None,
                            }
                        });
                    }
                }
                let body = serde_json::json!({
                    "id": "fixture-req",
                    "model": "fixture-model",
                    "choices": [{"message": {"content": patch_json}}],
                    "usage": {"prompt_tokens": 10, "completion_tokens": 20}
                })
                .to_string();
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();
                // Half-close our side so the client sees EOF after the body
                // instead of racing its connection teardown.
                let _ = stream.shutdown(std::net::Shutdown::Both);
            });
        }
    });
    format!("http://{address}/v1")
}

/// End-to-end: init -> run (6-role workflow against a local fixture provider) -> preview apply
/// -> approve apply -> verify (must honestly refuse completion; no sandbox ran) -> preview undo
/// -> approve undo -> file content and hash are restored exactly to their pre-patch state.
#[test]
fn mission_lifecycle_run_apply_verify_undo_round_trips_file_content() {
    let stop = std::sync::Arc::new(AtomicBool::new(false));
    let base_url = spawn_patch_fixture_server(stop.clone());

    let root = tempdir().unwrap();
    let root_str = root.path().to_str().unwrap();
    let original =
        "def add(a, b):\n    \"\"\"Add two numbers.\"\"\"\n    pass  # TODO: implement\n";
    fs::write(root.path().join("add.py"), original).unwrap();

    let init = forge().args(["--root", root_str, "init"]).output().unwrap();
    assert!(init.status.success());

    let config_path = root.path().join("forge.config.toml");
    let config = fs::read_to_string(&config_path).unwrap();
    let config = config.replace(
        "base_url = \"https://api.openai.com/v1\"",
        &format!("base_url = \"{base_url}\""),
    );
    fs::write(&config_path, config).unwrap();

    let run = forge()
        .env("OPENAI_API_KEY", "fixture-not-a-real-key")
        .args([
            "--root",
            root_str,
            "run",
            "Implement the add function in add.py",
            "--live",
            "--approve-context-sharing",
        ])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "run failed: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    let run_out = String::from_utf8_lossy(&run.stdout).to_string();
    assert!(run_out.contains("not yet applied") || run_out.contains("Patch is not yet applied"));
    assert_eq!(
        fs::read_to_string(root.path().join("add.py")).unwrap(),
        original,
        "run() must only propose a patch, never write files directly"
    );

    let mission_id = run_out
        .lines()
        .find_map(|l| l.strip_prefix("Mission: "))
        .expect("mission id in run output")
        .trim()
        .to_owned();

    let preview = forge()
        .args(["--root", root_str, "apply", &mission_id])
        .output()
        .unwrap();
    assert!(preview.status.success());
    assert!(String::from_utf8_lossy(&preview.stdout).contains("Dry run only"));
    assert_eq!(
        fs::read_to_string(root.path().join("add.py")).unwrap(),
        original,
        "preview apply without --approve must not write files"
    );

    let apply = forge()
        .args(["--root", root_str, "apply", &mission_id, "--approve"])
        .output()
        .unwrap();
    assert!(
        apply.status.success(),
        "apply failed: {}",
        String::from_utf8_lossy(&apply.stderr)
    );
    let apply_out = String::from_utf8_lossy(&apply.stdout).to_string();
    let checkpoint_id = apply_out
        .lines()
        .find_map(|l| l.split("Checkpoint: ").nth(1))
        .expect("checkpoint id in apply output")
        .trim()
        .to_owned();
    let applied_content = fs::read_to_string(root.path().join("add.py")).unwrap();
    assert!(applied_content.contains("return a + b"));
    assert_ne!(applied_content, original);

    let verify = forge()
        .args(["--root", root_str, "verify", &mission_id])
        .output()
        .unwrap();
    assert!(verify.status.success());
    let verify_out = String::from_utf8_lossy(&verify.stdout).to_string();
    assert!(verify_out.contains("Completion allowed: false"));
    assert!(verify_out.contains("has not executed project tests"));

    let undo_preview = forge()
        .args(["--root", root_str, "undo", &mission_id, &checkpoint_id])
        .output()
        .unwrap();
    assert!(!undo_preview.status.success());
    assert!(String::from_utf8_lossy(&undo_preview.stderr).contains("--approve"));
    assert_eq!(
        fs::read_to_string(root.path().join("add.py")).unwrap(),
        applied_content,
        "undo preview without --approve must not restore files"
    );

    let undo = forge()
        .args([
            "--root",
            root_str,
            "undo",
            &mission_id,
            &checkpoint_id,
            "--approve",
        ])
        .output()
        .unwrap();
    assert!(
        undo.status.success(),
        "undo failed: {}",
        String::from_utf8_lossy(&undo.stderr)
    );
    assert_eq!(
        fs::read_to_string(root.path().join("add.py")).unwrap(),
        original,
        "undo must restore the exact original file content"
    );

    stop.store(true, Ordering::SeqCst);
}
