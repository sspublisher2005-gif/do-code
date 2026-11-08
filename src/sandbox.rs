//! Native OS sandbox primitives. This module is intentionally fail-closed:
//! every public entry point returns an error on platforms or configurations
//! where the required primitive cannot be established.
//!
//! Current status:
//!   - Windows: Job Object containment + AppContainer launch (zero capabilities =
//!     OS network deny) + sanitized minimal environment + suspended process
//!     creation implemented. No unprivileged primary-token API exists —
//!     PROC_THREAD_ATTRIBUTE_TOKEN is absent from the documented attribute
//!     table, and CreateProcessWithTokenW requires SeImpersonatePrivilege.
//!     Full adversarial proof is still pending; command execution stays
//!     disabled until `sandbox self-test` passes on a real disposable
//!     Windows runner.
//!   - Linux / macOS: not yet implemented; execution stays disabled.

#[cfg(windows)]
use crate::error::{DocodeError, Result};

#[cfg(windows)]
mod windows_impl {
    #![allow(dead_code)]
    use super::*;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, LocalFree};
    use windows_sys::Win32::Security::Isolation::CreateAppContainerProfile;
    use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JobObjectExtendedLimitInformation, SetInformationJobObject, TerminateJobObject,
    };
    use windows_sys::Win32::System::Memory::{LMEM_FIXED, LocalAlloc};
    use windows_sys::Win32::System::Pipes::CreatePipe;
    use windows_sys::Win32::System::Threading::{
        CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT, CreateProcessW, GetExitCodeProcess,
        PROCESS_INFORMATION, ResumeThread, STARTF_USESTDHANDLES, TerminateProcess,
        WaitForSingleObject,
    };

    /// Conservative resource limits for a sandboxed child process tree.
    #[derive(Clone, Copy, Debug)]
    pub struct ResourceLimits {
        pub max_wall_time_ms: u32,
        pub max_memory_bytes: usize,
        pub max_processes: u32,
        pub max_output_bytes: usize,
    }

    impl Default for ResourceLimits {
        fn default() -> Self {
            Self {
                max_wall_time_ms: 120_000,
                max_memory_bytes: 512 * 1024 * 1024,
                max_processes: 64,
                max_output_bytes: 2 * 1024 * 1024,
            }
        }
    }

    /// A Job Object handle that kills all contained processes on drop.
    pub struct JobObject {
        handle: HANDLE,
    }

    impl JobObject {
        pub fn create(limits: ResourceLimits) -> Result<Self> {
            let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
            if handle.is_null() {
                return Err(DocodeError::Safety(
                    "CreateJobObjectW failed; cannot establish process containment.".into(),
                ));
            }
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
            info.BasicLimitInformation.LimitFlags = 0x00002000 // JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
                | 0x00000020 // JOB_OBJECT_LIMIT_ACTIVE_PROCESS
                | 0x00000008; // JOB_OBJECT_LIMIT_PROCESS_MEMORY
            info.BasicLimitInformation.ActiveProcessLimit = limits.max_processes;
            info.ProcessMemoryLimit = limits.max_memory_bytes;
            let ok = unsafe {
                SetInformationJobObject(
                    handle,
                    JobObjectExtendedLimitInformation,
                    &info as *const _ as *const _,
                    std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                )
            };
            if ok == 0 {
                unsafe { CloseHandle(handle) };
                return Err(DocodeError::Safety(
                    "SetInformationJobObject failed; cannot enforce resource limits.".into(),
                ));
            }
            Ok(Self { handle })
        }

        pub fn assign(&self, process_handle: HANDLE) -> Result<()> {
            let ok = unsafe { AssignProcessToJobObject(self.handle, process_handle) };
            if ok == 0 {
                return Err(DocodeError::Safety(
                    "AssignProcessToJobObject failed; refusing to start unsandboxed process."
                        .into(),
                ));
            }
            Ok(())
        }

        pub fn handle(&self) -> HANDLE {
            self.handle
        }
    }

    impl Drop for JobObject {
        fn drop(&mut self) {
            if !self.handle.is_null() {
                unsafe { CloseHandle(self.handle) };
            }
        }
    }

    /// Create (or reuse) the Do Code AppContainer profile and return its SID as a raw pointer.
    fn appcontainer_sid_ptr() -> Result<*mut std::ffi::c_void> {
        let name: Vec<u16> = "DocodeSandbox\0".encode_utf16().collect();
        let display: Vec<u16> = "Docode Sandbox\0".encode_utf16().collect();
        let desc: Vec<u16> = "Do Code sandbox container\0".encode_utf16().collect();
        let mut sid: *mut std::ffi::c_void = std::ptr::null_mut();
        let hr = unsafe {
            CreateAppContainerProfile(
                name.as_ptr(),
                display.as_ptr(),
                desc.as_ptr(),
                std::ptr::null(),
                0,
                &mut sid,
            )
        };
        // S_OK (0) or APPCONTAINER_PROFILE_ALREADY_EXISTS (HRESULT 0x800700B7 = -2147024713).
        if hr != 0 && hr != -2147024713_i32 {
            return Err(DocodeError::Safety(format!(
                "CreateAppContainerProfile failed (hr=0x{hr:08x})."
            )));
        }
        if sid.is_null() {
            // Profile already existed; derive its SID.
            let mut derived: *mut std::ffi::c_void = std::ptr::null_mut();
            let hr = unsafe {
                windows_sys::Win32::Security::Isolation::DeriveAppContainerSidFromAppContainerName(
                    name.as_ptr(),
                    &mut derived,
                )
            };
            if hr != 0 {
                return Err(DocodeError::Safety(format!(
                    "DeriveAppContainerSidFromAppContainerName failed (hr=0x{hr:08x})."
                )));
            }
            sid = derived;
        }
        Ok(sid)
    }

    /// Sanitized environment block for the child: "KEY=VALUE" pairs, each
    /// null-terminated, list terminated by an extra null. Allowlist only:
    /// SystemRoot/TEMP/TMP/SystemDrive + runtime essentials that AppContainer
    /// needs to spawn without ERROR 203.
    pub fn sanitized_environment(workspace: &std::path::Path) -> Vec<u16> {
        const ALLOWLIST: &[&str] = &[
            "SystemRoot",
            "SystemDrive",
            "TEMP",
            "TMP",
            "PATHEXT",
            "COMPUTERNAME",
            "PROCESSOR_ARCHITECTURE",
            "PROCESSOR_IDENTIFIER",
            "PROCESSOR_REVISION",
            "OS",
            "NUMBER_OF_PROCESSORS",
            "USERPROFILE",
            "HOMEDRIVE",
            "HOMEPATH",
            "LOCALAPPDATA",
            "APPDATA",
            "PSModulePath",
            "PATH",
        ];
        let mut block: Vec<u16> = Vec::new();
        for key in ALLOWLIST {
            if let Ok(value) = std::env::var(key) {
                if crate::redaction::redact(&value) != value {
                    continue;
                }
                block.extend(format!("{key}={value}").encode_utf16());
                block.push(0);
            }
        }
        let ws_line = format!("DOCODE_WORKSPACE={}", workspace.display());
        block.extend(ws_line.encode_utf16());
        block.push(0);
        // In-sandbox marker so the hidden `docode sandbox probe` subcommand
        // can distinguish "running inside the AppContainer" from
        // "an end user invoked it on their machine".
        block.extend("DOCODE_PROBE_VIA_SANDBOX=1".encode_utf16());
        block.push(0);
        block.push(0); // double-null terminator
        block
    }

    /// Spawn a process suspended inside an AppContainer with:
    /// Create the private disposable workspace for one sandbox run: a fresh
    /// directory under the project's `.docode/sandbox/` directory whose DACL
    /// grants the container SID full control, protected so nothing broad is
    /// inherited. This is the only directory the sandboxed process can write.
    ///
    /// Located under the project instead of %TEMP%: some hardened Windows
    /// images deny AppContainer traversal on temp roots, which makes any
    /// %TEMP%-based workspace unusable there.
    pub fn create_container_workspace(
        container_sid: *mut std::ffi::c_void,
        parent: &std::path::Path,
    ) -> Result<std::path::PathBuf> {
        use windows_sys::Win32::Security::Authorization::SE_FILE_OBJECT;
        use windows_sys::Win32::Security::Authorization::{
            EXPLICIT_ACCESS_W, GRANT_ACCESS, SetEntriesInAclW, SetNamedSecurityInfoW,
            TRUSTEE_IS_SID, TRUSTEE_IS_UNKNOWN, TRUSTEE_W,
        };
        use windows_sys::Win32::Security::DACL_SECURITY_INFORMATION;
        // Parent (e.g. .docode/sandbox/) must exist and be private before use.
        crate::storage::ensure_private_dir(parent)?;
        let dir = parent.join(format!("run-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir(&dir).map_err(|e| crate::error::io_error(&dir, e))?;
        let mut wide: Vec<u16> = dir
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let trustee = TRUSTEE_W {
            pMultipleTrustee: std::ptr::null_mut(),
            MultipleTrusteeOperation: 0,
            TrusteeForm: TRUSTEE_IS_SID,
            TrusteeType: TRUSTEE_IS_UNKNOWN,
            ptstrName: container_sid as *mut _,
        };
        let entry = EXPLICIT_ACCESS_W {
            grfAccessPermissions: 0x1F01FF, // file-generic all
            grfAccessMode: GRANT_ACCESS,
            grfInheritance: 0x3, // sub-containers and objects inherit
            Trustee: trustee,
        };
        // ADD the container ACE to the existing DACL instead of replacing it:
        // the child token carries the user SID alongside the container SID,
        // and keeping the existing ACEs (owner/SYSTEM/user) avoids the
        // elevated-runner case where a container-only DACL denied the child's
        // own cwd.
        let mut current_sd: *mut std::ffi::c_void = std::ptr::null_mut();
        let mut current_dacl: *mut windows_sys::Win32::Security::ACL = std::ptr::null_mut();
        let get_err = unsafe {
            windows_sys::Win32::Security::Authorization::GetNamedSecurityInfoW(
                wide.as_mut_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut current_dacl,
                std::ptr::null_mut(),
                &mut current_sd,
            )
        };
        if get_err != 0 {
            return Err(DocodeError::Safety(format!(
                "GetNamedSecurityInfoW (workspace read) failed (err={get_err})."
            )));
        }
        let mut new_dacl: *mut windows_sys::Win32::Security::ACL = std::ptr::null_mut();
        let hr = unsafe { SetEntriesInAclW(1, &entry, current_dacl, &mut new_dacl) };
        if hr != 0 || new_dacl.is_null() {
            return Err(DocodeError::Safety(format!(
                "SetEntriesInAclW (workspace grant) failed (hr={hr})."
            )));
        }
        let err = unsafe {
            SetNamedSecurityInfoW(
                wide.as_mut_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                new_dacl,
                std::ptr::null_mut(),
            )
        };
        unsafe {
            LocalFree(new_dacl as *mut _);
            LocalFree(current_sd);
        }
        if err != 0 {
            return Err(DocodeError::Safety(format!(
                "SetNamedSecurityInfoW (workspace grant) failed (err={err})."
            )));
        }
        Ok(dir)
    }

    ///   1. restricted token (no privileges) — defense in depth,
    ///   2. AppContainer SID with ZERO capabilities → OS-enforced network deny
    ///      (no internetClient capability granted) and filesystem deny-by-default,
    ///   3. sanitized minimal environment (no secrets inherited),
    ///   4. Job Object assigned BEFORE resume with kill-on-close.
    ///
    /// Any failure before resume terminates the child and returns an error.
    /// Returns the handles plus the private disposable workspace path — the
    /// caller owns cleanup (`remove_dir_all`) once the child exits.
    pub fn spawn_suspended(
        exe: &str,
        args: &[String],
        _limits: ResourceLimits,
        job: &JobObject,
        workspace_parent: &std::path::Path,
    ) -> Result<(HANDLE, HANDLE, u32, std::path::PathBuf, HANDLE)> {
        use windows_sys::Win32::Security::SECURITY_CAPABILITIES;
        use windows_sys::Win32::System::Threading::{
            DeleteProcThreadAttributeList, EXTENDED_STARTUPINFO_PRESENT,
            InitializeProcThreadAttributeList, PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES,
            STARTUPINFOEXW, UpdateProcThreadAttribute,
        };

        // 1. KNOWN GAP: restricted primary token is not wired yet —
        // windows-sys 0.61 does not export PROC_THREAD_ATTRIBUTE_TOKEN. The
        // AppContainer SID (zero capabilities) plus Job Object plus the
        // workspace DACL carry the enforcement until it lands.

        // 2. AppContainer SID with zero capabilities (network denied by OS).
        let sid = appcontainer_sid_ptr()?;
        let sec_caps = SECURITY_CAPABILITIES {
            AppContainerSid: sid as *mut _,
            Capabilities: std::ptr::null_mut(),
            CapabilityCount: 0,
            Reserved: 0,
        };

        // 3. Attribute list carrying the security-capabilities attribute.
        const ATTR_COUNT: usize = 1;
        let mut attr_size: usize = 0;
        unsafe {
            InitializeProcThreadAttributeList(
                std::ptr::null_mut(),
                ATTR_COUNT as u32,
                0,
                &mut attr_size,
            );
        }
        if attr_size == 0 {
            return Err(DocodeError::Safety(
                "InitializeProcThreadAttributeList size query failed; no process started.".into(),
            ));
        }
        let attr_list: *mut std::ffi::c_void = unsafe { LocalAlloc(LMEM_FIXED, attr_size) };
        if attr_list.is_null() {
            return Err(DocodeError::Safety(
                "allocating proc-thread attribute list failed; no process started.".into(),
            ));
        }
        let ok = unsafe {
            InitializeProcThreadAttributeList(attr_list, ATTR_COUNT as u32, 0, &mut attr_size)
        };
        if ok == 0 {
            unsafe { LocalFree(attr_list as _) };
            return Err(DocodeError::Safety(
                "InitializeProcThreadAttributeList failed; no process started.".into(),
            ));
        }
        let caps_value = &sec_caps as *const _ as *const core::ffi::c_void;
        let ok = unsafe {
            UpdateProcThreadAttribute(
                attr_list,
                0,
                PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES as usize,
                caps_value,
                std::mem::size_of::<SECURITY_CAPABILITIES>(),
                std::ptr::null_mut(),
                std::ptr::null(),
            )
        };
        if ok == 0 {
            unsafe {
                DeleteProcThreadAttributeList(attr_list);
                LocalFree(attr_list as _);
            }
            return Err(DocodeError::Safety(
                "UpdateProcThreadAttribute (security capabilities) failed; sandbox cannot start."
                    .into(),
            ));
        }

        // 4. Extended startup info with the attribute list.
        let mut startup_info_ex: STARTUPINFOEXW = unsafe { std::mem::zeroed() };
        startup_info_ex.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
        startup_info_ex.lpAttributeList = attr_list;

        // 5. Command line: structured argv, quoted, no shell interpolation.
        let mut cmd_line = format!("\"{exe}\"");
        for arg in args {
            cmd_line.push(' ');
            if arg.contains(' ') || arg.contains('"') {
                cmd_line.push('"');
                cmd_line.push_str(&arg.replace('"', "\\\""));
                cmd_line.push('"');
            } else {
                cmd_line.push_str(arg);
            }
        }
        let mut cmd_wide: Vec<u16> = cmd_line.encode_utf16().chain(std::iter::once(0)).collect();

        // 6. Private disposable workspace (granted to the container SID) and
        // a sanitized environment that names it. The explicit current
        // directory matters: with lpCurrentDirectory=NULL and a small custom
        // environment, CreateProcessW resolves the child's per-drive cwd from
        // hidden "=X:" entries the block does not carry and fails with
        // ERROR_ENVVAR_NOT_FOUND (203).
        let workspace = create_container_workspace(sid, workspace_parent)?;
        let mut env_block = sanitized_environment(&workspace);
        let mut cwd_wide: Vec<u16> = workspace
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        // 6b. Merged stdout/stderr pipe for the child. Without an explicit
        // inherited handle an AppContainer child has no console and the
        // runner sees NOTHING — every output assertion would be blind.
        let mut sec_attrs: SECURITY_ATTRIBUTES = unsafe { std::mem::zeroed() };
        sec_attrs.nLength = std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32;
        sec_attrs.bInheritHandle = 1;
        sec_attrs.lpSecurityDescriptor = std::ptr::null_mut();
        let mut stdout_read: HANDLE = std::ptr::null_mut();
        let mut stdout_write: HANDLE = std::ptr::null_mut();
        let pipe_ok = unsafe { CreatePipe(&mut stdout_read, &mut stdout_write, &sec_attrs, 0) };
        if pipe_ok == 0 {
            let _ = std::fs::remove_dir_all(&workspace);
            return Err(DocodeError::Safety(
                "CreatePipe failed; sandbox cannot capture child output.".into(),
            ));
        }
        startup_info_ex.StartupInfo.hStdOutput = stdout_write;
        startup_info_ex.StartupInfo.hStdError = stdout_write;
        startup_info_ex.StartupInfo.hStdInput = std::ptr::null_mut();
        startup_info_ex.StartupInfo.dwFlags |= STARTF_USESTDHANDLES;

        // 7. Create process suspended inside the AppContainer.
        let mut proc_info: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
        let ok = unsafe {
            CreateProcessW(
                std::ptr::null(),
                cmd_wide.as_mut_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                1, // inherit; only handles marked inheritable (the pipe) pass
                CREATE_SUSPENDED | EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT,
                env_block.as_mut_ptr().cast(),
                cwd_wide.as_mut_ptr(),
                &startup_info_ex.StartupInfo,
                &mut proc_info,
            )
        };
        // The attribute list is consumed by CreateProcessW; clean it up regardless.
        unsafe {
            DeleteProcThreadAttributeList(attr_list);
            LocalFree(attr_list as _);
        }

        if ok == 0 {
            let last = unsafe { windows_sys::Win32::Foundation::GetLastError() };
            unsafe {
                CloseHandle(stdout_read);
                CloseHandle(stdout_write);
            }
            let _ = std::fs::remove_dir_all(&workspace);
            return Err(DocodeError::Safety(format!(
                "CreateProcessW (suspended, AppContainer) failed; no process was started. (GetLastError={last})"
            )));
        }

        // 8. Assign to job BEFORE resuming (kill-on-close containment).
        if let Err(e) = job.assign(proc_info.hProcess) {
            unsafe {
                TerminateProcess(proc_info.hProcess, 1);
                CloseHandle(proc_info.hThread);
                CloseHandle(proc_info.hProcess);
                CloseHandle(stdout_read);
                CloseHandle(stdout_write);
            }
            let _ = std::fs::remove_dir_all(&workspace);
            return Err(e);
        }

        // 9. Resume the suspended main thread.
        let resumed = unsafe { ResumeThread(proc_info.hThread) };
        if resumed == u32::MAX {
            unsafe {
                TerminateProcess(proc_info.hProcess, 1);
                CloseHandle(proc_info.hThread);
                CloseHandle(proc_info.hProcess);
                CloseHandle(stdout_read);
                CloseHandle(stdout_write);
            }
            let _ = std::fs::remove_dir_all(&workspace);
            return Err(DocodeError::Safety(
                "ResumeThread failed; process terminated before execution.".into(),
            ));
        }

        unsafe {
            CloseHandle(stdout_write);
        }
        Ok((
            proc_info.hProcess,
            proc_info.hThread,
            proc_info.dwProcessId,
            workspace,
            stdout_read,
        ))
    }

    /// Wait for a sandboxed process to complete, with timeout, capturing its
    /// output. Returns (exit_code, timed_out, merged stdout+stderr bytes,
    /// capped at 1 MiB). On timeout the WHOLE process tree is terminated via
    /// the Job Object.
    pub fn wait_for_completion(
        process_handle: HANDLE,
        thread_handle: HANDLE,
        timeout_ms: u32,
        job: &JobObject,
        stdout_pipe: HANDLE,
    ) -> Result<(u32, bool, Vec<u8>)> {
        const WAIT_TIMEOUT: u32 = 0x0000_0102;
        const MAX_CAPTURE: usize = 1_048_576;
        // Drain the pipe on a helper thread while the child runs; a full
        // pipe would otherwise wedge the child.
        let stdout_addr = stdout_pipe as usize;
        let reader = std::thread::spawn(move || {
            let stdout_pipe = stdout_addr as HANDLE;
            let mut captured = Vec::with_capacity(8_192);
            loop {
                let mut chunk = [0u8; 8_192];
                let mut read_bytes: u32 = 0;
                let ok = unsafe {
                    windows_sys::Win32::Storage::FileSystem::ReadFile(
                        stdout_pipe,
                        chunk.as_mut_ptr(),
                        chunk.len() as u32,
                        &mut read_bytes,
                        std::ptr::null_mut(),
                    )
                };
                if ok == 0 || read_bytes == 0 {
                    break;
                }
                let n = read_bytes as usize;
                if captured.len() < MAX_CAPTURE {
                    let take = (MAX_CAPTURE - captured.len()).min(n);
                    captured.extend_from_slice(&chunk[..take]);
                }
            }
            unsafe {
                CloseHandle(stdout_pipe);
            }
            captured
        });
        let result = unsafe { WaitForSingleObject(process_handle, timeout_ms) };
        let timed_out = result == WAIT_TIMEOUT;
        if timed_out {
            let killed = unsafe { TerminateJobObject(job.handle(), 1) };
            if killed == 0 {
                let _ = unsafe { TerminateProcess(process_handle, 1) };
            }
            let _ = unsafe { WaitForSingleObject(process_handle, 5000) };
        } else if result == 0 {
            // Normal exit: fall through to code read below.
        } else {
            // Unexpected wait failure: stop the tree fail-safe, then report.
            let _ = unsafe { TerminateJobObject(job.handle(), 1) };
            let _ = unsafe { WaitForSingleObject(process_handle, 5000) };
            unsafe {
                CloseHandle(thread_handle);
                CloseHandle(process_handle);
            }
            let _ = reader.join();
            return Err(DocodeError::Safety(format!(
                "WaitForSingleObject failed (result=0x{result:08x})."
            )));
        }
        let exit_code = if timed_out {
            1
        } else {
            let mut exit_code: u32 = 0;
            unsafe { GetExitCodeProcess(process_handle, &mut exit_code) };
            if exit_code == 0 { 0 } else { exit_code }
        };
        unsafe {
            CloseHandle(thread_handle);
            CloseHandle(process_handle);
        }
        let captured = reader.join().unwrap_or_default();
        Ok((exit_code, timed_out, captured))
    }
}

#[cfg(windows)]
#[allow(unused_imports)]
pub use windows_impl::*;

/// A qualified sandbox has passed the adversarial acceptance matrix on a
/// disposable native runner. Do Code has not run those tests, so this stays
/// false and arbitrary command execution stays disabled. Do not flip this
/// to true without retained evidence from the 26-case matrix.
pub fn is_qualified() -> bool {
    false
}

pub fn status_text() -> String {
    if is_qualified() {
        "Sandbox: available (qualified)".to_string()
    } else if cfg!(windows) {
        "Sandbox: primitives implemented (AppContainer + Job Object + private workspace          DACL + sanitized env + OS network deny), but NOT qualified — adversarial acceptance          tests have not passed on a disposable runner yet. Arbitrary command execution is disabled."
            .to_string()
    } else {
        "Sandbox: not implemented on this platform; arbitrary command execution is disabled.          Windows driver exists; Linux (Bubblewrap+Landlock+seccomp) and macOS (Seatbelt) are          designed but not implemented."
            .to_string()
    }
}
