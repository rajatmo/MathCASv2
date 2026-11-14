# mc-ui: window support shared by the notebook and the documentation window (round 10)

Fonts, clipboard, GPU set-up and the settings folder, so both windows run on Windows, Linux and (later) Android.

## Modules

| module | contents |
|---|---|
| `fonts` | `Face { Code, CodeBold, Prose, ProseBold, ProseItalic, Ui }` with `.font(size)`, `.family()`; `install(ctx)` (the one set-up both apps call); `definitions(Source)`; `Source::{PreferSystem, Bundled}` (`MCUI_FONTS=bundled` forces the bundled set on Windows); `BUNDLED` (name, bytes, licence of each file) |
| `clipboard` | trait `SystemClipboard { set, set_picture, get }`; `system()`: Windows (`clipboard-win`: text, the private `MathCAS.Cells` format, pictures as PNG + bitmap + text), Linux/BSD/macOS (`arboard`: text, `image/png`), none on Android, without a display or with `MCUI_CLIPBOARD=local`; `Clipboard` (the window's: opened on first use, in-process copy so cells survive a paste where the system has no private format, `for_window(&ctx)` hands text to egui's copy command when no system clipboard answers); `Contents`, `Picture`, `encode_png`, `dib` |
| `gpu` | moved from mc-notebook unchanged: `backend_order`, `order_for`, `Prefer`, `choose_adapter`, crash marks in `gpu.json` (`starting`, `started`, `failed`, `crashed`, `to_try`); new `run_native(app, viewport, state, create)`, the start-up loop both `main.rs` files used to copy |
| `dirs` | `config_dir()`: `MCNB_CONFIG_DIR`, none during a self-test (`MCNB_SELFTEST`, `MCDOCS_SELFTEST`), else `user_config_dir()` + `/MathCAS` (Windows `%APPDATA%`; Linux/BSD absolute `$XDG_CONFIG_HOME` else `~/.config`; macOS `~/Library/Application Support`; Android the folder given to `set_app_dir`); `config_dir_for(os, env)` (testable rule); `write_atomic` |

## Decisions (PORT)

1. Bundled fonts: DejaVu Sans Mono regular/bold (code; covers Greek, maths, box drawing), Noto Sans 1.04
   regular/bold/italic (text), a DejaVu Sans symbol subset, then STIX Two Math from mc-mathfont, then egui's emoji and
   icon fonts. Subsets made with pyftsubset (hinting removed, kerning kept): 548 KB (full files about 2.7 MB). Sources
   and commands in `fonts/SOURCES.txt`; licences `fonts/LICENSE-DejaVu.txt`, `fonts/LICENSE-Apache-2.0.txt`.
2. On Windows Consolas/Cascadia and Segoe UI (and Segoe UI Symbol) stay first when present, so the look is unchanged;
   everywhere else the bundled set.
3. arboard cannot write custom formats: on Linux/macOS cells travel in-process (copy and paste in one program keeps
   cells; other programs get text). The notebook is single-instance, so that covers the usual case.
4. When the system clipboard refuses (Wayland without data-control and without Xwayland, e.g. WSLg), the text goes
   through egui's copy command.
5. One shared `run_native`; `gpu.json` shared between the two apps.
6. `dirs` follows each system's rules; a relative `XDG_CONFIG_HOME` is ignored (its specification).
7. (mc-docs) The page checker tolerates last-digit differences of machine reals (relative 1e-9) and equal-length data
   fingerprints, since each system's maths library rounds differently; `update` still writes exact text.

## Linux

WSL Ubuntu, Rust 1.99.1: the whole workspace builds (`--all-targets`, clippy clean); 1243 tests pass (the mc-bench
`memcap` case is skipped: the portable guard does not enforce memory on Linux, so `sum(range(10^9))` grows until
killed; fix: skip it when `!guard.enforces_memory()`, or an `RLIMIT_AS` guard). Binaries link only libc, libm,
libgcc_s; at run time they load Wayland or X11 libraries, Vulkan or EGL/Mesa, optionally fontconfig, and dbus plus an
xdg-desktop-portal (or zenity) for file dialogs. Both windows ran in WSLg (`WGPU_BACKEND=gl LIBGL_ALWAYS_SOFTWARE=1`).
Machine reals differ in the last digit from Windows (glibc vs the Windows runtime); bit-identical results would need
the engine's elementary functions through the pure-Rust `libm` crate (a numeric-policy decision).

## Android

