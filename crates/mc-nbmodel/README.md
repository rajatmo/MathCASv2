# mc-nbmodel

The document model of the notebook: cells and their styles, grouping, selection, commands, undo, the evaluation queue
and clipboard fragments. It has no user interface and no kernel.

## Modules
- `notebook`, `cell`, `style`, `group`, `tabs`: the document, a cell, cell styles, automatic and manual groups, and the
  tabs of a window.
- `command`, `edit`, `keys`, `text`, `code`, `outbox`: commands, their application (typing, motion, selection,
  clipboard, cell operations), the key table, positions in text, code cells as box trees, and the caret in typeset
  output.
- `eval`, `manip`, `printed`: the evaluation queue and kernel protocol, interactive controls (`manipulate`), and
  line breaking of printed output.
- `io`: the model and the `.mcn` file.
- `env`, `fake`, `script`: the surroundings the model asks for (`Env`), a scripted kernel and environment, and the key
  script runner.

## Design
- The window turns keys, menus and mouse actions into `Command`s; `Notebook::apply` runs a command and
  `Notebook::pump` exchanges messages with the kernel through a `KernelLink`.
- One key table (chord, context, command) is shared by the window, the key scripts and the help.
- Text positions are byte offsets on character boundaries.
- Every change goes through the notebook's primitives, which keep undo and the derived groups consistent.
- Depends on `mc-nbfile` (the file), `mc-boxes` and `mc-typeset` (typeset content), `mc-edit` and `mc-syntax`.

## Tests
- Integration tests in `tests/` cover the key table, tabs, file round trips of the documentation, typeset editing with
  undo, interactive controls and a fuzz test, with scripted keys run against a fake kernel; unit tests are in `src`.
- Run them with `cargo test --release -p mc-nbmodel`.
