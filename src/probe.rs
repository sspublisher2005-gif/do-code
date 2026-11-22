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

/// Alternate spellings of the same host path: traversal from the container
/// cwd, extended-length prefix, NTFS alternate data stream, trailing dot or
/// space, and UNC share. Any successful read is a containment breach.
pub fn fs_03(argv: &[String]) -> i32 {
    let Some(path) = argv.first() else {
        return 21;
    };
    let mut variants = vec![
        path.clone(),
        path.replace('\\', "/"),
        format!("\\\\?\\{path}"),
        format!("{path}::$DATA"),
        format!("{path}."),
        format!("{path} "),
    ];
    // Walk up past the drive root, then re-append the absolute tail: the same
    // file reached through a long relative path.
    if path.len() > 2
        && let Ok(cwd) = std::env::current_dir()
    {
        let mut up = cwd;
        for _ in 0..32 {
            up = up.join("..");
        }
        let tail = path[2..].trim_start_matches(['\\', '/']);
        variants.push(up.join(tail).to_string_lossy().into_owned());
    }
    if path.len() > 2 && path.as_bytes()[1] == b':' {
        let drive = &path[..1];
        let tail = path[2..].trim_start_matches(['\\', '/']);
        variants.push(format!("\\\\localhost\\{drive}$\\{tail}"));
        variants.push(format!("\\\\127.0.0.1\\{drive}$\\{tail}"));
    }
    let mut breached = false;
    for variant in &variants {
        if let Ok(bytes) = std::fs::read(variant)
            && bytes.windows(10).any(|w| w == b"DECOY-SECRE")
        {
            println!("FS03:LEAK via {variant}");
            breached = true;
        }
    }
    if breached {
        20
    } else {
        println!("FS03-BLOCKED");
        0
    }
}

/// Symlink, hardlink, and check-to-use races pointing at a host secret.
/// Link creation being denied is containment, so only a read through a link
/// counts as a breach.
pub fn fs_04(argv: &[String]) -> i32 {
    let Some(path) = argv.first() else {
        return 21;
    };
    let Ok(cwd) = std::env::current_dir() else {
        return 21;
    };
    let mut breached = false;
    let leaked_through = |candidate: &std::path::Path, marker: &str| {
        if let Ok(bytes) = std::fs::read(candidate)
            && bytes.windows(10).any(|w| w == b"DECOY-SECRE")
        {
            println!("FS04:{marker}");
            return true;
        }
        false
    };

    #[cfg(windows)]
    {
        use std::os::windows::fs::symlink_file;
        let link = cwd.join("fs04-symlink");
        if symlink_file(path, &link).is_ok() && leaked_through(&link, "SYMLINK-LEAK") {
            breached = true;
        }
        let _ = std::fs::remove_file(&link);

        // TOCTOU: the link is created after the path was observed as absent.
        let swap = cwd.join("fs04-swap");
        let _ = std::fs::write(&swap, b"safe");
        let _ = std::fs::remove_file(&swap);
        if symlink_file(path, &swap).is_ok() && leaked_through(&swap, "TOCTOU-LEAK") {
            breached = true;
        }
        let _ = std::fs::remove_file(&swap);
    }

    // A hard link needs read access on the source, so denial is containment.
    let hard = cwd.join("fs04-hardlink");
    if std::fs::hard_link(path, &hard).is_ok() && leaked_through(&hard, "HARDLINK-LEAK") {
        breached = true;
    }
    let _ = std::fs::remove_file(&hard);

    if breached {
        20
    } else {
        println!("FS04-BLOCKED");
        0
    }
}

/// Device, raw volume, kernel and host service IPC content. A handle that
/// opens but yields no bytes is not a data breach; any byte read is.
pub fn fs_05(_argv: &[String]) -> i32 {
    use std::io::Read;
    const TARGETS: &[&str] = &[
        r"\\.\PhysicalDrive0",
        r"\\.\C:",
        r"\\.\GLOBALROOT\Device\HarddiskVolume1",
        r"\\?\GLOBALROOT\Device\KsecDD",
        r"\\.\pipe\lsass",
        r"\\.\pipe\winlogon",
        r"\\.\pipe\ntsvcs",
    ];
    let mut breached = false;
    for target in TARGETS {
        if let Ok(mut handle) = std::fs::File::open(target) {
            let mut byte = [0u8; 1];
            if matches!(handle.read(&mut byte), Ok(n) if n > 0) {
                println!("FS05:READ {target}");
                breached = true;
            } else {
                println!("FS05:OPEN-NO-DATA {target}");
            }
        }
    }
    if breached {
        20
    } else {
        println!("FS05-BLOCKED");
        0
    }
}

