# mc-notebook: the notebook window (notebook round, P6)

eframe/egui 0.37 (wgpu), a library plus the `mc-notebook` binary. Milestones M1 and M2 of
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

## Tabs, pointer, caret and polish (package nb-polish)

- **Tabs**: the tab model is `mc_nbmodel::tabs` (order, active tab, Untitled-N numbering, close one/others/to the
  right with per-tab unsaved prompts, window close asks once, reopen closed `Ctrl+Shift+T`, an already open file
  activates its tab; 19 tab scripts in `mc-nbmodel/tests/tabs/`). Tab bar (`tabbar.rs`): modified dot, close button,
  middle-click close, drag to reorder, context menu (Close, Close others, Close to the right, Copy path, Reveal in
  Explorer), path tooltip, busy spinner, overflow arrows + list + wheel, "+" for a new notebook. One kernel per tab,
  started on its first evaluation; closing a tab shuts its kernel down. Keys: `Ctrl+N`, `Ctrl+O` (several files),
  `Ctrl+W`, `Ctrl+Tab`/`Ctrl+Shift+Tab`, `Ctrl+PageDown`/`PageUp`, `Ctrl+1..7` and `Ctrl+9` (last); `Ctrl+8` stays
  "toggle initialization cell" (the reference binding). The typesetting commands keep their shifted spellings
  (`Ctrl+@ % ^ &`); M6 will need other keys for "next expression" and "convert to traditional" (key table `keys.rs`).
  Closing the last tab leaves a fresh Untitled tab; a blank Untitled tab is replaced by the next file opened.
- **Single instance** (`instance.rs`): a loopback socket whose port and random token are in
  `%APPDATA%/MathCAS/instance.json`; a later launch with files hands them over and exits, else opens its own window.
- **Session restore** (default on): open file tabs, active tab, scroll and caret; window size and position remembered.
- **Pointer**: I-beam over cell content only; arrow over brackets, margins, labels and gaps (egui has no insert-cell
  cursor); hand over buttons, links and "+"; closed hand while dragging cells or tabs; hidden while typing.
- **Caret**: on the baseline at the font's height, snapped to device pixels, 2 px at 100 %, 530 ms blink (setting to
  turn off), shown at once after keys, clicks and scrolls, hidden while text is selected; empty cells get a full line
  (was a dot); fixed a crash selecting from a line end; insertion line full width, a steady hairline (it blinked
  first: distracting, user decision), with a "+" menu (Input, Text, Other style); drag selection auto-scrolls; IME composition shown underlined with the caret position reported;
  bracket tooltips (style and label).
- **Look**: labels on the first baseline 10 pt from the content, crisp bracket paths, hairline rules visible in One
  Dark, empty-notebook hint, flat toolbar with tooltips and a kernel menu, dialog buttons with faces, menu check marks
  and right-aligned shortcuts, slim scroll bar, About dialog with the icon; notebooks open at the top.
- Self-test steps added: `ppp`, `move`, `rclick`, `mclick`, `shiftclick`, `tripleclick`, `drag`, `newtab`, `sel`,
  `tabs`, `about`, `ime`, `imecommit`; `MCNB_CONFIG_DIR` gives tests their own settings folder.
- Gaps: no pixel comparison with the reference (by eye); the caret on Segoe UI headings reaches slightly above cap
  height; 100 % shots emulate the scale; input methods tested with injected events only.

## Rendering and text size (after a machine reset)

- A machine (Ryzen 7700X, Radeon iGPU plus RX 6750 XT, driver 32.0.21036.18) reset while the notebook was open: two
  GPU timeouts (LiveKernelEvent 141) and an IOMMU fault from the RX 6750 XT 30 s before. The window's own work per
  frame is under 2 ms and nothing in the paint code is unbounded, so the driver hang is the likely cause, but it was
  not reproduced. Since then the notebook draws with wgpu on **Direct3D 12** (`main.rs`; eframe's default may pick
  Vulkan), on the discrete GPU that can present to the window, else integrated, else the software renderer
  (`choose_adapter`); explicit vsync; if no Direct3D 12 device starts, the default choice. `WGPU_BACKEND`
  (`vulkan`, `dx12`, `gl`) overrides the backend.
- With the RX 6750 XT disabled, any eframe window (a bare probe too) ran at about 6 frames per second with a core
  busy: the machine's graphics state, not the notebook.
- **Text size**: the styles' sizes are typographic points; `View::zoom` is the magnification times `PX_PER_PT`
  (96/72), so a 13 pt Input cell is 17.3 px at 100 % (it was 13 px, 3/4 of the intended size).
  `View::magnification`/`set_magnification` are the user's value (menus, Ctrl+wheel, settings).
