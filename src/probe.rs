//! Native in-sandbox adversarial probes. These run INSIDE the AppContainer
//! through `docode sandbox exec` and are the only honest denial proof for
//! this build — shells/PowerShell cannot run inside the sandbox at all.
//!
//! Exit codes (the runner maps these to pass/fail):
//!   0  — containment held (the host denied the attempt, or the marker prints cleanly)
//!   20 — containment BREACHED (host did the thing the sandbox must prevent)
//!   21 — probe could not run (harness failure, not an attack signal)
//!
//! Every probe prints `<MARKER>` on stdout; the adversarial runner asserts on
//! those markers. A probe that cannot establish its precondition exits 21.

use std::time::Duration;

/// Read a host file argument and report whether the sandbox let it through.
/// argv[0] after the probe keyword: the absolute path to a host secret file.
pub fn fs_01(argv: &[String]) -> i32 {
    let Some(path) = argv.first() else {
        return 21;
    };
    match std::fs::read(path) {
        Ok(bytes) => {
            let leaked = bytes.windows(10).any(|w| w == b"DECOY-SECRE");
            if leaked {
                println!("FS01:LEAK");
            } else {
                println!("FS01:READ-HOST-FILE");
            }
            20
        }
        Err(_) => {
            println!("FS01-DONE");
            0
        }
    }
}

/// Attempt to create a host file OUTSIDE the workspace.
/// argv[0]: absolute host path to create.
pub fn fs_02(argv: &[String]) -> i32 {
    let Some(path) = argv.first() else {
        return 21;
    };
    match std::fs::File::create(path) {
        Ok(_) => {
            let _ = std::fs::remove_file(path);
            println!("FS02:ESCAPED");
            20
        }
        Err(_) => {
            println!("FS02-BLOCKED");
            0
        }
    }
}

/// Attempt outbound TCP to an external host on 443. Expects network denial.
pub fn net_01(_argv: &[String]) -> i32 {
    use std::net::ToSocketAddrs;
    let addrs = match "example.com:443".to_socket_addrs() {
        Ok(iter) => iter.collect::<Vec<_>>(),
        Err(_) => {
            println!("NET01:DENIED");
            return 0;
        }
    };
    for addr in addrs {
        if std::net::TcpStream::connect_timeout(&addr, Duration::from_secs(3)).is_ok() {
            println!("NET01:ESCAPED");
            return 20;
        }
    }
    println!("NET01:DENIED");
    0
}

/// Attempt loopback connect to the runner's listener. argv[0]: port number.
pub fn net_02(argv: &[String]) -> i32 {
    let Some(port_s) = argv.first() else {
        return 21;
    };
    let Ok(port) = port_s.parse::<u16>() else {
        return 21;
    };
    match std::net::TcpStream::connect_timeout(
        &([127, 0, 0, 1], port).into(),
        Duration::from_secs(3),
    ) {
        Ok(_) => {
            println!("NET02:ESCAPED");
            20
        }
        Err(_) => {
            println!("DENIED");
            0
        }
    }
}

/// Print the environment as the sandbox sees it; the runner asserts the
/// allowlisted vars are present and credential-shaped keys are absent.
pub fn env_01(_argv: &[String]) -> i32 {
    let mut names: Vec<String> = std::env::vars().map(|(k, _)| k).collect();
    names.sort_unstable();
    for name in names {
        println!("{name}");
    }
    println!("ENVLOOP-DONE");
    0
}

/// Echo argv (CMD-01). Any byte would be visible to the test runner; this
/// checks argument fidelity — nothing else.
pub fn cmd_01(argv: &[String]) -> i32 {
    let arg = argv.first().cloned().unwrap_or_default();
    println!("CMD01:{arg}");
    0
}

/// Allocate memory forever; the job-object wall limit is the kill signal.
pub fn res_01(_argv: &[String]) -> i32 {
    let mut junk: Vec<u8> = Vec::new();
    loop {
        junk.extend_from_slice(&vec![0x41u8; 1_000_000]);
    }
}

/// Parent end for PROC-01: spawn a nested process, print its pid, wait for it,
/// exit. The runaway/RES case covers kills; this proves nested creation works
/// and lands in the Job Object. The child is a System32 image because the
/// container is denied read on our own binary — a system image is readable by
/// every AppContainer by design.
pub fn proc_01(_argv: &[String]) -> i32 {
    let system_root = match std::env::var("SystemRoot") {
        Ok(v) => v,
        Err(_) => return 21,
    };
    let exe = std::path::Path::new(&system_root)
        .join("System32")
        .join("whoami.exe");
    let child = std::process::Command::new(exe).spawn();
    match child {
        Ok(mut child) => {
            println!("PID:{}", child.id());
            let _ = child.wait();
            0
        }
        Err(_) => 21,
    }
}
