# mc-ui

Window support shared by the MathCAS notebook (`mc-notebook`) and the documentation window, so both behave the same on
Windows, Linux and Android.

## Modules
- `fonts`: the font faces both windows use and their set-up: a code font, a text and interface font and symbol
  fallbacks, bundled with their licences under `fonts/`; on Windows the system's Consolas and Segoe UI are preferred
  when present.
- `clipboard`: the system clipboard behind the `SystemClipboard` trait, with one implementation per platform (text
  everywhere, pictures and the notebook's private cell format where the platform supports them) and an in-process
  copy as the fallback.
- `gpu`: which graphics interface and GPU the window draws with, with a memory of interfaces that crashed an earlier
  start, and `gpu::run_native`, the start-up both programs share.
- `icons`: the icon set of both windows, rasterised at the display's scale, cached as textures and tinted with the
  theme's colours.
- `dirs`: the settings folder of the platform and atomic file writes for what is kept there.
- `platform`: what a phone's system does for an app and a desktop does not, behind one set of functions.
- `android` (Android only): the activity the window opens in and the folders the system gives the app.

## Design
- The windows are built on `eframe` and `winit`; this crate holds only what both windows need, with no notebook logic.
- Platform differences sit behind small traits and functions, chosen per target in `Cargo.toml` and with `cfg`.
- The math font comes from `mc-mathfont`; pictures are decoded with `png` and `resvg`.

## Tests
- Unit tests in `src` cover the clipboard, folders, fonts, GPU choice and icons.
- Run them with `cargo test --release -p mc-ui`.
