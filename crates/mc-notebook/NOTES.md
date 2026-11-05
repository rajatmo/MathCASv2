# mc-notebook: the notebook window (notebook round, P6)

eframe/egui 0.37 (glow), a library plus the `mc-notebook` binary. Milestones M1 and M2 of
`docs/design/08-notebook.md` are done; M3 except visual indentation, operator glyph rendering and line wrapping of
inputs.

- **Canvas**: virtualised; cell heights cached by revision; only visible cells laid out, plus 64 per frame refining the
  estimates of the others; positions by an O(n) prefix-sum pass per frame (cheap at 1,000 cells; a Fenwick tree is
  not needed yet).
- **Painting**: labels `In[n]:=` / `Out[n]=` / `In[*]:=` in small italics; nested brackets with hover, selected,
  queued, running, error and closed marks; the insertion line with its "+" button; caret, text selections,
  Section/Program rules, item bullets and numbers, Code cell background.
- **Themes**: One Light, One Dark, System (follows the OS live), palette and roles of design section 8.
- **Fonts**: Consolas, Consolas Bold, Segoe UI (regular, semibold, italic), Segoe UI Symbol from the Windows font
  folder, egui's fonts as fallback.
- **Chrome**: menus, toolbar, status bar (section 10), context menu, dialogs (other style, initialization cells,
  interrupt, unsaved changes, errors, kernel log, keyboard help generated from the key table, about).
- **Files and settings**: open, save, save as, save without outputs, export script, open recent; File > New and
  opening a second notebook start a new window process (one kernel per window); settings in
  `%APPDATA%/MathCAS/notebook.json`.
- **Clipboard**: text plus the private `MathCAS.Cells` format on Windows (`clipboard-win`).
- **Kernel client**: `mc-kernel` next to the executable, or `MATHCAS_KERNEL`; started on the first evaluation without
  a console window, in the notebook's folder; sends `values: "each"` and `numbered: true`; Quit is `shutdown` with
  `interrupt: true` and a kill after 2 s; stderr goes to the kernel log; `names` answered on a separate channel.
- **M3**: colouring through `mc_syntax::colour` (cached per cell revision, kernel names make undefined globals show;
  names refreshed after each evaluation; every global plain without a kernel), bracket match at the caret, completion
  popup (Ctrl+K/F2 or after two identifier characters and 300 ms; kernel names, keywords, `names_in_scope` locals;
  prefix matches first, then fuzzy), find bar (Ctrl+F, Enter/F3, Shift+F3, Ctrl+E).
- **Self-test mode**: `MCNB_SELFTEST=<script>` injects keys, text and clicks through the raw input hook, takes
  screenshots, probes frame times, opens/saves/dumps the notebook.

Decision: galleys are not kept across frames, only heights (egui rebuilds its glyph atlas, and kept galleys drew
garbage after a theme switch).

## Tests and measurements

4 unit tests (decoding, colour roles, alias runs) and 1 integration test with the real kernel (opens
`examples/notebook.mcn`, evaluates it through `mc-kernel`, compares with `mc-repl --update` modulo labels and block
markers; skipped when `mc-kernel` is not built).

Self-test probe, generated 1,000-cell notebook, release, 1000x760 window (the window's own work per frame, GPU
painting excluded): generation + first layout 0.4 ms; first frames median 0.33 ms, max 1.4 ms; typing at the top
median 0.24 ms, max 0.68 ms; scrolled to the middle 0.21 ms; typing at the end 0.25 ms; save 10.2 ms; reopen 0.9 ms.

## Known gaps

- Rest of M3: visual indentation, operator glyph rendering and spacing, line wrapping of inputs beyond egui's word
  wrap; signature-based colouring classes.
- Keypad Enter is not distinguishable from Enter in egui (Shift+Keypad Enter unreachable from the keyboard); no "Kill
  kernel" offer after 3 s without an answer to an interrupt; Ctrl+click on brackets not wired to the mouse.
- Drag and drop of cells implemented but not exercised in a screenshot; accessibility nodes (9.7) not done; golden
  screenshot comparison not done; scrolling far past the end can overshoot while estimated heights refine.
- M4 (graphics, large outputs), M5 (typeset output), M6 (2D input), M7 not started: `display` responses show as
  their printed text; alias templates stay marked.
