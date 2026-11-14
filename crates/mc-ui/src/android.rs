//! Android: the activity a window is opened in, and the folders the system gives the app.
//!
//! A native Android program starts in `android_main(app)`, not in `main`. The entry point hands the activity to
//! [`start`] before it opens a window; [`crate::gpu::run_native`] then passes it on to the window system. [`start`]
//! also names the folders the rest of the program asks for by the usual means:
//!
//! - the settings folder ([`crate::dirs::set_app_dir`]): the app's internal files folder, private to the app;
//! - the folder for notebooks ([`crate::dirs::set_documents_dir`]): the app's folder on the shared storage (file
//!   managers and a computer over USB reach it, and it needs no permission), else `notebooks` in the internal folder;
//! - `TMPDIR`: `cache` in the internal folder. The system's own temporary folder is not writable by apps, and the
//!   plots, embedded pictures and typeset caches all go to `std::env::temp_dir()`;
//! - `HOME`: the internal folder.
pub use winit::platform::android::activity::AndroidApp;

use std::path::PathBuf;
use std::sync::OnceLock;

static APP: OnceLock<AndroidApp> = OnceLock::new();

/// Keep the activity for the window, and name the app's folders. Call it first in `android_main`.
pub fn start(app: &AndroidApp) {
    let _ = APP.set(app.clone());
    let Some(internal) = app.internal_data_path() else { return };
    let cache = internal.join("cache");
    let _ = std::fs::create_dir_all(&cache);
    // nothing else of the program runs yet (the entry point calls this before it starts a thread or a window)
    std::env::set_var("TMPDIR", &cache);
    std::env::set_var("HOME", &internal);
    let documents = app.external_data_path().unwrap_or_else(|| internal.join("notebooks"));
    let _ = std::fs::create_dir_all(&documents);
    crate::dirs::set_documents_dir(documents);
    crate::dirs::set_app_dir(internal);
}

/// The activity given to [`start`].
pub fn app() -> Option<AndroidApp> {
    APP.get().cloned()
}

/// How much of the window's top and bottom the system's bars cover, in pixels (the status bar; the navigation bar or
/// the gesture handle): the window fills the screen, and the system reports the part that is free.
pub fn bars() -> (f32, f32) {
    let Some(app) = APP.get() else { return (0.0, 0.0) };
    let (Some(window), free) = (app.native_window(), app.content_rect()) else { return (0.0, 0.0) };
    // not reported yet
    if free.bottom <= free.top {
        return (0.0, 0.0);
    }
    (free.top.max(0) as f32, (window.height() - free.bottom).max(0) as f32)
}

/// The folder of the app's native libraries (where the package installer unpacks `lib/<abi>/` of the package): the
/// only place an app may run a program from. Found as the folder of the loaded library named `library` (a file name
/// such as `libmc_notebook_android.so`) in this process's memory map.
pub fn native_library_dir(library: &str) -> Option<PathBuf> {
    let maps = std::fs::read_to_string("/proc/self/maps").ok()?;
    maps.lines()
        .filter_map(|l| l.split_whitespace().last())
        .find(|p| p.starts_with('/') && p.ends_with(library))
        .and_then(|p| std::path::Path::new(p).parent().map(std::path::Path::to_path_buf))
}

// ------------------------------------------------------------------------------------------- the activity's Java
//
// The share sheet, the clipboard and the document picker are the system's Java interfaces. The activity
// (`packaging/android/java/.../MathCasActivity.java`) has one small method for each; they are called here through the
// Java native interface. Text crosses as UTF-8 bytes (`byte[]`): the interface's own strings are a variant of UTF-8
// that mangles the characters beyond the basic plane, which mathematics uses.
use jni_sys::{jbyteArray, jobject, jvalue, JNIEnv, JavaVM, JNI_OK, JNI_VERSION_1_6};
use std::ffi::CString;
use std::path::Path;

/// Run `f` with this thread's Java environment and the activity; `None` before [`start`] or when the thread cannot be
/// attached to the Java machine.
fn with_java<R>(f: impl FnOnce(*mut JNIEnv, jobject) -> R) -> Option<R> {
    let app = APP.get()?;
    let vm = app.vm_as_ptr() as *mut JavaVM;
    let activity = app.activity_as_ptr() as jobject;
    if vm.is_null() || activity.is_null() {
        return None;
    }
    let mut env: *mut JNIEnv = std::ptr::null_mut();
    // SAFETY: `vm` is the process's Java machine (it lives as long as the process). A thread that is attached
    // already gets its environment from `GetEnv`; any other is attached and stays so until it ends, when the Java
    // machine detaches it.
    unsafe {
        let table = &**vm;
        let have = (table.GetEnv?)(vm, &mut env as *mut _ as *mut _, JNI_VERSION_1_6);
        if have != JNI_OK
            && (table.AttachCurrentThread?)(vm, &mut env as *mut _ as *mut _, std::ptr::null_mut()) != JNI_OK
        {
            return None;
        }
    }
    (!env.is_null()).then(|| f(env, activity))
}

