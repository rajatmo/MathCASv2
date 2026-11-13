//! The Unix guard: a resource limit set in the child between `fork` and `exec`, so the cap holds from the child's
//! first instruction (which satisfies the release handshake of decision D.4 trivially), and `wait4` for the peak.
//!
//! On Linux and Android the limit is `RLIMIT_DATA`, which since Linux 4.7 counts every private writable mapping
//! (heap, anonymous `mmap`, thread stacks) but not code, shared libraries or address space reserved without access
//! (glibc reserves 64 MB per malloc arena that way): it is the closest the kernel offers to the job object's
//! committed memory. Other Unix systems do not apply `RLIMIT_DATA` to `mmap`, so there the limit is `RLIMIT_AS`.
use super::{supervise, Container, Guard, GuardLimits, GuardReport};
use std::io;
#[cfg(not(target_vendor = "apple"))]
use std::os::unix::process::CommandExt;
use std::os::unix::process::ExitStatusExt;
use std::process::{Child, Command, ExitStatus};

/// `ru_maxrss` is in kibibytes on Linux and the BSDs, in bytes on Apple systems.
#[cfg(target_vendor = "apple")]
const MAXRSS_UNIT: u64 = 1;
#[cfg(not(target_vendor = "apple"))]
const MAXRSS_UNIT: u64 = 1024;

/// Runs the child under a memory resource limit: allocations past the cap fail; the process is killed at the
/// timeout.
pub struct RlimitGuard;

impl Guard for RlimitGuard {
    fn name(&self) -> &'static str {
        "rlimit"
    }
    fn enforces_memory(&self) -> bool {
        // Apple systems refuse an address-space limit below what a process already maps (`setrlimit` gives
        // "invalid argument") and apply no data limit to `mmap`: there the guard enforces the timeout only and
        // still reports the peak.
        !cfg!(target_vendor = "apple")
    }
    #[cfg(target_vendor = "apple")]
    fn run(&self, cmd: &mut Command, limits: &GuardLimits) -> io::Result<GuardReport> {
        supervise(cmd, limits, &mut Rlimit { peak: None }, false)
    }
    #[cfg(not(target_vendor = "apple"))]
    fn run(&self, cmd: &mut Command, limits: &GuardLimits) -> io::Result<GuardReport> {
        #[cfg(any(target_os = "linux", target_os = "android"))]
        let resource = libc::RLIMIT_DATA;
        #[cfg(not(any(target_os = "linux", target_os = "android")))]
        let resource = libc::RLIMIT_AS;
        let mut current = libc::rlimit { rlim_cur: libc::RLIM_INFINITY, rlim_max: libc::RLIM_INFINITY };
        // SAFETY: `current` is a writable `rlimit` for the call. On failure it keeps the "no limit" values.
        unsafe { libc::getrlimit(resource, &mut current) };
        // An unprivileged process cannot raise its hard limit (a shell's `ulimit` may have set one): stay below it.
        let cap = libc::rlim_t::try_from(limits.memory_bytes).unwrap_or(libc::RLIM_INFINITY).min(current.rlim_max);
        let limit = libc::rlimit { rlim_cur: cap, rlim_max: cap };
        // SAFETY: the closure runs in the forked child before `exec` and calls only `setrlimit`, which is
        // async-signal-safe; it touches no memory other than the copied `limit`.
        unsafe {
            cmd.pre_exec(move || {
                if libc::setrlimit(resource, &limit) != 0 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
        supervise(cmd, limits, &mut Rlimit { peak: None }, true)
    }
}

/// The container side: nothing to attach (the limit is already in place), reaping with `wait4` for the peak.
struct Rlimit {
    peak: Option<u64>,
}

impl Container for Rlimit {
    fn attach(&mut self, _: &Child) -> io::Result<()> {
        Ok(())
    }

    fn kill(&mut self, child: &mut Child) {
        // The process may have exited in the meantime; nothing else to do then.
        let _ = child.kill();
    }

    fn wait(&mut self, child: &mut Child) -> io::Result<ExitStatus> {
        let pid = libc::pid_t::try_from(child.id()).map_err(|_| io::Error::other("process id out of range"))?;
        let mut status = 0;
        // SAFETY: an all-zero `rusage` is a valid value of the plain C struct.
        let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
        loop {
            // SAFETY: `status` and `usage` are writable and live for the call; `pid` is our own unreaped child
            // (std reaps only in `wait`/`try_wait`, which the guard does not call when this container is used).
            let r = unsafe { libc::wait4(pid, &mut status, 0, &mut usage) };
            if r == pid {
                break;
            }
            let err = io::Error::last_os_error();
            if err.kind() != io::ErrorKind::Interrupted {
                return Err(err);
            }
        }
        self.peak = u64::try_from(usage.ru_maxrss).ok().map(|v| v * MAXRSS_UNIT);
        Ok(ExitStatus::from_raw(status))
    }

    fn peak_bytes(&self) -> Option<u64> {
        self.peak
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::guard::Exit;
    use std::time::Duration;

    #[test]
    fn the_child_starts_under_the_cap_and_its_peak_is_reported() {
        let limits = GuardLimits { memory_bytes: 64 << 20, timeout: Duration::from_secs(20) };
        // `ulimit -d` (Linux) or `ulimit -v` (elsewhere) prints the limit in kibibytes.
        let flag = if cfg!(any(target_os = "linux", target_os = "android")) { "-d" } else { "-v" };
        let mut cmd = Command::new("/bin/sh");
        cmd.args(["-c", &format!("ulimit {flag}")]);
        let report = RlimitGuard.run(&mut cmd, &limits).expect("sh runs");
        assert_eq!(report.exit, Exit::Code(0), "{report:?}");
        if RlimitGuard.enforces_memory() {
            assert_eq!(report.stdout.trim(), "65536", "{report:?}");
        }
        assert!(report.peak_bytes.is_some_and(|p| p > 0), "{report:?}");
    }

    #[test]
    fn the_timeout_still_kills() {
        let limits = GuardLimits { memory_bytes: 64 << 20, timeout: Duration::from_millis(300) };
        let mut cmd = Command::new("/bin/sh");
        cmd.args(["-c", "while :; do :; done"]);
        let report = RlimitGuard.run(&mut cmd, &limits).expect("sh runs");
        assert_eq!(report.exit, Exit::Timeout, "{report:?}");
    }
}