Built and run on an emulator (x86_64, Android 17); arm64 is built, not yet run on a device. The pieces:

- `android::start(app)` (called first in `android_main`, crate `mc-notebook-android`) keeps the activity for
  `gpu::run_native` (`NativeOptions { android_app }`) and names the folders: settings in the internal files folder
  (`dirs::set_app_dir`), notebooks in the app's folder on the shared storage (`dirs::set_documents_dir`), `TMPDIR` in
  the internal `cache` (the system's temporary folder is not writable by apps), `HOME`.
- eframe features per platform in `Cargo.toml`: `android-native-activity` on Android (no Java code; it refuses
  `accesskit`), the desktop set elsewhere. GLES then Vulkan, as the order always said.
- `android::bars()`: the window lies under the status and navigation bars and under the on-screen keyboard; the
  activity reports the free rectangle, the notebook pads its chrome by it and centres dialogs in it.
- `android::native_library_dir(name)`: where the package's native libraries are unpacked, the only folder an app may
  run a program from. The kernel is there as `libmc_kernel.so` (the notebook starts it as a process, as on the
  desktop).
- The activity is `MathCasActivity` (`packaging/android/java/`, the only Java: about 300 lines with the content
  provider): `NativeActivity` plus the share sheet, the clipboard, the document picker and the file an "Open with"
  intent carries. `android.rs` calls its methods through `jni-sys` (text crosses as UTF-8 `byte[]`); `platform.rs`
  is the same set of functions for every system (`share_bytes`, `share_text`, `pick_document`, `take_incoming`,
  `bars`), with stand-ins on a desktop. `clipboard.rs` has the Android clipboard (text; a picture as a PNG another
  app is let read through the provider `SharedFiles`).
- The layout for phones and tablets is `mc-notebook/src/app/mobile.rs` (its NOTES); touch is rewritten in
  `mc-notebook/src/touch.rs`.
- Packaging without Gradle: `packaging/android/build-apk.sh` (javac, d8, aapt2, zipalign, apksigner), `smoke.sh`.

Not done: pasting from other apps (no key asks for it yet), input-method composition (keys arrive one by one, which
the stock keyboard does for Latin text), the documentation window, a build on the build server, a signing key for a
store. "Open with" from a file manager is declared in the manifest but was not tried; the picker was.

## Tests

16 (15 run; `system_round_trip` with `--ignored`, it overwrites the real clipboard): every bundled font parses with
its licence present and the set under 700 KB; chains name loaded fonts; bundled chains cover ASCII, Latin-1, Greek,
maths operators, arrows, letterlike symbols and maths alphanumerics in every face; the code font is monospaced;
clipboard in-process, cells, window fallback, pictures, bitmap layout, PNG round trip; settings folders per system,
atomic writes; the three GPU tests.

## Known gaps

Cells reach other programs only on Windows; a copied picture on Linux/macOS carries only the PNG (one flavour per
copy); theme, icon and settings still live in mc-notebook (mc-docs depends on it for them); egui's Ubuntu-Light and
Hack are still compiled in through eframe's `default_fonts`; DejaVu Sans Mono is wider than Consolas.

## Round nb-ui

- `fonts::FaceFonts` (a face's fonts up to the math font: `glyph(c)` gives advance and ink extent in ems from the first
  font that has `c`, `axis()` the middle of its minus sign) and `fonts::FontSet` (all faces of a `FontDefinitions`):
  what typeset cells measure with. `ttf-parser` (MIT/Apache-2.0, already in the lock) is now a normal dependency.
- `icons` (user request: every interface icon generated from TikZ): 26 icons drawn in TikZ
  (`assets/icons/src/<name>.tex`, the common 24 x 24 canvas and 1.8 strokes of `icon-style.tex`, one colour), turned
  into `assets/icons/<name>.svg` by `tools/icons.ps1` (latex, dvisvgm without fonts; committed, so building never
  needs TeX) and compiled in. `icons::image(ctx, icon, size, tint)` (an egui image for buttons and labels),
  `icons::paint` / `paint_turned` (a painter's square, on device pixels; the spinner turned): each icon is rasterised
  with resvg at its size times the display scale as white coverage, uploaded once per (icon, pixel size), at most
  256 textures, and tinted at paint time. `resvg` 0.49 (Apache-2.0/MIT, already in the lock) is now a dependency.
  Tests: every source has its SVG and every icon its source; every SVG parses, is square, has ink and a margin.