/// A Java `byte[]` holding `bytes` (a local reference of the calling frame).
///
/// # Safety
/// `env` is the calling thread's environment.
unsafe fn java_bytes(env: *mut JNIEnv, bytes: &[u8]) -> Option<jbyteArray> {
    let table = &**env;
    let array = (table.NewByteArray?)(env, bytes.len() as i32);
    if array.is_null() {
        return None;
    }
    (table.SetByteArrayRegion?)(env, array, 0, bytes.len() as i32, bytes.as_ptr() as *const i8);
    Some(array)
}

/// Call the activity's method `name` of signature `sig` with byte-array arguments; a method that returns `byte[]`
/// gives its bytes (`Some(vec![])` for a method without a result, or one that returned null). An exception thrown in
/// Java is cleared and gives `None`.
fn call(name: &str, sig: &str, args: &[&[u8]]) -> Option<Vec<u8>> {
    let (name, sig_c) = (CString::new(name).ok()?, CString::new(sig).ok()?);
    let returns_bytes = sig.ends_with(")[B");
    with_java(|env, activity| {
        // SAFETY: `env` belongs to this thread and `activity` is the activity's global reference; every local
        // reference made here is released before returning (the thread never returns to Java, which would do it).
        unsafe {
            let table = &**env;
            let class = (table.GetObjectClass?)(env, activity);
            let method = (table.GetMethodID?)(env, class, name.as_ptr(), sig_c.as_ptr());
            let mut locals: Vec<jobject> = vec![class];
            let mut result = None;
            if !method.is_null() {
                let arrays: Option<Vec<jbyteArray>> = args.iter().map(|a| java_bytes(env, a)).collect();
                if let Some(arrays) = arrays {
                    let values: Vec<jvalue> = arrays.iter().map(|a| jvalue { l: *a }).collect();
                    if returns_bytes {
                        let out = (table.CallObjectMethodA?)(env, activity, method, values.as_ptr());
                        result = Some(vec![]);
                        if !out.is_null() {
                            let n = (table.GetArrayLength?)(env, out);
                            let mut bytes = vec![0u8; n.max(0) as usize];
                            (table.GetByteArrayRegion?)(env, out, 0, n, bytes.as_mut_ptr() as *mut i8);
                            result = Some(bytes);
                            locals.push(out);
                        }
                    } else {
                        (table.CallVoidMethodA?)(env, activity, method, values.as_ptr());
                        result = Some(vec![]);
                    }
                    locals.extend(arrays);
                }
            }
            if (table.ExceptionCheck?)(env) != 0 {
                (table.ExceptionClear?)(env);
                result = None;
            }
            for l in locals {
                (table.DeleteLocalRef?)(env, l);
            }
            result
        }
    })
    .flatten()
}

/// Open the system's document picker; the chosen file arrives through [`take_incoming`]. `false` when the activity
/// cannot be asked.
pub fn pick_document() -> bool {
    call("pickDocument", "()V", &[]).is_some()
}

/// A file another app handed over (the picker's answer, "Open with"), copied into the app's cache.
pub fn take_incoming() -> Option<PathBuf> {
    let path = call("takeIncoming", "()[B", &[])?;
    String::from_utf8(path).ok().filter(|p| !p.is_empty()).map(PathBuf::from)
}

/// Offer the file at `path` (in the folder of `platform::shared_dir`) to other apps as `mime`.
pub fn share_file(path: &Path, mime: &str) -> Result<(), String> {
    call("shareFile", "([B[B)V", &[path.to_string_lossy().as_bytes(), mime.as_bytes()])
        .map(|_| ())
        .ok_or_else(|| "the system's share sheet could not be opened".to_string())
}

/// Offer `text` to other apps.
pub fn share_text(text: &str) {
    call("shareText", "([B)V", &[text.as_bytes()]);
}

/// Put `text` on the system clipboard.
pub fn copy_text(text: &str) -> bool {
    call("copyText", "([B)V", &[text.as_bytes()]).is_some()
}

/// Put the picture file at `path` (in the folder of `platform::shared_dir`) on the system clipboard.
pub fn copy_image(path: &Path) -> bool {
    call("copyImage", "([B)V", &[path.to_string_lossy().as_bytes()]).is_some()
}

/// Whether the system shows its dark theme (`None` when the activity cannot be asked).
pub fn night_mode() -> Option<bool> {
    call("nightMode", "()[B", &[]).map(|b| b.first().is_some_and(|v| *v != 0))
}

/// Tell the system that the page under its bars is light (`false`: dark), so its clock and icons contrast with it.
pub fn light_bars(light: bool) {
    call("lightBars", "([B)V", &[&[u8::from(light)]]);
}

/// The text on the system clipboard (`None`: none, or it cannot be read now).
pub fn clipboard_text() -> Option<String> {
    String::from_utf8(call("clipboardText", "()[B", &[])?).ok().filter(|t| !t.is_empty())
}
