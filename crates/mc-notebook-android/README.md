# mc-notebook-android

The MathCAS notebook as an Android app: the native library (`cdylib`) the app package loads. On every other system the
crate is empty.

## Modules
- `lib.rs`: `android_main`, called by the window system's glue on a thread of its own after Android's
  `NativeActivity` loads the library.

## Design
- From `android_main` on, the program is the notebook window of `mc-notebook`, with three platform differences:
  - folders come from the activity (`mc_ui::android::start`): settings in the app's private folder, notebooks in the
    app's folder on shared storage, temporary files in the app's cache;
  - the kernel is the package's `libmc_kernel.so` (the kernel program under a library name, since an app may run
    programs only from its native library folder), found by `mc_notebook::kernel`;
  - the notebook shows its own file dialogs, as native programs have no system ones.
- The dependencies on `mc-notebook` and `mc-ui` apply only when the target is Android.
- A self-test runs when the notebooks folder holds `selftest.txt`; panics and start-up errors go to the system log
  under the tag `MathCAS`.

## Tests
- The crate has no tests of its own; the notebook code it wraps is tested in `mc-notebook` with
  `cargo test --release -p mc-notebook`, and the app is built and exercised with the Android packaging scripts.
