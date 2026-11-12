//! Guards: run one child process under a memory cap and a wall-clock timeout (02-benchmarks section 1).
//!
//! A [`Guard`] owns the platform mechanism; the supervision loop is shared. The child is spawned with piped
//! standard streams and waits for one line on its standard input before it starts work: the guard sends that line
//! only after the process is inside its container, so no allocation escapes the cap (decision D.4). The guard waits
//! for both output pipes to close, with the timeout as deadline, so it needs no polling.
//!
//! [`JobGuard`] (Windows) puts the child in a job object with a per-process memory limit, as `tools/guard.ps1`
//! does, through the Windows API directly. `RlimitGuard` (Unix) sets a memory resource limit in the child before
//! `exec`. [`PortableGuard`] works everywhere with `std` alone and enforces only the timeout.
use std::io::{self, Read, Write};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::JobGuard;
#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub use unix::RlimitGuard;

/// The limits of one guarded run.
#[derive(Clone, Copy, Debug)]
pub struct GuardLimits {
    pub memory_bytes: u64,
    /// Wall-clock time after which the process tree is killed.
    pub timeout: Duration,
}

/// How the guarded process ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Exit {
    /// It exited by itself with this code.
    Code(i32),
    /// It ended without an exit code (killed by a signal).
    NoCode,
    /// It ran past the timeout and was killed.
    Timeout,
    /// It failed because it reached the memory cap.
    MemoryCap,
}

/// Everything a guarded run produced.
#[derive(Clone, Debug)]
pub struct GuardReport {
    pub exit: Exit,
    pub stdout: String,
    pub stderr: String,
    pub wall: Duration,
    /// Peak memory of the process as the container saw it, when the guard can tell.
    pub peak_bytes: Option<u64>,
}

/// Runs a command under limits.
pub trait Guard {
    /// A short name for traces.
    fn name(&self) -> &'static str;
    /// Whether [`GuardLimits::memory_bytes`] is enforced (otherwise only the timeout is).
    fn enforces_memory(&self) -> bool;
    /// Run `cmd` to completion or to a limit. The command's standard streams are replaced by pipes.
    fn run(&self, cmd: &mut Command, limits: &GuardLimits) -> io::Result<GuardReport>;
}

/// The guard best suited to this platform.
pub fn platform_guard() -> Box<dyn Guard> {
    #[cfg(windows)]
    return Box::new(JobGuard);
    #[cfg(unix)]
    return Box::new(RlimitGuard);
    #[cfg(not(any(windows, unix)))]
    return Box::new(PortableGuard);
}

/// The line the guard sends once the child is contained; the child waits for it (see [`wait_for_release`]).
pub const RELEASE: &[u8] = b"go\n";

/// Called by the child first thing: block until the guard has contained this process.
pub fn wait_for_release() -> io::Result<()> {
    let mut byte = [0u8; 1];
    let mut stdin = io::stdin().lock();
    loop {
        // End of input also releases, so the child still runs when started by hand with closed input.
        if stdin.read(&mut byte)? == 0 || byte[0] == b'\n' {
            return Ok(());
        }
    }
}

/// The platform part of a guard: how to contain, kill and measure one process.
trait Container {
    fn attach(&mut self, child: &Child) -> io::Result<()>;
    fn kill(&mut self, child: &mut Child);
    /// Reap the process; a container may override this to collect resource usage on the way.
    fn wait(&mut self, child: &mut Child) -> io::Result<ExitStatus> {
        child.wait()
    }
    fn peak_bytes(&self) -> Option<u64>;
}

/// The guard without a memory container: timeout only, `std` only.
pub struct PortableGuard;

struct NoContainer;

impl Container for NoContainer {
    fn attach(&mut self, _: &Child) -> io::Result<()> {
        Ok(())
    }
    fn kill(&mut self, child: &mut Child) {
        // The process may have exited in the meantime; nothing else to do then.
        let _ = child.kill();
    }
    fn peak_bytes(&self) -> Option<u64> {
        None
    }
}

impl Guard for PortableGuard {
    fn name(&self) -> &'static str {
        "portable"
    }
    fn enforces_memory(&self) -> bool {
        false
    }
    fn run(&self, cmd: &mut Command, limits: &GuardLimits) -> io::Result<GuardReport> {
        supervise(cmd, limits, &mut NoContainer, false)
    }
}

/// The marker Rust's runtime prints when an allocation fails and the process aborts.
const ALLOC_FAILURE: &str = "memory allocation of";

fn supervise(
    cmd: &mut Command,
    limits: &GuardLimits,
    container: &mut dyn Container,
    memory_enforced: bool,
) -> io::Result<GuardReport> {
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let start = Instant::now();
    let mut child = cmd.spawn()?;
    if let Err(e) = container.attach(&child) {
        let _ = child.kill();
        let _ = container.wait(&mut child);
        return Err(e);
    }
    if let Some(mut stdin) = child.stdin.take() {
        // A child that already exited cannot read the release line; its exit status tells the story.
        let _ = stdin.write_all(RELEASE);
    }
    let (tx, rx) = mpsc::channel();
    let out = spawn_reader(child.stdout.take(), tx.clone());
    let err = spawn_reader(child.stderr.take(), tx);
    let deadline = start + limits.timeout;
    let mut closed = 0;
    let mut timed_out = false;
    while closed < 2 {
        match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(()) => closed += 1,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                timed_out = true;
                container.kill(&mut child);
                break;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    let status = container.wait(&mut child)?;
    let wall = start.elapsed();
    let stdout = out.join().unwrap_or_default();
    let stderr = err.join().unwrap_or_default();
    let peak_bytes = container.peak_bytes();
    let exit = if timed_out {
        Exit::Timeout
    } else {
        match status.code() {
            Some(0) => Exit::Code(0),
            _ if memory_enforced && hit_memory_cap(&stderr, peak_bytes, limits.memory_bytes) => Exit::MemoryCap,
            Some(code) => Exit::Code(code),
            None => Exit::NoCode,
        }
    };
    Ok(GuardReport { exit, stdout, stderr, wall, peak_bytes })
}

/// A failed process hit the cap if the runtime reported a failed allocation or its peak reached the cap. A single
/// huge allocation can fail with a low peak, hence the message test as well (decision D.4).
fn hit_memory_cap(stderr: &str, peak: Option<u64>, cap: u64) -> bool {
    let margin = (cap / 32).max(16 << 20);
    stderr.contains(ALLOC_FAILURE) || peak.is_some_and(|p| p + margin >= cap)
}

fn spawn_reader<R: Read + Send + 'static>(pipe: Option<R>, done: mpsc::Sender<()>) -> thread::JoinHandle<String> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = pipe {
            // A read error ends the stream like end of file; what was read so far is kept.
            let _ = pipe.read_to_end(&mut bytes);
        }
        let _ = done.send(());
        String::from_utf8_lossy(&bytes).into_owned()
    })
}
