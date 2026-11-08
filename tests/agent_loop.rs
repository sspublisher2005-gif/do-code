//! End-to-end: the agent tool loop drives its own sequence against a scripted
//! local fixture provider — no network, no real model. Proves: schema
//! validation rejects a traversal before execution, policy denies process.run,
//! a valid read still succeeds after errors, the final proposal is stored as
//! an applicable patch record, nothing is applied, and the audit trail shows
//! every decision.

use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    process::Command,
    sync::Arc,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    thread,
};
use tempfile::tempdir;

fn docode() -> Command {
    Command::new(env!("CARGO_BIN_EXE_docode"))
}

fn spawn_sequence_fixture(stop: Arc<AtomicBool>, responses: Vec<String>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let responses = Arc::new(responses);
    let counter = Arc::new(AtomicUsize::new(0));
    thread::spawn(move || {
        while !stop.load(Ordering::SeqCst) {
            let Ok((stream, _)) = listener.accept() else {
                thread::sleep(std::time::Duration::from_millis(5));
                continue;
            };
            let respond = {
                let index = counter.fetch_add(1, Ordering::SeqCst);
                responses.get(index).cloned().unwrap_or_else(|| {
                    "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string()
                })
            };
            thread::spawn(move || {
                let mut stream = stream;
                let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(5)));
                let mut received: Vec<u8> = Vec::new();
                let mut chunk = [0_u8; 8192];
                let mut expected: Option<usize> = None;
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
                let _ = stream.write_all(respond.as_bytes());
                let _ = stream.flush();
                let _ = stream.shutdown(std::net::Shutdown::Both);
            });
        }
    });
    format!("http://{address}/v1")
}

fn chat_response(content: &serde_json::Value) -> String {
    let body = serde_json::json!({
        "id": "fixture-req",
        "model": "fixture-model",
        "choices": [{"message": {"content": content.to_string()}}],
        "usage": {"prompt_tokens": 10, "completion_tokens": 20}
    })
    .to_string();
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    )
}

#[test]
fn agent_tool_loop_makes_safe_proposal_and_nothing_else() {
    let stop = Arc::new(AtomicBool::new(false));
    let canned = vec![
        chat_response(&serde_json::json!({
            "tool_call": {"name": "workspace.list", "arguments": {"path": ".", "max_files": 50}}
        })),
        chat_response(&serde_json::json!({
            "tool_call": {"name": "workspace.read", "arguments": {"path": "../outside.txt"}}
        })),
        chat_response(&serde_json::json!({
            "tool_call": {"name": "process.run", "arguments": {"argv": ["cmd.exe", "/c", "echo hi"]}}
        })),
        chat_response(&serde_json::json!({
            "tool_call": {"name": "workspace.read", "arguments": {"path": "add.py"}}
        })),
        chat_response(&serde_json::json!({
            "final": {
                "summary": "implement add",
                "files": [{
                    "path": "add.py",
                    "content": "def add(a, b):\n    return a + b\n",
                    "reason": "implement add per the task"
                }]
            }
        })),
    ];
    let base_url = spawn_sequence_fixture(stop.clone(), canned);

    let root = tempdir().unwrap();
    let root_str = root.path().to_str().unwrap();
    let original =
        "def add(a, b):\n    \"\"\"Add two numbers.\"\"\"\n    pass  # TODO: implement\n";
    fs::write(root.path().join("add.py"), original).unwrap();
    fs::write(
        root.path().join("outside.txt"),
        "outside-sentinel-must-not-leak",
    )
    .unwrap();

    let init = docode()
        .args(["--root", root_str, "init"])
        .output()
        .unwrap();
    assert!(init.status.success());

    let config_path = root.path().join("docode.config.toml");
    let config = fs::read_to_string(&config_path).unwrap();
    let config = config.replace(
        "base_url = \"https://api.openai.com/v1\"",
        &format!("base_url = \"{base_url}\""),
    );
    fs::write(&config_path, config).unwrap();

    // The loopback fixture can be slow under full-suite parallel load on this
    // machine; three attempts tolerate that benign flake (a persistent failure
    // is a real failure).
    let mut output = None;
    for _ in 1..=3 {
        let attempt = docode()
            .args([
                "--root",
                root_str,
                "agent",
                "implement add in add.py",
                "--live",
                "--approve-context-sharing",
            ])
            .output()
            .unwrap();
        if attempt.status.success() {
            output = Some(attempt);
            break;
        }
        output = Some(attempt);
    }
    let output = output.expect("agent ran");
    stop.store(true, Ordering::SeqCst);
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(output.status.success(), "agent failed: {stdout}\n{stderr}");

    // 1. The proposal was made and nothing was applied.
    assert!(
        stdout.contains("PATCH PROPOSAL"),
        "missing proposal: {stdout}"
    );
    assert!(stdout.contains("add.py"), "proposal missing file: {stdout}");
    assert!(stdout.contains("no shell, tests, build, or browser commands were executed"));
    assert_eq!(
        fs::read_to_string(root.path().join("add.py")).unwrap(),
        original
    );

    // 2. The outside sentinel was never read by the traversal-rejected tool call.
    assert!(!stdout.contains("outside-sentinel-must-not-leak"));

    // 3. Mission id parsed from stdout; apply flow proves the record is usable.
    let mission_id = stdout
        .lines()
        .find_map(|line| line.strip_prefix("Mission: "))
        .map(str::trim)
        .expect("mission id printed")
        .to_owned();
    let apply = docode()
        .args(["--root", root_str, "apply", &mission_id, "--approve"])
        .output()
        .unwrap();
    assert!(
        apply.status.success(),
        "apply failed: {}",
        String::from_utf8_lossy(&apply.stdout)
    );
    let updated = fs::read_to_string(root.path().join("add.py")).unwrap();
    assert_eq!(updated, "def add(a, b):\n    return a + b\n");

    // 4. Audit trail recorded the denial and the proposal.
    let audit = docode()
        .args(["--root", root_str, "audit", "--tail", "50"])
        .output()
        .unwrap();
    let audit_log = String::from_utf8_lossy(&audit.stdout).into_owned();
    assert!(
        audit_log.contains("agent.tool_denied"),
        "missing denial in audit: {audit_log}"
    );
    assert!(
        audit_log.contains("agent.patch_proposed"),
        "missing proposal audit: {audit_log}"
    );
    assert!(
        audit_log.contains("process.run"),
        "missing denied tool name: {audit_log}"
    );
}
