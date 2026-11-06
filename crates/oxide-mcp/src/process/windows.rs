//! Windows Job Objects.
//!
//! `unsafe` is allowed in this module only: it wraps four Win32 calls
//! (`CreateJobObjectW`, `SetInformationJobObject`, `AssignProcessToJobObject`,
//! `TerminateJobObject`) and `CloseHandle`. A job with `KILL_ON_JOB_CLOSE` ends every process
//! a server started (e.g. the `node.exe` behind `npx.cmd`) when Oxide stops it, and also when
//! Oxide itself exits or crashes, because Windows closes the job handle then.
#![allow(unsafe_code)]

use std::ffi::c_void;
use std::os::windows::io::RawHandle;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
    SetInformationJobObject, TerminateJobObject,
};

/// `CREATE_NO_WINDOW`: console programs (npx, uvx, python) get no console window.
pub const CREATE_NO_WINDOW: u32 = windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;

/// An owned job object handle.
pub struct Job(HANDLE);

// SAFETY: a job handle is a kernel object handle; it may be used and closed from any thread.
unsafe impl Send for Job {}
// SAFETY: the Win32 job functions used here are thread-safe.
unsafe impl Sync for Job {}

impl Job {
    /// A new anonymous job that kills its processes when its last handle closes.
    pub fn new() -> std::io::Result<Job> {
        // SAFETY: null attributes and name are allowed (default security, anonymous job).
        let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if handle.is_null() {
            return Err(std::io::Error::last_os_error());
        }
        let job = Job(handle);
        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        // SAFETY: `info` is a valid JOBOBJECT_EXTENDED_LIMIT_INFORMATION and the length
        // matches its size; the handle is a job we own.
        let ok = unsafe {
            SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                (&raw const info).cast::<c_void>(),
                u32::try_from(size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>()).unwrap_or(0),
            )
        };
        if ok == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(job)
    }

    /// Puts a process (and so everything it starts later) into the job.
    pub fn assign(&self, process: RawHandle) -> std::io::Result<()> {
        // SAFETY: `process` is a live process handle owned by tokio's `Child`.
        let ok = unsafe { AssignProcessToJobObject(self.0, process as HANDLE) };
        if ok == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }

    /// Ends every process in the job.
    pub fn terminate(&self) {
        // SAFETY: the handle is a job we own.
        unsafe {
            TerminateJobObject(self.0, 1);
        }
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        // SAFETY: the handle is ours and closed exactly once. KILL_ON_JOB_CLOSE ends any
        // process still in the job.
        unsafe {
            CloseHandle(self.0);
        }
    }
}
