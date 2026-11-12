//! The Windows guard: a job object with a per-process memory limit that kills its processes when closed, the same
//! mechanism as `tools/guard.ps1`, called through the Windows API with hand-written declarations (no crates).
use super::{supervise, Container, Guard, GuardLimits, GuardReport};
use std::ffi::c_void;
use std::io;
use std::os::windows::io::AsRawHandle;
use std::process::{Child, Command};
use std::ptr;

type Handle = *mut c_void;

/// `JOBOBJECT_BASIC_LIMIT_INFORMATION`.
#[repr(C)]
#[derive(Default)]
struct BasicLimits {
    per_process_user_time_limit: i64,
    per_job_user_time_limit: i64,
    limit_flags: u32,
    minimum_working_set_size: usize,
    maximum_working_set_size: usize,
    active_process_limit: u32,
    affinity: usize,
    priority_class: u32,
    scheduling_class: u32,
}

/// `IO_COUNTERS`.
#[repr(C)]
#[derive(Default)]
struct IoCounters {
    counters: [u64; 6],
}

/// `JOBOBJECT_EXTENDED_LIMIT_INFORMATION`.
#[repr(C)]
#[derive(Default)]
struct ExtendedLimits {
    basic: BasicLimits,
    io: IoCounters,
    process_memory_limit: usize,
    job_memory_limit: usize,
    peak_process_memory_used: usize,
    peak_job_memory_used: usize,
}

const JOB_OBJECT_EXTENDED_LIMIT_INFORMATION: i32 = 9;
const JOB_OBJECT_LIMIT_PROCESS_MEMORY: u32 = 0x100;
const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: u32 = 0x2000;
/// The exit code of processes killed at the timeout (as in `guard.ps1`).
const KILLED_EXIT_CODE: u32 = 124;

#[link(name = "kernel32")]
extern "system" {
    fn CreateJobObjectW(attributes: *mut c_void, name: *const u16) -> Handle;
    fn SetInformationJobObject(job: Handle, class: i32, info: *const c_void, length: u32) -> i32;
    fn QueryInformationJobObject(job: Handle, class: i32, info: *mut c_void, length: u32, ret: *mut u32) -> i32;
    fn AssignProcessToJobObject(job: Handle, process: Handle) -> i32;
    fn TerminateJobObject(job: Handle, exit_code: u32) -> i32;
    fn CloseHandle(handle: Handle) -> i32;
}

/// Runs the child in a job object: allocations past the cap fail, and the whole process tree is killed at the
/// timeout or when the guard is done.
pub struct JobGuard;

impl Guard for JobGuard {
    fn name(&self) -> &'static str {
        "job-object"
    }
    fn enforces_memory(&self) -> bool {
        true
    }
    fn run(&self, cmd: &mut Command, limits: &GuardLimits) -> io::Result<GuardReport> {
        let mut job = Job::new(limits.memory_bytes)?;
        supervise(cmd, limits, &mut job, true)
    }
}

/// An owned job-object handle.
struct Job(Handle);

impl Job {
    fn new(memory_bytes: u64) -> io::Result<Job> {
        // SAFETY: null attributes and name are allowed; the returned handle is owned by `Job` and closed in `drop`.
        let handle = unsafe { CreateJobObjectW(ptr::null_mut(), ptr::null()) };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        let job = Job(handle);
        let info = ExtendedLimits {
            basic: BasicLimits {
                limit_flags: JOB_OBJECT_LIMIT_PROCESS_MEMORY | JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                ..BasicLimits::default()
            },
            process_memory_limit: usize::try_from(memory_bytes).unwrap_or(usize::MAX),
            ..ExtendedLimits::default()
        };
        // SAFETY: `info` is a valid JOBOBJECT_EXTENDED_LIMIT_INFORMATION of the size passed.
        let ok = unsafe {
            SetInformationJobObject(
                job.0,
                JOB_OBJECT_EXTENDED_LIMIT_INFORMATION,
                (&info as *const ExtendedLimits).cast(),
                size_of::<ExtendedLimits>() as u32,
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(job)
    }
}

impl Container for Job {
    fn attach(&mut self, child: &Child) -> io::Result<()> {
        // SAFETY: both handles are valid for the duration of the call; the child handle stays owned by `child`.
        if unsafe { AssignProcessToJobObject(self.0, child.as_raw_handle()) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    fn kill(&mut self, _: &mut Child) {
        // SAFETY: the job handle is valid. A failure means the processes are already gone.
        unsafe { TerminateJobObject(self.0, KILLED_EXIT_CODE) };
    }

    fn peak_bytes(&self) -> Option<u64> {
        let mut info = ExtendedLimits::default();
        // SAFETY: `info` is a writable buffer of the size passed; the return-length pointer may be null.
        let ok = unsafe {
            QueryInformationJobObject(
                self.0,
                JOB_OBJECT_EXTENDED_LIMIT_INFORMATION,
                (&mut info as *mut ExtendedLimits).cast(),
                size_of::<ExtendedLimits>() as u32,
                ptr::null_mut(),
            )
        };
        (ok != 0).then_some(info.peak_process_memory_used as u64)
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        // SAFETY: the handle is owned and closed exactly once; closing kills any process left in the job.
        unsafe { CloseHandle(self.0) };
    }
}
