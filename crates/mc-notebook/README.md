# mc-notebook

The MathCAS notebook window: an egui application around the document model of `mc-nbmodel`, a client for the
`mc-kernel` process, the Atom One light and dark themes and a self-test mode. `mc-notebook [file.mcn]` opens a
notebook.

## Modules
- `app`, `view`, `dialog`, `tabbar`, `about`, `theme`, `settings`: the window chrome, the notebook canvas, dialogs,
  tab bar, About page, colours and settings kept between sessions.
- `input`, `touch`, `zoom`: egui keys mapped to the model's chords, touch-screen gestures, magnification.
- `kernel`: the kernel process started on the first evaluation, with a reader thread turning its output into messages.
- `typeset`, `highlight`, `images`, `outputs`: painting typeset boxes, syntax colouring, pictures decoded on a worker
  thread, and outputs drawn for other windows.
- `plothover`, `rotate3d`: coordinates under the pointer on plots, turning 3D pictures.
- `instance`: single instance through a loopback socket.
- `selftest`: scripted runs that inject input and write logs and screenshots.

## Design
- The model lives in `mc-nbmodel`; this crate only converts input into commands and paints the result.
- Evaluation always runs in a separate kernel process, never inside the window.
- Typesetting and layout come from `mc-boxes`, `mc-typeset` and `mc-mathfont`; files from `mc-nbfile`; export from
  `mc-nbexport`; widgets and folders from `mc-ui`.
- Per-cell layout is cached by revision, and cell positions are prefix sums.
- The same code runs on Android through `mc-notebook-android`.

## Tests
- Integration tests in `tests/` run the window headless: self-test scripts in `tests/selftest` check input, clipboard,
  help, typesetting, zoom and mobile layout; further tests cover kernel requests.
- Run them with `cargo test --release -p mc-notebook`.
