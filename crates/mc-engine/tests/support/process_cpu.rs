//! The processor time of the whole test process (every thread, user and kernel), to tell the work an evaluation
//! does after a stop from the time it waits for a processor on a loaded machine. `None` where the system call is
//! not wired up (the callers then judge by the time on the clock alone).
use std::time::Duration;

/// The processor time the process has used so far.
#[cfg(windows)]
pub fn process_cpu() -> Option<Duration> {
    #[repr(C)]
    #[derive(Default)]
    struct FileTime {
        low: u32,
        high: u32,
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentProcess() -> isize;
        fn GetProcessTimes(p: isize, creation: *mut FileTime, exit: *mut FileTime, kernel: *mut FileTime, user: *mut FileTime) -> i32;
    }
    let (mut c, mut e, mut k, mut u) = (FileTime::default(), FileTime::default(), FileTime::default(), FileTime::default());
    // SAFETY: GetCurrentProcess returns a pseudo handle that needs no closing; the four out-pointers are valid
    // FILETIME structures of this frame.
    let ok = unsafe { GetProcessTimes(GetCurrentProcess(), &mut c, &mut e, &mut k, &mut u) };
    if ok == 0 {
        return None;
    }
    // units of 100 ns
    let ticks = |t: &FileTime| ((t.high as u64) << 32) | t.low as u64;
    Some(Duration::from_nanos((ticks(&k) + ticks(&u)) * 100))
}

/// The processor time the process has used so far.
#[cfg(any(target_os = "linux", target_os = "android", target_os = "macos"))]
pub fn process_cpu() -> Option<Duration> {
    use std::os::raw::{c_int, c_long};
    #[repr(C)]
    struct TimeSpec {
        sec: c_long,
        nsec: c_long,
    }
    extern "C" {
        fn clock_gettime(clock: c_int, t: *mut TimeSpec) -> c_int;
    }
    #[cfg(target_os = "macos")]
    const PROCESS_CPU: c_int = 12;
    #[cfg(not(target_os = "macos"))]
    const PROCESS_CPU: c_int = 2;
    let mut t = TimeSpec { sec: 0, nsec: 0 };
    // SAFETY: `t` is a valid timespec of this frame; the clock id is the process CPU clock of the platform.
    if unsafe { clock_gettime(PROCESS_CPU, &mut t) } != 0 {
        return None;
    }
    Some(Duration::new(t.sec as u64, t.nsec as u32))
}

/// The processor time the process has used so far.
#[cfg(not(any(windows, target_os = "linux", target_os = "android", target_os = "macos")))]
pub fn process_cpu() -> Option<Duration> {
    None
}

/// The tests that read the process time run one at a time, so that one test's work is not counted for another.
pub fn serial() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|p| p.into_inner())
}

/// Did an evaluation that ended `wall` after the stop, having used `cpu` of processor time since the stop, end
/// late? Only when it also kept working: more than `allowed` of processor time per worker thread (`threads`), or,
/// whatever it used, more than twenty times `allowed` on the clock (a wait that never ends uses no processor time).
pub fn late(wall: Duration, cpu: Option<Duration>, allowed: Duration, threads: u32) -> bool {
    if wall <= allowed {
        return false;
    }
    match cpu {
        Some(c) => c > allowed * threads || wall > allowed * 20,
        None => true,
    }
}
