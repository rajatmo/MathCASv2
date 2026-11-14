//! The MathCAS v2 notebook as an Android app: the native library of the package (`packaging/android`).
//!
//! Android starts a native program through an activity, not through `main`: the system's `NativeActivity` loads this
//! library and the window system's glue calls [`android_main`] on a thread of its own. From there on the program is
//! the notebook window of `mc-notebook`, with three differences the platform asks for:
//!
//! - the folders come from the activity (`mc_ui::android::start`): settings in the app's private folder, notebooks in
//!   the app's folder on the shared storage, temporary files in the app's cache;
//! - the kernel is the package's `libmc_kernel.so` (the `mc-kernel` program under a library's name: the folder of
//!   the native libraries is the only place an app may run a program from), found by `mc_notebook::kernel`;
//! - there are no system file dialogs for a native program: the notebook shows its own.
//!
//! A self-test runs when the notebooks folder holds `selftest.txt` (a script as for `MCNB_SELFTEST`): the log and the
//! screenshots go where the script says. Panics and start-up errors are written to the system log under the tag
//! `MathCAS`.
//!
//! On every other system this crate is empty.
#![cfg(target_os = "android")]

use mc_ui::android::AndroidApp;
use std::ffi::{c_char, c_int, CString};

#[link(name = "log")]
extern "C" {
    fn __android_log_write(priority: c_int, tag: *const c_char, text: *const c_char) -> c_int;
}

/// The priority of an error in the system log (`ANDROID_LOG_ERROR`).
const ERROR: c_int = 6;
/// The priority of a note in the system log (`ANDROID_LOG_INFO`).
const INFO: c_int = 4;

/// Write a line to the system log (`adb logcat -s MathCAS`).
fn log(priority: c_int, text: &str) {
    let Ok(text) = CString::new(text.replace('\0', " ")) else { return };
    // SAFETY: both pointers are NUL-terminated strings that live through the call
    unsafe {
        __android_log_write(priority, c"MathCAS".as_ptr(), text.as_ptr());
    }
}

/// The entry point: called by the activity's glue once the activity exists.
#[no_mangle]
fn android_main(app: AndroidApp) {
    std::panic::set_hook(Box::new(|info| log(ERROR, &format!("panic: {info}"))));
    mc_ui::android::start(&app);
    let script = mc_ui::dirs::documents_dir().map(|d| d.join("selftest.txt")).filter(|p| p.is_file());
    let selftest = script.and_then(|p| {
        // as on a desktop system: a self-test keeps no settings
        std::env::set_var("MCNB_SELFTEST", &p);
        mc_notebook::selftest::SelfTest::load(&p.to_string_lossy())
    });
    log(INFO, &format!("starting{}", if selftest.is_some() { " (self-test)" } else { "" }));
    if let Err(e) = mc_notebook::run(vec![], selftest) {
        log(ERROR, &format!("the window could not be opened: {e}"));
    }
}
