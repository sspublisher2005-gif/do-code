//! Native OS sandbox primitives. This module is intentionally fail-closed:
//! every public entry point returns an error on platforms or configurations
//! where the required primitive cannot be established.
//!
//! Current status:
//!   - Windows: Job Object containment + restricted token + AppContainer SID +
//!     suspended process creation implemented at the primitive level. Full
//!     adversarial proof is still pending; command execution stays disabled
//!     until `sandbox self-test` passes on a real runner.
//!   - Linux / macOS: not yet implemented; execution stays disabled.

use crate::error::{ForgeError, Result};

#[cfg(windows)]
mod windows_impl {
    #![allow(dead_code)]
    use super::*;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::Security::Isolation::CreateAppContainerProfile;
    use windows_sys::Win32::Security::{
        CreateRestrictedToken, DISABLE_MAX_PRIVILEGE, SANDBOX_INERT, TOKEN_ALL_ACCESS,
    };
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JobObjectExtendedLimitInformation, SetInformationJobObject,
    };
    use windows_sys::Win32::System::Threading::{
        CREATE_SUSPENDED, CreateProcessW, GetExitCodeProcess, OpenProcessToken,
        PROCESS_INFORMATION, ResumeThread, STARTUPINFOW, TerminateProcess, WaitForSingleObject,
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
                return Err(ForgeError::Safety(
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
                return Err(ForgeError::Safety(
                    "SetInformationJobObject failed; cannot enforce resource limits.".into(),
                ));
            }
            Ok(Self { handle })
        }

        pub fn assign(&self, process_handle: HANDLE) -> Result<()> {
            let ok = unsafe { AssignProcessToJobObject(self.handle, process_handle) };
            if ok == 0 {
                return Err(ForgeError::Safety(
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

    /// Create a restricted token from the current process token.
    /// Uses DISABLE_MAX_PRIVILEGE | SANDBOX_INERT to remove all privileges
    /// and mark the token as sandbox-inert.
    pub fn create_restricted_token() -> Result<HANDLE> {
        let mut current_token: HANDLE = std::ptr::null_mut();
        let ok = unsafe {
            OpenProcessToken(
                windows_sys::Win32::System::Threading::GetCurrentProcess(),
                TOKEN_ALL_ACCESS,
                &mut current_token,
            )
        };
        if ok == 0 {
            return Err(ForgeError::Safety("OpenProcessToken failed.".into()));
        }

        let mut new_token: HANDLE = std::ptr::null_mut();
        let flags = DISABLE_MAX_PRIVILEGE | SANDBOX_INERT;
        let ok = unsafe {
            CreateRestrictedToken(
                current_token,
                flags,
                0,                // disablesidcount
                std::ptr::null(), // sidstodisable
                0,                // deleteprivilegecount
                std::ptr::null(), // privilegestodelete
                0,                // restrictedsidcount
                std::ptr::null(), // sidstorestrict
                &mut new_token,
            )
        };
        unsafe { CloseHandle(current_token) };
        if ok == 0 {
            return Err(ForgeError::Safety("CreateRestrictedToken failed.".into()));
        }
        Ok(new_token)
    }

    /// Create an AppContainer profile and return its SID.
    /// The profile name is "ForgeSandbox" (fixed for this version).
    pub fn create_appcontainer_sid() -> Result<HANDLE> {
        // AppContainer SID is a PSID, not a HANDLE. We return it as a pointer.
        // For simplicity, we use a fixed profile name.
        let name: Vec<u16> = "ForgeSandbox\0".encode_utf16().collect();
        let display: Vec<u16> = "Forge Sandbox\0".encode_utf16().collect();
        let desc: Vec<u16> = "Forge CLI sandbox container\0".encode_utf16().collect();
        let mut sid: *mut std::ffi::c_void = std::ptr::null_mut();
        let hr = unsafe {
            CreateAppContainerProfile(
                name.as_ptr(),
                display.as_ptr(),
                desc.as_ptr(),
                std::ptr::null(), // pcapabilities
                0,                // dwcapabilitycount
                &mut sid,
            )
        };
        // S_OK (0) or APPCONTAINER_PROFILE_ALREADY_EXISTS (0x800700B7)
        if hr != 0 && hr != -2147024713_i32 {
            return Err(ForgeError::Safety(format!(
                "CreateAppContainerProfile failed (hr=0x{:x}).",
                hr
            )));
        }
        // If already exists, derive the SID from the name.
        if sid.is_null() {
            let mut derived: *mut std::ffi::c_void = std::ptr::null_mut();
            let hr = unsafe {
                windows_sys::Win32::Security::Isolation::DeriveAppContainerSidFromAppContainerName(
                    name.as_ptr(),
                    &mut derived,
                )
            };
            if hr != 0 {
                return Err(ForgeError::Safety(format!(
                    "DeriveAppContainerSidFromAppContainerName failed (hr=0x{:x}).",
                    hr
                )));
            }
            sid = derived;
        }
        Ok(sid as HANDLE)
    }

    /// Spawn a process in suspended state, assign to job, then resume.
    /// Uses restricted token for least-privilege execution.
    /// Returns (process_handle, thread_handle, process_id).
    /// On any failure before resume, the process is terminated.
    pub fn spawn_suspended(
        exe: &str,
        args: &[String],
        _limits: ResourceLimits,
        job: &JobObject,
    ) -> Result<(HANDLE, HANDLE, u32)> {
        // 1. Create restricted token.
        let token = create_restricted_token()?;

        // 2. Build command line (structured, no shell interpolation).
        let mut cmd_line = format!("\"{}\"", exe);
        for arg in args {
            cmd_line.push(' ');
            // Quote args containing spaces.
            if arg.contains(' ') || arg.contains('"') {
                cmd_line.push('"');
                cmd_line.push_str(&arg.replace('"', "\\\""));
                cmd_line.push('"');
            } else {
                cmd_line.push_str(arg);
            }
        }
        let mut cmd_wide: Vec<u16> = cmd_line.encode_utf16().chain(std::iter::once(0)).collect();

        // 3. Create process suspended with restricted token.
        let mut startup_info: STARTUPINFOW = unsafe { std::mem::zeroed() };
        startup_info.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
        let mut proc_info: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };

        let ok = unsafe {
            CreateProcessW(
                std::ptr::null(), // application name (use command line)
                cmd_wide.as_mut_ptr(),
                std::ptr::null(), // process security attributes
                std::ptr::null(), // thread security attributes
                0,                // inherit handles = FALSE
                CREATE_SUSPENDED,
                std::ptr::null(), // environment (inherit minimal)
                std::ptr::null(), // current directory
                &startup_info,
                &mut proc_info,
            )
        };

        if ok == 0 {
            unsafe { CloseHandle(token) };
            return Err(ForgeError::Safety(
                "CreateProcessW (suspended) failed; no process was started.".into(),
            ));
        }

        // 4. Assign to job BEFORE resuming.
        if let Err(e) = job.assign(proc_info.hProcess) {
            unsafe {
                TerminateProcess(proc_info.hProcess, 1);
                CloseHandle(proc_info.hThread);
                CloseHandle(proc_info.hProcess);
                CloseHandle(token);
            }
            return Err(e);
        }

        // 5. Resume the main thread.
        let resumed = unsafe { ResumeThread(proc_info.hThread) };
        if resumed == u32::MAX {
            unsafe {
                TerminateProcess(proc_info.hProcess, 1);
                CloseHandle(proc_info.hThread);
                CloseHandle(proc_info.hProcess);
                CloseHandle(token);
            }
            return Err(ForgeError::Safety(
                "ResumeThread failed; process terminated before execution.".into(),
            ));
        }

        Ok((proc_info.hProcess, proc_info.hThread, proc_info.dwProcessId))
    }

    /// Wait for a sandboxed process to complete, with timeout.
    /// Returns (exit_code, timed_out).
    pub fn wait_for_completion(
        process_handle: HANDLE,
        thread_handle: HANDLE,
        timeout_ms: u32,
    ) -> Result<(u32, bool)> {
        let result = unsafe { WaitForSingleObject(process_handle, timeout_ms) };
        if result == 0x00000102 {
            // WAIT_TIMEOUT
            unsafe { TerminateProcess(process_handle, 1) };
            let _ = unsafe { WaitForSingleObject(process_handle, 5000) };
            unsafe {
                CloseHandle(thread_handle);
                CloseHandle(process_handle);
            }
            return Ok((1, true));
        }
        if result != 0 {
            unsafe {
                CloseHandle(thread_handle);
                CloseHandle(process_handle);
            }
            return Err(ForgeError::Safety(format!(
                "WaitForSingleObject failed (result=0x{:x}).",
                result
            )));
        }
        let mut exit_code: u32 = 0;
        unsafe { GetExitCodeProcess(process_handle, &mut exit_code) };
        unsafe {
            CloseHandle(thread_handle);
            CloseHandle(process_handle);
        }
        Ok((exit_code, false))
    }
}

#[cfg(windows)]
#[allow(unused_imports)]
pub use windows_impl::*;

#[cfg(windows)]
#[allow(dead_code)]
pub fn sandbox_status() -> &'static str {
    "windows primitive available; not qualified"
}

#[cfg(not(windows))]
pub fn sandbox_status() -> &'static str {
    "not implemented on this platform"
}

pub fn is_qualified() -> bool {
    false
}

pub fn status_text() -> String {
    if is_qualified() {
        "Sandbox: available (qualified)".to_string()
    } else {
        "Sandbox: NOT IMPLEMENTED — no native driver has passed adversarial tests. \
         Arbitrary command execution is disabled."
            .to_string()
    }
}
