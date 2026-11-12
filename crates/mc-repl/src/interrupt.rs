//! Forwarding Ctrl+C to the engine as a stop flag instead of ending the process.
//!
//! On Windows a console control handler raises the flag and reports the event as handled. Elsewhere no handler is
//! installed (`std` has no signal API), so Ctrl+C ends the process as usual; a platform implementation can be
//! added behind [`install`] without changing the loop.
use mc_api::StopFlag;
use std::io;
use std::sync::OnceLock;

/// The flag the handler raises. Set once by [`install`].
static FLAG: OnceLock<StopFlag> = OnceLock::new();

/// Route Ctrl+C to `flag` for the rest of the process. Returns whether a handler is active on this platform.
pub fn install(flag: StopFlag) -> io::Result<bool> {
    if FLAG.set(flag).is_err() {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, "interrupt handler already installed"));
    }
    platform::install()
}

#[cfg(windows)]
mod platform {
    use std::io;

    pub(super) const CTRL_C_EVENT: u32 = 0;
    const CTRL_BREAK_EVENT: u32 = 1;

    #[link(name = "kernel32")]
    extern "system" {
        fn SetConsoleCtrlHandler(handler: Option<unsafe extern "system" fn(u32) -> i32>, add: i32) -> i32;
    }

    /// Runs on a thread the system creates; it only raises an atomic flag.
    pub(super) unsafe extern "system" fn handler(event: u32) -> i32 {
        match (event, super::FLAG.get()) {
            (CTRL_C_EVENT | CTRL_BREAK_EVENT, Some(flag)) => {
                flag.raise();
                1
            }
            // Closing the console, logging off and shutting down keep their default handling.
            _ => 0,
        }
    }

    pub fn install() -> io::Result<bool> {
        // SAFETY: `handler` has the signature the API requires and stays valid for the life of the process.
        if unsafe { SetConsoleCtrlHandler(Some(handler), 1) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(true)
    }
}

#[cfg(not(windows))]
mod platform {
    pub fn install() -> std::io::Result<bool> {
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handler_raises_the_flag() {
        let flag = StopFlag::new();
        let active = install(flag.clone()).unwrap();
        assert!(install(StopFlag::new()).is_err(), "only one handler per process");
        #[cfg(windows)]
        {
            assert!(active);
            // SAFETY: calling the handler directly, as the system would on its own thread.
            assert_eq!(unsafe { platform::handler(platform::CTRL_C_EVENT) }, 1);
            assert!(flag.is_raised());
            assert_eq!(unsafe { platform::handler(2) }, 0, "close events keep their default handling");
        }
        #[cfg(not(windows))]
        assert!(!active);
    }
}
