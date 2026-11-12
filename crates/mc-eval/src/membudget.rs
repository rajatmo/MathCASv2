//! The memory the process may use, for the default memory budget of an evaluation (decision p6core.11): the
//! smaller of the physical memory and a limit set on the process (on Windows a job object's process memory limit,
//! as `tools/guard.ps1` sets; on Unix `RLIMIT_AS`). `MC_MEMORY_LIMIT_MB` overrides both.

/// The cap in bytes, if one is known.
pub(crate) fn process_cap() -> Option<usize> {
    if let Some(mb) = std::env::var("MC_MEMORY_LIMIT_MB").ok().and_then(|v| v.trim().parse::<usize>().ok()) {
        return Some(mb.saturating_mul(1 << 20));
    }
    [physical(), limit()].into_iter().flatten().min()
}

#[cfg(windows)]
fn physical() -> Option<usize> {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    // SAFETY: a zeroed MEMORYSTATUSEX with its length set is what the call expects; it only writes the struct.
    unsafe {
        let mut m: MEMORYSTATUSEX = std::mem::zeroed();
        m.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
        (GlobalMemoryStatusEx(&mut m) != 0).then_some(m.ullTotalPhys as usize)
    }
}

#[cfg(windows)]
fn limit() -> Option<usize> {
    use windows_sys::Win32::System::JobObjects::{
        JobObjectExtendedLimitInformation, QueryInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_JOB_MEMORY, JOB_OBJECT_LIMIT_PROCESS_MEMORY,
    };
    // SAFETY: a null job handle queries the job of the calling process; the call writes at most the struct's size.
    unsafe {
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        let ok = QueryInformationJobObject(
            std::ptr::null_mut(),
            JobObjectExtendedLimitInformation,
            &mut info as *mut _ as *mut core::ffi::c_void,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            std::ptr::null_mut(),
        );
        if ok == 0 {
            return None;
        }
        let flags = info.BasicLimitInformation.LimitFlags;
        let process = (flags & JOB_OBJECT_LIMIT_PROCESS_MEMORY != 0).then_some(info.ProcessMemoryLimit);
        let job = (flags & JOB_OBJECT_LIMIT_JOB_MEMORY != 0).then_some(info.JobMemoryLimit);
        [process, job].into_iter().flatten().filter(|&v| v > 0).min()
    }
}

#[cfg(unix)]
fn physical() -> Option<usize> {
    // SAFETY: sysconf has no preconditions.
    let (pages, size) = unsafe { (libc::sysconf(libc::_SC_PHYS_PAGES), libc::sysconf(libc::_SC_PAGESIZE)) };
    (pages > 0 && size > 0).then(|| (pages as usize).saturating_mul(size as usize))
}

#[cfg(unix)]
fn limit() -> Option<usize> {
    let mut r = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
    // SAFETY: getrlimit writes the struct it is given.
    let ok = unsafe { libc::getrlimit(libc::RLIMIT_AS, &mut r) } == 0;
    (ok && r.rlim_cur != libc::RLIM_INFINITY).then_some(r.rlim_cur as usize)
}

#[cfg(not(any(windows, unix)))]
fn physical() -> Option<usize> {
    None
}

#[cfg(not(any(windows, unix)))]
fn limit() -> Option<usize> {
    None
}

#[cfg(test)]
mod tests {
    #[test]
    fn some_cap_is_known() {
        let cap = super::process_cap();
        assert!(cap.is_some_and(|c| c > 1 << 26), "{cap:?}");
    }
}