/// Bulk output first, then credential-shaped lines, then the done marker. The
/// host keeps only a tail, so the secrets must be in that tail to prove
/// redaction actually runs on what the caller sees.
pub fn log_01(_argv: &[String]) -> i32 {
    use std::io::Write;
    let line = "L".repeat(1_023) + "\n";
    let mut out = std::io::stdout().lock();
    for _ in 0..2_048 {
        if out.write_all(line.as_bytes()).is_err() {
            break;
        }
    }
    let _ = out.write_all(
        b"OPENAI_API_KEY=sk-A1b2C3d4E5f6G7h8I9j0K1l2M3n4\nAuthorization: Bearer log01-secret-token-value\nLOG01-DONE\n",
    );
    let _ = out.flush();
    0
}

/// Sleep until the job-object wall limit kills the tree. The host must report
/// a timeout, never a success, and stay usable afterwards.
pub fn life_01(_argv: &[String]) -> i32 {
    use std::io::Write;
    println!("LIFE01-START");
    let _ = std::io::stdout().flush();
    loop {
        std::thread::sleep(Duration::from_secs(1));
    }
}

/// Spawn a child that outlives this probe, print its pid, then exit. The Job
/// Object must kill the child when the sandbox process ends.
pub fn win_03(_argv: &[String]) -> i32 {
    let system_root = match std::env::var("SystemRoot") {
        Ok(value) => value,
        Err(_) => return 21,
    };
    let shell = std::path::Path::new(&system_root)
        .join("System32")
        .join("WindowsPowerShell")
        .join("v1.0")
        .join("powershell.exe");
    let child = std::process::Command::new(shell)
        .args(["-NoProfile", "-Command", "Start-Sleep -Seconds 60"])
        .spawn();
    match child {
        Ok(child) => {
            println!("PID:{}", child.id());
            0
        }
        Err(_) => 21,
    }
}

/// Report the container's own token. The runner asserts AppContainer,
/// low integrity, and no elevation — the properties the sandbox claims.
#[cfg(windows)]
pub fn win_02(_argv: &[String]) -> i32 {
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::Security::{
        GetSidSubAuthority, GetSidSubAuthorityCount, GetTokenInformation, TOKEN_ELEVATION,
        TOKEN_MANDATORY_LABEL, TOKEN_QUERY, TokenElevation, TokenIntegrityLevel,
        TokenIsAppContainer,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    let mut token: HANDLE = std::ptr::null_mut();
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return 21;
    }

    let mut container: u32 = 0;
    let mut len: u32 = 0;
    let ok = unsafe {
        GetTokenInformation(
            token,
            TokenIsAppContainer,
            (&mut container as *mut u32).cast(),
            std::mem::size_of::<u32>() as u32,
            &mut len,
        )
    };
    if ok == 0 {
        unsafe { CloseHandle(token) };
        return 21;
    }

    let mut elevated = TOKEN_ELEVATION::default();
    let ok = unsafe {
        GetTokenInformation(
            token,
            TokenElevation,
            (&mut elevated as *mut TOKEN_ELEVATION).cast(),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut len,
        )
    };
    if ok == 0 {
        unsafe { CloseHandle(token) };
        return 21;
    }

    let mut integrity: u32 = 0xffff_ffff;
    let mut buffer = [0u8; 128];
    let ok = unsafe {
        GetTokenInformation(
            token,
            TokenIntegrityLevel,
            buffer.as_mut_ptr().cast(),
            buffer.len() as u32,
            &mut len,
        )
    };
    if ok != 0 {
        let label = unsafe { &*(buffer.as_ptr() as *const TOKEN_MANDATORY_LABEL) };
        let sid = label.Label.Sid;
        if !sid.is_null() {
            let count = unsafe { *GetSidSubAuthorityCount(sid) };
            if count > 0 {
                integrity = unsafe { *GetSidSubAuthority(sid, (count - 1) as u32) };
            }
        }
    }
    unsafe { CloseHandle(token) };

    println!("WIN02:APPCONTAINER={container}");
    println!("WIN02:INTEGRITY=0x{integrity:04x}");
    println!("WIN02:ELEVATED={}", elevated.TokenIsElevated);
    println!("WIN02-DONE");
    0
}

/// Open the System and Idle processes. A container must never be able to.
#[cfg(windows)]
pub fn proc_02(_argv: &[String]) -> i32 {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::OpenProcess;

    const PROCESS_TERMINATE: u32 = 0x0001;
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    let mut breached = false;
    for (pid, label) in [(4u32, "system"), (0u32, "idle")] {
        let handle = unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE,
                0,
                pid,
            )
        };
        if !handle.is_null() {
            println!("PROC02:OPENED {label}");
            breached = true;
            unsafe { CloseHandle(handle) };
        }
    }
    if breached {
        20
    } else {
        println!("PROC02-BLOCKED");
        0
    }
}

#[cfg(not(windows))]
pub fn win_02(_argv: &[String]) -> i32 {
    21
}

#[cfg(not(windows))]
pub fn proc_02(_argv: &[String]) -> i32 {
    21
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
