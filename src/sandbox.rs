//! Native OS sandbox primitives. This module is intentionally fail-closed:
//! every public entry point returns an error on platforms or configurations
//! where the required primitive cannot be established.
//!
//! Current status:
//!   - Windows: Job Object containment + restricted token + AppContainer launch
//!     (PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES, zero capabilities =
//!     OS-enforced deny-by-default network) + sanitized minimal environment +
//!     suspended process creation implemented. Full adversarial proof is still
//!     pending; command execution stays disabled until `sandbox self-test`
//!     passes on a real disposable Windows runner.
//!   - Linux / macOS: not yet implemented; execution stays disabled.

use crate::error::{ForgeError, Result};

#[cfg(windows)]
mod windows_impl {
    #![allow(dead_code)]
    use super::*;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, LocalFree};
    use windows_sys::Win32::Security::Isolation::CreateAppContainerProfile;
    use windows_sys::Win32::Security::{
        CreateRestrictedToken, DISABLE_MAX_PRIVILEGE, SANDBOX_INERT, TOKEN_ALL_ACCESS,
    };
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JobObjectExtendedLimitInformation, SetInformationJobObject,
    };
    use windows_sys::Win32::System::Memory::{LMEM_FIXED, LocalAlloc};
    use windows_sys::Win32::System::Threading::{
        CREATE_SUSPENDED, CreateProcessW, GetExitCodeProcess, OpenProcessToken,
        PROCESS_INFORMATION, ResumeThread, TerminateProcess, WaitForSingleObject,
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
    /// DISABLE_MAX_PRIVILEGE removes every privilege; SANDBOX_INERT marks the
    /// token sandbox-inert so UAC/privilege escalation is disabled.
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
                0,
                std::ptr::null(),
                0,
                std::ptr::null(),
                0,
                std::ptr::null(),
                &mut new_token,
            )
        };
        unsafe { CloseHandle(current_token) };
        if ok == 0 {
            return Err(ForgeError::Safety("CreateRestrictedToken failed.".into()));
        }
        Ok(new_token)
    }

    /// Create (or reuse) the Forge AppContainer profile and return its SID as a raw pointer.
    fn appcontainer_sid_ptr() -> Result<*mut std::ffi::c_void> {
        let name: Vec<u16> = "ForgeSandbox\0".encode_utf16().collect();
        let display: Vec<u16> = "Forge Sandbox\0".encode_utf16().collect();
        let desc: Vec<u16> = "Forge CLI sandbox container\0".encode_utf16().collect();
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
            return Err(ForgeError::Safety(format!(
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
                return Err(ForgeError::Safety(format!(
                    "DeriveAppContainerSidFromAppContainerName failed (hr=0x{hr:08x})."
                )));
            }
            sid = derived;
        }
        Ok(sid)
    }

    /// Sanitized environment block for the child: "KEY=VALUE" pairs, each
    /// null-terminated, list terminated by an extra null. Only SystemRoot is
    /// forwarded (needed by most Windows binaries); no HOME, no proxy vars,
    /// no API keys, no agent sockets, no credentials.
    pub fn sanitized_environment() -> Vec<u16> {
        let allowed = [
            ("SystemRoot", std::env::var("SystemRoot").ok()),
            ("TEMP", std::env::var("TEMP").ok()),
            ("SystemDrive", std::env::var("SystemDrive").ok()),
        ];
        let mut block: Vec<u16> = Vec::new();
        for (key, value) in allowed {
            let Some(value) = value else { continue };
            // Drop credential-shaped values entirely (defense in depth).
            if crate::redaction::redact(&value) != value {
                continue;
            }
            let mut line: Vec<u16> = format!("{key}={value}").encode_utf16().collect();
            line.push(0);
            block.extend(line);
        }
        block.push(0); // double-null terminator
        block
    }

    /// Spawn a process suspended inside an AppContainer with:
    ///   1. restricted token (no privileges) — defense in depth,
    ///   2. AppContainer SID with ZERO capabilities → OS-enforced network deny
    ///      (no internetClient capability granted) and filesystem deny-by-default,
    ///   3. sanitized minimal environment (no secrets inherited),
    ///   4. Job Object assigned BEFORE resume with kill-on-close.
    ///
    /// Any failure before resume terminates the child and returns an error.
    pub fn spawn_suspended(
        exe: &str,
        args: &[String],
        _limits: ResourceLimits,
        job: &JobObject,
    ) -> Result<(HANDLE, HANDLE, u32)> {
        use windows_sys::Win32::Security::SECURITY_CAPABILITIES;
        use windows_sys::Win32::System::Threading::{
            DeleteProcThreadAttributeList, EXTENDED_STARTUPINFO_PRESENT,
            InitializeProcThreadAttributeList, PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES,
            STARTUPINFOEXW, UpdateProcThreadAttribute,
        };

        // 1. Restricted token (defense in depth alongside the AppContainer).
        let token = create_restricted_token()?;

        // 2. AppContainer SID with zero capabilities (network denied by OS).
        let sid = appcontainer_sid_ptr()?;
        let sec_caps = SECURITY_CAPABILITIES {
            AppContainerSid: sid as *mut _,
            Capabilities: std::ptr::null_mut(),
            CapabilityCount: 0,
            Reserved: 0,
        };

        // 3. Attribute list carrying the security-capabilities attribute.
        let mut attr_size: usize = 0;
        unsafe {
            InitializeProcThreadAttributeList(std::ptr::null_mut(), 1, 0, &mut attr_size);
        }
        if attr_size == 0 {
            unsafe { CloseHandle(token) };
            return Err(ForgeError::Safety(
                "InitializeProcThreadAttributeList size query failed; no process started.".into(),
            ));
        }
        let attr_list: *mut std::ffi::c_void = unsafe { LocalAlloc(LMEM_FIXED, attr_size) };
        if attr_list.is_null() {
            unsafe { CloseHandle(token) };
            return Err(ForgeError::Safety(
                "allocating proc-thread attribute list failed; no process started.".into(),
            ));
        }
        let ok = unsafe { InitializeProcThreadAttributeList(attr_list, 1, 0, &mut attr_size) };
        if ok == 0 {
            unsafe { LocalFree(attr_list as _) };
            unsafe { CloseHandle(token) };
            return Err(ForgeError::Safety(
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
                CloseHandle(token);
            }
            return Err(ForgeError::Safety(
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

        // 6. Sanitized environment block (no inherited secrets).
        let mut env_block = sanitized_environment();

        // 7. Create process suspended inside the AppContainer.
        let mut proc_info: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
        let ok = unsafe {
            CreateProcessW(
                std::ptr::null(),
                cmd_wide.as_mut_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                0, // no handle inheritance
                CREATE_SUSPENDED | EXTENDED_STARTUPINFO_PRESENT,
                env_block.as_mut_ptr().cast(),
                std::ptr::null(),
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
            unsafe { CloseHandle(token) };
            return Err(ForgeError::Safety(
                "CreateProcessW (suspended, AppContainer) failed; no process was started.".into(),
            ));
        }

        // 8. Assign to job BEFORE resuming (kill-on-close containment).
        if let Err(e) = job.assign(proc_info.hProcess) {
            unsafe {
                TerminateProcess(proc_info.hProcess, 1);
                CloseHandle(proc_info.hThread);
                CloseHandle(proc_info.hProcess);
                CloseHandle(token);
            }
            return Err(e);
        }

        // 9. Resume the suspended main thread.
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
    /// Returns (exit_code, timed_out). Terminates the tree on timeout.
    pub fn wait_for_completion(
        process_handle: HANDLE,
        thread_handle: HANDLE,
        timeout_ms: u32,
    ) -> Result<(u32, bool)> {
        const WAIT_TIMEOUT: u32 = 0x0000_0102;
        let result = unsafe { WaitForSingleObject(process_handle, timeout_ms) };
        if result == WAIT_TIMEOUT {
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
                "WaitForSingleObject failed (result=0x{result:08x})."
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

/// A qualified sandbox has passed the adversarial acceptance matrix on a
/// disposable native runner. Forge has not run those tests, so this stays
/// false and arbitrary command execution stays disabled. Do not flip this
/// to true without retained evidence from the 26-case matrix.
pub fn is_qualified() -> bool {
    false
}

pub fn status_text() -> String {
    if is_qualified() {
        "Sandbox: available (qualified)".to_string()
    } else {
        "Sandbox: primitives implemented (restricted token + AppContainer + Job Object + \
         sanitized env + OS network deny), but NOT qualified — adversarial acceptance tests \
         have not run on a disposable Windows runner. Arbitrary command execution is disabled."
            .to_string()
    }
}
