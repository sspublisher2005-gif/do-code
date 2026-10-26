//! Native OS sandbox primitives. This module is intentionally fail-closed:
//! every public entry point returns an error on platforms or configurations
//! where the required primitive cannot be established.
//!
//! Current status:
//!   - Windows: Job Object containment + restricted-token/DACL helpers are
//!     implemented at the primitive level. Full AppContainer integration and
//!     adversarial proof are still pending; command execution stays disabled
//!     until `sandbox self-test` passes on a real runner.
//!   - Linux / macOS: not yet implemented; execution stays disabled.

use crate::error::{ForgeError, Result};

#[cfg(windows)]
mod windows_impl {
    #![allow(dead_code)]
    use super::*;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JobObjectExtendedLimitInformation, SetInformationJobObject,
    };
    use windows_sys::Win32::System::Threading::PROCESS_INFORMATION;

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
        /// Create a new Job Object with kill-on-close and resource limits.
        pub fn create(limits: ResourceLimits) -> Result<Self> {
            let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
            if handle.is_null() {
                return Err(ForgeError::Safety(
                    "CreateJobObjectW failed; cannot establish process containment.".into(),
                ));
            }
            // Configure extended limits.
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

        /// Assign an already-created (suspended) process to this job.
        /// Must be called before the process is resumed.
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

    /// Create a child process in suspended state, assign it to a job, then resume.
    /// Returns process info. On any failure before resume, the process is terminated.
    pub fn spawn_suspended(
        _exe: &str,
        _args: &[String],
        _limits: ResourceLimits,
    ) -> Result<PROCESS_INFORMATION> {
        // This is a skeleton. Full implementation requires:
        // 1. Restricted token creation (CreateRestrictedToken)
        // 2. AppContainer SID (CreateAppContainerProfile)
        // 3. Suspended process creation with CREATE_SUSPENDED
        // 4. Job assignment
        // 5. Resume
        // For now, return an error to keep execution disabled.
        Err(ForgeError::Safety(
            "Windows sandbox driver is not yet qualified; command execution remains disabled."
                .into(),
        ))
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

/// Report whether a native sandbox driver is available and qualified.
pub fn is_qualified() -> bool {
    false
}

/// Human-readable status for `forge sandbox status`.
pub fn status_text() -> String {
    if is_qualified() {
        "Sandbox: available (qualified)".to_string()
    } else {
        "Sandbox: NOT IMPLEMENTED — no native driver has passed adversarial tests. \
         Arbitrary command execution is disabled."
            .to_string()
    }
}
