# mc-notebook: the notebook window (notebook round, P6)

eframe/egui 0.37 (wgpu), a library plus the `mc-notebook` binary. Milestones M1 and M2 of
`docs/design/08-notebook.md` are done; M3 except visual indentation, operator glyph rendering and line wrapping of
inputs.

- **Canvas**: virtualised; cell heights cached by revision; only visible cells laid out, plus 64 per frame refining the
  estimates of the others; positions by an O(n) prefix-sum pass per frame (cheap at 1,000 cells; a Fenwick tree is
  not needed yet).
- **Painting**: input and output number labels (and a pending mark) in small italics; nested brackets with hover, selected,
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
- Drag and drop of cells implemented but not exercised in a screenshot; accessibility nodes (9.7) not done; comparison
  with stored screenshots not done; scrolling far past the end can overshoot while estimated heights refine.
- M4 (graphics, large outputs), M5 (typeset output), M6 (2D input), M7 not started: `display` responses show as
  their printed text; alias templates stay marked.

## Tabs, pointer, caret and polish

- **Tabs**: the tab model is `mc_nbmodel::tabs` (order, active tab, Untitled-N numbering, close one/others/to the
  right with per-tab unsaved prompts, window close asks once, reopen closed `Ctrl+Shift+T`, an already open file
  activates its tab; 19 tab scripts in `mc-nbmodel/tests/tabs/`). Tab bar (`tabbar.rs`): modified dot, close button,
  middle-click close, drag to reorder, context menu (Close, Close others, Close to the right, Copy path, Reveal in
  Explorer), path tooltip, busy spinner, overflow arrows + list + wheel, "+" for a new notebook. One kernel per tab,
  started on its first evaluation; closing a tab shuts its kernel down. Keys: `Ctrl+N`, `Ctrl+O` (several files),
  `Ctrl+W`, `Ctrl+Tab`/`Ctrl+Shift+Tab`, `Ctrl+PageDown`/`PageUp`, `Ctrl+1..7` and `Ctrl+9` (last); `Ctrl+8` stays
  "toggle initialization cell". The typesetting commands keep their shifted spellings
  (`Ctrl+@ % ^ &`); M6 will need other keys for "next expression" and "convert to typeset" (key table `keys.rs`).
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
- Gaps: no pixel comparison of the shots (checked by eye); the caret on Segoe UI headings reaches slightly above cap
  height; 100 % shots emulate the scale; input methods tested with injected events only.

## Graphics, text size, input storms

- **GPU and interface** (`gpu.rs`): the integrated GPU first (user decision: a notebook needs little, and the
  discrete card stays out of it), then discrete, virtual, software; `MCNB_GPU=discrete|software`. Interfaces in order
  of trust: Windows Direct3D 12 then GL (Vulkan only through `WGPU_BACKEND=vulkan`: AMD's Vulkan driver
  `amdvlk64.dll` 32.0.21036.18 crashed a bare eframe window with an access violation on the test machine, and a
  machine reset with GPU driver timeouts while the notebook ran with eframe's default choice, which can pick Vulkan
  on the discrete card); Linux Vulkan then GL; Android GLES then Vulkan; Apple Metal. `WGPU_BACKEND` names one.
  Explicit vsync. A start that fails without a device tries the next interface; a driver crash ends the process, so
  each start marks the interface in `gpu.json` (settings folder) and clears the mark after the second frame: a mark
  found at the next start means a crash, and that interface is skipped from then on (`gpu.json` also records the
  last adapter; delete it to forget).
- Measured with a bare eframe window (Ryzen 7700X iGPU, 2560x1440): Direct3D 12 59 fps, GL 59 fps, Vulkan crash.
  While the discrete card was disabled after the reset every window ran at about 6 fps with a core busy; that
  cleared once the card was back.
- **Text size**: the styles' sizes are typographic points; `View::zoom` is the magnification times `PX_PER_PT`
  (96/72), so a 13 pt Input cell is 17.3 px at 100 % (it was 13 px). `View::magnification`/`set_magnification` are the
  user's value (menus, Ctrl+wheel, settings).
- **Input storm** (`tools/nb-storm.ps1`, Windows): real window messages (characters, keys with modifiers set on the
  window's input queue, moves, clicks, double clicks, drags, wheel, Ctrl+wheel, resizes, minimize/restore,
  Shift+Enter evaluations) at a set rate into a job object with a memory cap; samples CPU, memory, handles and
  threads each second and stops on a window that does not answer within 5 s; `-Check` confirms modifiers get
  through. Shortcuts opening system file dialogs, quitting or touching the clipboard are not sent.
- Storm findings, fixed: (1) Ctrl+wheel set the magnification continuously, so every wheel event made a new text
  size and egui's glyph atlas filled with them (+600 MB); the wheel now steps through the menu's magnifications, one
  per notch. (2) egui's atlas grows to the GPU's largest texture (8192: 256 MB, plus its GPU copy) before clearing at
  80 % full; `raw_input_hook` caps egui's textures at 4096 (64 MB), the window surface is unaffected. (3) A minimized
  window's place (-32000 on Windows) was saved as the window's place; minimized, maximized and tiny rects are no
  longer saved and implausible saved places are ignored.
- Portability (round 10, `mc-ui`): fonts are bundled (DejaVu Sans Mono, Noto Sans, symbols, STIX Two Math; Consolas
  and Segoe UI first on Windows when present); the clipboard is `mc_ui::clipboard` (the cell format on the system
  clipboard on Windows only, in-process elsewhere); `gpu.rs` and the settings folder moved to `mc_ui::gpu` and
  `mc_ui::dirs`. The storm driver is still Windows-only.

## Graphics and large outputs (milestone M4)

- **Pictures** (`images.rs`, design 6): an Output cell whose value is `image("path")` shows the picture (PNG, or SVG
  rasterised with `resvg` at the display's pixel width in 64 px steps, text in the system fonts loaded once on the
  worker). Decoding runs on one worker thread; the result is uploaded once as a texture keyed by path, modification
  time and raster width; 256 MB of pixels at most, least recently drawn evicted; sides above 4096 scaled down while
  decoding, pictures above 100 Mpixel refused. Size: `size=WxH` (points at 100 %) or the picture's pixels, times the
  magnification, never wider than the cell. Relative paths are relative to the notebook's folder. A missing picture
  is a framed `path: why`. An older raster shows while a new one decodes (no flicker on zoom).
- **Selection and resizing**: a click selects the picture (frame in the accent colour, eight handles); corners keep
  the ratio, sides too unless Shift is held (one dimension); the release stores `size=` (an undo unit,
  `Command::ResizeGraphic`); double-click or the context menu's "Natural size" removes it. Esc and arrows deselect;
  Delete removes the output; typing does not reach the text behind the picture. Copy (Ctrl+C, context menu) puts the
  PNG (registered format "PNG"), a bitmap (CF_DIB, laid on white) and the `image(...)` text on the clipboard.
- **Saving** (design 7.5): `mc_nbfile::images::store_images` copies pictures into `<name>.files/` under content-hash
  names before the text is written, `remove_unreferenced` deletes stray files afterwards, the model takes the new
  paths (`Notebook::rename_values`). Save As to another folder copies them there.
- **Outputs**: Output text is broken at the cell width by `mc_nbmodel::printed::wrap` (spaces chosen by bracket
  depth, then after `,`/`;`, then before `+ -`, comparisons, `* /`, `^`), as a same-length text so positions carry
  over. Outputs over 100,000 characters show a sample (2,000 characters, `«n»` for elided elements) under a title,
  with Show less / Show more / Show all; only the lines on the screen are laid out (a 2.3 MB output shown in full
  paints in 0.2 ms a frame). A click on a sample selects the cell; copying copies the whole output.
- **Help**: Help > Documentation and F1 (the name at the caret or the selection) start `mc-docs [name]` from
  `MATHCAS_DOCS` or next to the executable; a dialog says so when it is missing.
- Self-test steps added: `hit <x> <y>` (what is at a point), `images` (cache size). `tools/nb-storm.ps1` copies a
  notebook's sidecar folder with it.

## Typeset output and input, plot coordinates, closing brackets (round 10)

- **Painter** (`typeset.rs`, NB5.4): box layouts from `mc_boxes` are measured and drawn entirely with the bundled
  math font (letters and digits too; characters it lacks fall back to the cell's text font through egui). Glyphs are
  rasterised at device pixels with four sub-pixel phases into one atlas texture of our own (1024², grows once to
  2048², cleared and refilled when full), uploaded once per glyph; each frame the visible glyphs and rules of a layout
  become one mesh.
- **Typeset output** (M5, NB5.5/NB5.6): Output cells show `mc_typeset::typeset_output(printed)` in display style at
  the cell width with the layout's own line breaking; the setting "Typeset output by default" (Format > Output form)
  and per output `form=linear|typeset|traditional` (`Ctrl+Shift+N`, `Ctrl+Shift+I`, `Ctrl+Shift+Alt+N`). A typeset
  output is one unit for the caret (before or after it; a double-click or a drag selects all of it; copying copies
  the printed text). Failures, pictures, outputs over 100,000 characters (`TYPESET_LIMIT`) and large-output samples
  stay linear. Outputs over 3,000 characters are typeset on a worker thread (linear text, or the layout at the old
  width, shows meanwhile); off-screen height refinement stops after 4 ms a frame.
- **Typeset input** (NB5.3): see mc-nbmodel; the view lays out typeset Input cells with `LayoutOptions::input`,
  hit-tests clicks into box caret positions (`Hit::Boxes`, `View::box_hit`), answers `Env::box_line` (Up/Down across
  wrapped lines) and draws the editor's caret, selection and alias marker. No completion in typeset cells.
- **Plot coordinates** (`plothover.rs`, NB5.2): a picture with a `plot-N.json` description shows the nearest point
  (12 pt snap, 4 pt vertex snap on lines) with a marker, guide lines and a tooltip, else the pointer's data
  coordinates (linear, log, signed_log axes).
- egui's focus navigation is cleared every frame unless a dialog or the find bar is open (Tab had left the new-tab
  button focused, which then took Enter).

## Round 11

- **Brackets on the left, no labels** (user decision, NB6.1): the bracket column moved to the left margin, cell
  brackets nearest the content, enclosing groups further left (at least two group columns reserved so the content does
  not jump when the first group appears), hooks pointing right. The content is left-justified 8 pt (scaled with the
  magnification) right of the innermost bracket; the styles' left margins count from the Input margin (66), so Title,
  Section, Text, Input and Output share one edge and items keep their indents. Input and output number labels are no
  longer drawn (the numbers stay in the model and file; an output's bracket tooltip and the status bar say `out(n)`). The
  insertion line runs from the content edge to the right, its "+" at its right end, clear of the group brackets.
- **Modules** (NB6.11): `app.rs` (2,664 lines) is split by concern without behaviour change: `app.rs` (state, `new`,
  the frame, self-test accessors), `app/tabs.rs` (tabs, session), `app/files.rs`, `app/commands.rs` (model commands,
  window actions, F1, picture copy, magnification), `app/keyboard.rs` (keys, completion, find), `app/canvas.rs`
  (scrolling, painting, pointer, mouse, IME, context menu), `app/chrome.rs` (menus, toolbar, status bar),
  `app/dialogs.rs`, `app/prefs.rs` (preferences).
- **Typeset input** (M6 rest): characters coloured with the syntax classes of the cell's canonical text, placed
  through `mc_boxes::to_text_mapped` (cached per revision, names, theme, width; `typeset::Ink::colours`); completion
  in typeset cells (the name before the caret in its row, `Notebook::box_word`; accepting runs
  `Command::ReplaceWord`, which also replaces the old two-click selection in plain cells); selections are drawn line
  by line when they wrap.
- **Typeset output** (NB6.4): a click puts a caret between the items of an output (`Hit::Boxes` for outputs too);
  arrows, Shift, `Ctrl+.`, double-click and drags select parts; copying gives the part's printed text. Find (now
  `Command::SelectText`) highlights the matched part in typeset outputs and typeset inputs. Rational coefficients as
  one fraction and matrices as grids come from mc-boxes; the setting "Lists of equal-length lists as matrices" (on)
  is in the preferences.
- **Copy as LaTeX** (M7): Edit > Copy as > Plain text (`Ctrl+Shift+C`) / LaTeX, and in the context menu.
- **Preferences** (Edit > Preferences...): theme, magnification, fonts (system where available / bundled only),
  automatic closing, caret blink, typeset output by default, matrices, save without outputs, session restore,
  toolbar; applied at once and saved (`notebook.json` gains `matrices`, `bundled_fonts`).
- **Keyboard and accessibility** (NB6.9, NB6.10): F10 opens the File menu, Alt+F/E/I/O/C/V/W/H the menu of that
  letter, its first item focused (a focus ring is drawn; arrows, Enter, Esc work as egui's focus navigation does);
  Enter/Esc answer the initialization, interrupt and error dialogs; Esc closes the kernel log and the keyboard help.
  AccessKit nodes: every cell on the screen (headings with level for title and section styles, paragraphs, multi-line
  text inputs for Input/Code with the linear text as value, status for outputs named `out(n)` with the printed text,
  log for Print/Message, image for pictures; selected state; bounds), the tabs (role tab, selected) and the tab-bar
  buttons; a polite live region announces "Evaluation finished" and "Kernel stopped". Toolbar, menus and dialogs are
  egui widgets with their own nodes.
- Self-test steps added: `prefs` / `prefs off`, `clip` (clipboard text to the log), `focus` (focused widget, popup
  open). Run self-tests with `MCUI_CLIPBOARD=local` so they never touch the real clipboard.
- Tests: 15 unit tests (new: the accessibility tree of a five-cell notebook, M7's snapshot); the kernel integration
  test.
- Gaps: no Narrator walk-through recorded (the tree is tested, not read aloud); cells hang flat under the window's
  root node (egui offers no public parent registration for nodes made with `accesskit_node_builder`); focus visuals
  in menus show only while the window has the system focus (so self-test shots do not show them); the keyboard
  menus are egui's own (no type-ahead); completion in typeset cells offers the same names as in text cells.
- **Kernel requests never block the window** (found by the `all` storm mix: once in six runs the window stopped
  answering for 5 s): requests to the kernel go through a channel to a writer thread of their own; before, a busy
  kernel that did not read its input could fill the pipe and block the window's thread in `write`.
- Status bar: the file path takes at most half the bar (a long path covered the items on the left).
- **No egui fonts**: `mc-ui`, `mc-notebook` and `mc-docs` declare eframe without
  `default_fonts` (every other default feature kept; `winit/default` through a direct `winit` dependency of mc-ui),
  so `epaint_default_fonts` (Ubuntu Light, Hack, egui's emoji and icon fonts) is no longer built; menu check marks
  are `✓` (in the bundled symbols) instead of `✔`; an mc-ui test checks that every symbol the windows draw as text has
  a bundled glyph in every face.

## Alignment, one font, selection (round nb-ui)

- **Outputs right-justified** (NBUI.1, user decision): an Output cell's lines end at the content's right edge. Typeset
  outputs: `Layout::align_right(width)` after the layout (also on the background worker); linear outputs: the galley
  is laid out with `halign = RIGHT` and anchored at the right edge (`View::text_dx`), which caret rectangles, hits,
  selections, `x_of`/`pos_at` (Up/Down) and the caret painting add; pictures: `graphic_rect` puts the right edge on the
  content edge (plot read-outs and resize handles follow); large outputs: title, sample lines and buttons end at the
  edge, the sample's rule stands left of its widest line. Print and Message cells stay left (program output read as a
  log); an output showing its source stays left (it is edited text).
- **One font in cells** (NBUI.2, user decision): `typeset::TextFont` (the cell face's fonts from `mc_ui::fonts::FontSet`,
  egui's row height and baseline of the face at the cell's size) and `typeset::CellMetrics` (implements
  `mc_boxes::Metrics`: text-font glyph extents and advances for every character the face's chain has before the math
  font, the math font's for the rest; `line` = egui's row, `line_pitch` = the row height, `axis_height` = the middle
  of the font's minus sign, `italic_letters` false). The painter draws text-font characters as egui galleys, one per
  run of characters that follow each other on a baseline at one size (so they are placed and rasterised exactly as
  plain text is, colours per character as sections), and only math-font characters, stretched delimiters, radicals,
  big operators, placeholders and rules through the glyph atlas. The window's `FontSet` lives in `Glyphs` (shared by
  the tabs) with a generation number that is part of every typeset layout key and height key, so changing fonts in the
  preferences re-lays out every tab.
- **Calm Ctrl+6** (NBUI.3): typeset Input cells lay out with `operator_spacing` off (no thin spaces, `->` stays two
  characters) and `typeset::keep_baseline` lets the first line's scripts reach into the top margin (at most 80 % of
  it and half an em) and the last line's into the bottom margin, so the first baseline stays where the plain text's
  is. Measured: the window's pixels of `f(a) = a+b*c-d+x` before and after `Ctrl+6 "2"` are identical left of the new
  superscript (before this round the whole line re-fonted in STIX and moved 7 px down).
- **Selection** (NBUI.4): bugs found by self-test scripts and fixed: Shift+Up/Down dropped the selection in typeset
  input (now: anchor kept, structural move, else visual line, else to the cell's start/end) and left typeset outputs
  (now extend inside); Home/End jumped over line breaks and wrapped lines (now the visual line: `Env::box_line_edge`,
  `View::box_line_edge`; the editor's own Home/End stop at line breaks); triple-click acted as a double-click (now the
  visual line, `View::box_line_items`); a drag leaving a typeset cell stopped or jumped to another cell (now
  `View::box_hit` clamps: above the boxes the start, below them the end, and the drag stays in its cell); the
  highlight spanned the whole line height (now `tighten`: the ink of the selected items, text characters at their
  row height); Shift+click took the frame's final modifiers (now the press's). Word/innermost-box double-click, Ctrl+A,
  copy/cut text, typing over a selection, Delete/Backspace on a selection were checked and were right.
- Self-test: `boxes` logs the box caret or selection (input or output) in the box notation; double and triple clicks
  arrive one click per frame; modifiers are released a frame after a key chord or a Shift+click (before, a later
  click inherited Shift).
- Tests: 16 unit tests (new: right-justified geometry of typeset and linear outputs, prints and inputs on the left).

## One kind of cell (round nb-ui, NBUI.5)

- Outputs are always drawn by the cell renderer over `mc_typeset::math_text` (failures, pictures, outputs over the
  typeset limit and multi-line tables stay text); layout options without operator spacing or small inline rationals,
  so the printed characters keep their places. Removed: Format > Output form, the preferences "Typeset output by
  default" and "Lists of equal-length lists as matrices" (`typeset_output`/`matrices` settings and view fields),
  `View::clear_boxes`.

## Help while writing code (round 12)

All help reads the help index (`mc_help`: the catalogue plus the pages' usage lines), shared with the documentation
window and the language server. The code is in `app/help.rs` (hint, hover, F1's target) and `app/keyboard.rs`
(completion).

- **F1** (and Help > Look up, renamed from "Find selected function") looks up the selected name, else the
  documented name at the caret, else the function whose call the caret is in (`sin(x|)` looks up `sin`), else the
  name at the caret; in a cell with boxes it reads the canonical text at the box caret. It starts `mc-docs <name>`,
  which hands the name to an open documentation window (single instance, `docs-instance.json`) and brings it to the
  front, or opens one (decision NBDOCS.1). `MATHCAS_DOCS=-` only records the request (self-tests, storms).
- **Usage hint**: inside the brackets of a call of a documented function, a popup above the caret's line (below it
  when the line is near the top of the canvas) shows the usage call that fits the arguments typed so far, the
  parameter being typed underlined in the accent colour, "k of n" when there are several calls, and what the call
  gives. Esc hides it for that call (Esc is taken only while the hint shows; the next Esc starts an alias as
  before). Not shown while the completion list is open. In cells with boxes the calls are read in the canonical
  text; a square root box (`((sqrt(...)))` underneath) is not taken for a typed call.
- **Completion**: the candidates are the keywords, the locals in scope, every documented function (always, without
  a kernel; completion no longer starts a kernel) and, from a running kernel, the session's own names. Each row shows
  the name and its one-line usage (keywords and locals their kind). Prefix and word-prefix matches first;
  subsequence matches only when there are none (NBDOCS.6: with 700 names they buried the useful ones). Up/Down
  choose, Tab or Enter accept, Esc cancels; accepting a function inserts `(` too (with automatic closing on, `()`),
  unless a bracket follows. Enter, Tab and Esc are taken only while the list is open.
- **Hover**: a documented global name in an Input or Code cell (plain text or boxes; locals and parameters of the
  same name excluded by the colouring classes) shows its usage line after a 0.45 s rest, with "Ctrl+click opens its
  page"; Ctrl over such a name shows the hand pointer; Ctrl+click opens the page (as F1) instead of placing the
  caret. No tooltip while typing (the pointer is hidden then).
- Fixed: replacing a tab's notebook (`set_notebook`: opening a file into a blank tab, the self-test's `new`) passed
  the view's pixel zoom as the magnification, so the text grew by a third each time.
- Self-test steps added: `help` (completion list, hint, hover, pages asked for), `ctrlclick <x> <y>`,
  `hovername <name>`, `ctrlclickname <name>`. The script `tests/selftest/help.txt` covers completion (open, accept
  with the bracket, cancel, Enter not stolen, Enter accepting), the hint (arguments 1 and 2, Esc), F1, hover and
  Ctrl+click, in plain text and inside a square root box; `tests/help_selftest.rs` runs it and checks the log
  (ignored by default: it opens a window).
- `tools/nb-storm.ps1`: F1 among the keys, Ctrl+clicks, names that open completion and calls that show the hint
  among the texts; `MATHCAS_DOCS=-`.
- Tests: 18 unit tests (new: F1's target), the kernel integration test, the help self-test (ignored).

## Round 13: clipboard with boxes

Copy and paste of square root, fraction and power boxes within and between cells, tabs and windows (design 3.6).

- **Flavours.** A copied range holding a box writes the text flavour (its linear text, `a/b`, `x^2`, `sqrt(x)`, for
  other programs) and a second private flavour, the box flavour: the canonical box text of the range (4.4,
  `((a)/(b))`), UTF-8, only the three boxes. On Windows it is the registered clipboard format `MathCAS.Boxes`
  beside the text (and `MathCAS.Cells` for cells), so it travels between windows; elsewhere it is kept in-process
  and comes back while the system clipboard still holds the text of that copy, as the cells flavour does
  (`mc_ui::clipboard::Contents::boxes`, `mc_nbmodel::Clip::boxes`, `Clip::of_boxes`).
- **Copy.** From a cell with boxes: the box caret's selection (a range without boxes, say inside a slot, writes text
  only). From a typeset output: the selected items turned back to input boxes (`mc_boxes::output_input_boxes`: the
  thin space of a product to `*`, a `^(1/2)` drawn as a root copies as a root, `y^-1` drawn as 1/y as that
  fraction); the text flavour stays the printed text of the part as before. Copy as plain text (`Ctrl+Shift+C`) and
  Copy as LaTeX write text only. Cut writes the same as Copy. Copied cells now give the linear text of their boxes
  as the text flavour (it was the canonical text), with the cells flavour as before.
- **Paste.** The box flavour into a code cell inserts the boxes at the caret, replacing the selection (one undo
  unit); a plain cell takes its first boxes as with a box key, its own characters staying characters
  (`Notebook::paste_boxes`). Into a Text cell or another non-code cell, and into the find bar (egui's text field),
  the text flavour. At an insertion point (or over selected cells) without the cells flavour: one cell of the
  insertion point's style holding the boxes. The cells flavour keeps precedence at insertion points. Text from other
  programs stays text everywhere, in cells with boxes too (`((a)/(b))` pasted is those characters; before, a cell
  with boxes read pasted text as canonical box text).
- the box flavour is restricted to the three boxes when written and again when read (another program's box
  text with a subscript or a grid pastes those as their linear text).
- at an insertion point whose style was set to a non-code style (Text), pasted boxes become their linear
  text in that style: the cell style chosen wins over the boxes.
- pasted boxes get the box editor's implicit `*` where they touch a name (`z` + pasted `x^2` is
  `z*((x)^(2))`), as boxes made by the box keys do.
- a cell with boxes holding pasted text such as `((a)/(b))` keeps it as characters while edited; on reload or
  after undo the canonical reading shows it as a fraction (the known ambiguity of 4.4, same meaning).
- Tests: `mc-nbmodel/tests/keys/clipboard.keys` (17 scripts: fraction into a plain cell, plain characters kept,
  root into the middle of a numerator, nested and box-free ranges, Copy as plain text, cut/paste of a power, paste
  over selections in plain and box cells, at an insertion point, one undo unit, other programs' text with `((` in
  plain and box cells, into a Text cell, copied cells, a root from typeset output, an output part without boxes);
  two scripts of `typeset.keys` updated (Ctrl+C in boxes now pastes boxes; a plain-text variant added). Key scripts
  gained `TypesetOutputs` (Output cells shown typeset, `FakeEnv::typeset_outputs`) and a `--- clip` section.
  `mc-boxes`: `output_parts_copy_as_input_boxes`. `mc-ui`: `boxes_travel_with_their_linear_text` (in-process, as
  with `MCUI_CLIPBOARD=local`, and over a text-only system clipboard); the ignored `system_round_trip` also checks
  the box format. Self-test `tests/selftest/clipboard.txt` run by `tests/clipboard_selftest.rs` (ignored: it opens
  a window): copy a fraction, Copy as plain text, paste into a plain cell of another tab, again into the same cell,
  at an insertion point, cut and paste a power; the self-test step `clip` also logs the box flavour.

## Round 13: labeled and legended values

`labeled(x, l)` and `legended(x, l)` around a value that is not a picture (the kernel shows the printed call, gap
G80) are drawn by the notebook: `mc_typeset::math_text` reads the call into a `Node::Labeled` box (see mc-boxes NOTES),
the cell renderer lays it out like any output (right-justified as a whole) and paints it.

- The label row is painted in the theme's label colour (`Palette::label`, weak text: mono-2), a legend's swatch
  characters (`━━` for `line_legend`, `■` for `swatch_legend`, `●` for `point_legend`) in their own colours, the
  value in the output's text colour (`View::label_colours`: a per-layout-row colour table as typeset input has,
  cached in `box_colours` by revision, theme and width; outputs without `labeled(`/`legended(` skip it).
- Selection, the box caret, Find and copying go through the source map as for any output: copying the whole output
  gives the printed call, a part of the value the value's part, the label its characters.
- positions follow the graphics crate's layout of the same terms: the label
  below by default, `top`/`above`, `bottom`/`below`, `left`/`before`, `right`/`after` (a symbol or a list of one);
  legends at the right, entries one per line, centred on the value. Label and legend at the size of the output text.
- forms not recognised (other sides such as `center`, labels that are calls or lists, unknown legend heads,
  wrong arity, a call that is not the output itself or a top-level list element, values over 20,000 bytes) stay the
  printed text, typeset as before. Options (named arguments: `frame`, `label_style`, ...) are not drawn.
- Tests: `view::tests::labeled_outputs_draw_their_labels_at_the_right_edge` (right edge, label under the value's
  baseline, label and swatch colours, a click on the value lands in the value slot). Screenshots by the self-test
  (One Light and One Dark at 100 % and 150 %) of a notebook with these outputs, outside the repository.

## Round 13: calm Ctrl+wheel magnification (user request)

- Reproduced first (self-test `tests/selftest/zoom.txt` on the old code, 300 generated cells, pointer at y 350): one
  notch did nothing (egui smooths wheel deltas over frames and the step waited for a 1.08 threshold), a burst of
  five notches gave one step, 40 touchpad deltas ran the magnification up to 300 %, every step scrolled to the caret
  (the view jumped from cell 58 to cell 284), and Ctrl+0 did nothing. A smoothed remainder arriving after Ctrl was
  released also scrolled the page.
- Now (`zoom.rs`): Ctrl+wheel and pinch events are taken out of egui's input in `raw_input_hook` (egui neither
  smooths nor scrolls with them) and summed into whole steps of one ladder, 50 % to 300 % in steps of 10 %: a notch is
  one step, a burst is its notch count applied once in its frame (one relayout), touchpad point deltas make a step per
  50 points, fractional lines add up, a pinch steps by its logarithm in 10 % units; a partial step is forgotten after
  0.6 s or on a reversal. `Ctrl+=` (and `Ctrl+Shift+=`), `Ctrl+-` step, `Ctrl+0` resets to 100 % (it was a reserved
  no-op); `Ctrl+Alt+=`/`-` stay. The menus offer rungs of the ladder.
- **Anchor**: a change keeps the content under the pointer (the canvas middle for keys and menus when the pointer is
  elsewhere) in place: the cell under it and the fraction of its height are held for one second (or until a scroll,
  click or key); each frame the cells around the anchor are measured before the scroll area is shown and the offset
  is solved for (at most four passes), and the canvas's own pass refines no off-screen cell (`View::hold`), so cells
  measured at the new size above the anchor cannot move it.
- Tests: 4 unit tests in `zoom.rs` (ladder, notch and burst, touchpad accumulation, forgetting); the self-test
  `tests/zoom_selftest.rs` (ignored: opens a window) checks 100, 110, 160, 180, 150, 100, 110, 100 % and the same cell
  at the same fraction (58 at 0.672) under the pointer after every step.

## Round 13: output alignment, a setting (user request)

- View > Output alignment > Left / Right (check marks; the other choice shows `Ctrl+Shift+J`, which switches), also in
  the preferences ("Outputs: on the left / on the right"); saved as `output_alignment` in `notebook.json`. **The default
  is Left** (user: "the left justification of outputs was probably better"). `View::set_outputs_right` makes every
  layout again (galleys, wrapped outputs, large-output samples, box layouts); everything that follows the geometry goes
  through `View::right_justified(&self, c)` (text anchoring `text_dx`, box layouts `align_right`, background typesetting
  jobs, picture rects and their resizing, large outputs, carets, hits, Find, plot read-outs), so both alignments share
  one code path. The switch keeps the content at the canvas's middle in place (the magnification's anchor).
- A View menu (no Alt letter: V is Evaluation's) holds Output alignment and Magnification (moved from Format).
- Test: `outputs_start_at_the_left_edge_by_default_and_switch` (typeset and linear outputs start at the left edge,
  clicks at it hit the start; Right and back re-lays out); the right-edge tests set Right explicitly.

## Round 13: one file per notebook (user request)

- Saving embeds every picture in its output block (`mc_nbfile::embed::embed`: PNG, plus the deflated `.json`
  description and `.scene3d` scene) instead of copying it into `<name>.files/`; nothing is deleted any more
  (`store_images`/`remove_unreferenced` are gone; an older notebook's folder is left for the user). Opening extracts
  the embedded files into `%TEMP%/mathcas-plots/embedded/` (content-hash names, written once; swept after 30 days
  unused at start) and the document shows them from there, so hover read-outs and 3D rotation work from the file
  alone. Pictures named relative to the old folder are kept by full path after a Save As. The status bar says how
  many pictures were embedded and their size.
- Tests: mc-nbfile `pictures_are_embedded_in_the_file_and_read_back` (round trip, siblings, cache, saving again gives
  the same text, carried pictures kept), `an_older_notebook_with_a_sidecar_folder_is_embedded_on_save_and_the_folder_stays`;
  mc-repl `tests/embedded.rs` (a 2D and a 3D plot through `--update`: one file in the folder, no cache path, five
  embedded files, 312 KB, export finds both pictures, a second run writes the same text).

## Round 13: turning 3D pictures, signed-log read-outs

- `rotate3d.rs`: the scenes of 3D pictures (`mc_graphics::view3d::Scene3d::cached`, read once per picture path), a
  worker thread that renders only the newest request per cell (overtaken ones are dropped), and the textures. A
  picture whose output keeps a view (`view`, `zoom`, `vertical`; `Command::SetOutputAttrs`, an undo unit) or is being
  turned shows the render (the PNG until the first one arrives); scales snap to 1/8 so a resize does not ask for a
  render per pixel. Mouse: a press on a 3D picture starts `Drag::Rotate` (`orbit_drag` from the view at the press),
  the release keeps the view; the wheel over the selected 3D picture is taken out of egui's input (it does not scroll)
  and zooms (`zoom_wheel`), kept 0.4 s after the last notch; a double-click drops the view. No plot read-out over 3D.
- mc-notebook now depends on mc-graphics (its `view3d` needs no evaluator; still never mc-engine).
- Outputs with attributes but no number (written by the headless runner) now keep their attributes when saved (they
  close their block): a size or a view set on such an output was lost before.
- Plot read-outs follow the round-13 signed-log mapping `sign(v) log10(a + k |v|)` when the description gives `a`
  and `k` (older descriptions keep the old mapping).
- Measured (release, 1000 x 700 window): a default `plot3d` re-renders in 16 ms median (19 ms max) at scale 1.5, a
  small `graphics3d` in 2 ms.
- Tests: `rotate3d::tests::views_are_kept_as_attributes_and_read_back`; `plothover` signed-log test extended; the
  self-test `tests/rotate3d_selftest.rs` (ignored: opens a window): drag turns, two notches zoom 1.21, the saved file
  keeps the view and embeds the scene, reopening renders the kept view, a double-click drops it.

## Round 13: File > Export

File > Export > HTML page / PDF document / Script: HTML and PDF through `mc_nbexport` (`to_html`, `to_pdf`) on the
document as shown; 3D pictures that keep a view of their own are drawn from that view (scale 2) for the export.
Self-test step `export <file>`.

## Round 13: interactive controls (`manipulate`)

- Kernel side (`mc_kernel::front_end`, protocol only, decision nb-mat.MAN1): every front-end session (the kernel,
  `mc-repl`, the documentation runner) holds `manipulate`'s arguments; no new protocol message: the notebook reads
  the controls from the printed value and evaluates `{ let a = v; ...; expr }` itself (`KernelRequest::Side`: an
  `eval` with `values: each` and no numbering). Catalogued under Plots (`front_end::FUNCTIONS`), page
  `docs/functions/manipulate.mcn`.
- Model (`mc_nbmodel::manip`): parsing (controls `[a, lo, hi]`, `[a, lo, hi, step]`, `[[a, init], lo, hi(, step)]`,
  bounds as constant expressions), snapping (the step, else a thousandth of the range), positions on the output
  (`controls=`), one evaluation at a time with only the newest waiting position kept, an interrupt for a stale one when
  the slider is let go (`Command::SetControl(cell, k, v, done)`, not an undo unit), the result in the Output cell after
  it (`manipulated`), asked for at the initial positions as soon as the value arrives.
- Window: `Shown::Controls` draws a row per control (name, track filled to the knob in the accent colour, step ticks
  when there are at most 40, the value); `Hit::Control` and `Drag::Control` move it. The cell's bracket shows the
  running mark while its evaluation runs.
- Measured: frames while dragging both sliders with a plot re-evaluating: median 0.33 ms, max 0.51 ms (the window's
  own work).
- Tests: `manip` unit tests (parsing, snapping, sources, constants), `tests/manipulate.rs` (stale positions dropped,
  letting go interrupts), `tests/keys/manipulate.keys` (3 scripts), the self-test `tests/manipulate_selftest.rs`
  (ignored: a window and a kernel).

## Round 13: checks

- `tools/nb-storm.ps1` on a notebook with two `manipulate` outputs and three 3D pictures, 60 s per mix: type, mouse,
  window, eval ok; all (seed 15) found a panic: a magnification step in the frame a notebook was replaced, before the
  canvas had laid it out, indexed the cell positions (`View::cell_rect`); fixed (an unknown cell has an empty
  rectangle), all (seeds 15, 16) ok since. The storm now writes the window's stderr (with a backtrace) to
  `stderr.txt` in its output folder. Memory: the mouse, window and eval mixes grew the window's private memory by
  400-460 MB in 60 s (picture textures of new plot results and 3D renders, within the 256 MB picture cache plus
  egui's atlas); peaks 0.5-1.2 GB with the kernel.

## Round nb-svg: plots as vector drawings in the window's theme (user request)

"Can you make it SVG instead? Plots should look consistent in both themes and sharp at any scale."

- **Vector display** (`images.rs`): a picture whose PNG has an SVG sibling (every plot and graphic), or an SVG, is
  rasterised with resvg at exactly the device pixels it covers (the size it keeps x magnification x display scale;
  no 64 px steps any more) and drawn one texel per device pixel on the pixel grid (`view::pixel_rect`); any other
  texture fills the picture's rectangle. Keys: `images::Key` (file, version, pixel width, theme). The worker keeps the
  last 6 parsed trees (a zoom step or theme switch renders without parsing), does only the newest of the waiting
  requests of one drawing (the others answer `Skipped` and are asked again if still wanted), and the newest older
  raster of the file shows stretched meanwhile (same theme first). `prune` drops textures of a file last drawn more
  than two frames before its newest (a picture shown twice at two sizes keeps both). Resizing stretches the kept
  raster while dragging; the release makes the new one.
- **Theme**: `Palette::plot_theme()` (no background, text = text, frame = weak, grid = 12 % text over the page, legend
  box = page); `View::paint` hands it to the cache each frame; its CSS is injected when a drawing is parsed. 3D
  pictures turned from their scene (`rotate3d.rs`) render with the same theme at the exact device scale of the kept
  size (no 1/8 steps), keyed by theme. The clipboard copy and File > Export keep the light look; a turned 3D picture
  exported is marked 2 pixels per point (`with_density`).
- Status bar after saving: the embedded size as stored (deflated drawings), before base64.
- Self-test `images` also logs the drawings' rasterisation times with their widths.
- Measured (release, Ryzen 7700X, worker thread, parse included on first use): the two-curve plot 13.5 ms at 640 px,
  2.5-5.7 ms at 640-960 px once parsed, 8.7-9.1 ms at 1248-1280 px; the 1 MB SVG of a default `plot3d` 34 ms at
  540 px; a 200 000-point `list_plot` (3.1 MB SVG) 501 ms the first time, 412-442 ms per zoom step after (frames meanwhile:
  median 0.17 ms, max 0.42 ms; the older raster shows). Turned 3D: 19.5 ms median at 1.5 x.
- Screenshots (outside the repository) of a six-picture notebook in One Light and One Dark at 100 % and 150 % display
  scale and at 130 % and 200 % magnification, the hover read-out and a resize at 150 %, a turned 3D picture, the
  dense plot in the session and saved.
- Storms (60 s, notebook with six plots incl. 3D): eval ok (seed 3, 20 673 events, private 438 -> 846 MB, max ping
  32 ms), window ok (seed 4, private 456 -> 761 MB, max ping 47 ms).
- Tests: `images` (exact widths, the theme recolours chrome and keeps data, one tree per theme, cache keys),
  `theme::contrast::plot_chrome_contrasts_in_both_themes`, `view::tests::rasters_are_drawn_on_device_pixels`.
- Gaps: a dense plot embedded as a PNG (over 256 KiB deflated) keeps the light look in One Dark until it is
  evaluated again; the default curve colours of plots were chosen for white and stay as drawn, as asked: on the
  One Dark page deep teal #1B5E63 is 2.0:1, slate indigo 1.8:1, aubergine 1.4:1, espresso 1.1:1 (terracotta and olive
  3.2:1, ochre 5.9:1); chrome text is 10.3:1 (frame 6.3:1) in One Dark, 14.1:1 (6.2:1) in One Light; `export_data` files and HTML/PDF exports stay light
  (no option for a dark export yet).

## Dialogs and menus polish (2026-11-08)

- **One dialog look** (`src/dialog.rs`): every question and settings dialog (unsaved changes, initialization cells,
  interrupt, errors, other style, preferences, keyboard shortcuts, about) is a modal card over a dimmed window: the
  title left-aligned in bold with an optional close button, wider margins, the message beside its icon and wrapped,
  and the buttons at the bottom right with the default action filled in the accent colour (dark text on the dark
  theme's light accent). Enter and Escape keep their meanings per dialog. The kernel log stays a movable window.
- **Menus line up:** every entry keeps the check-mark column, so labels start at one place whether or not some
  entries can be checked; choices (theme, magnification) are check marks instead of radio buttons; menus and
  submenus are at least 220 points wide, so shortcuts never touch their labels; submenu chevrons are weak.
- **Preferences:** each label sits level with the first line of its options (labels of several-line groups no longer
  float between them), and the magnification list uses the toolbar's dropdown marker.
- The native file dialogs were checked with and without a Common Controls 6 manifest: Windows draws them in the
  current style either way, so no manifest is embedded.

## Round 19: day-to-day use

Worked from scripted sessions of real use (`MCNB_SELFTEST` / `MCDOCS_SELFTEST`, both themes, 100 % and 150 %), every
fix with a key script, unit test or self-test that would catch it again.

- **Typing.** Return in code keeps the line's indentation, one level more after an opening bracket; between a
  bracket and its closer (`f(|)`, also right after an automatic pair) the closer goes onto its own line (U19.1:
  inside strings and comments, with a selection and in prose Return stays a plain line break). Backspace in a code
  line's leading spaces goes back to the previous indentation stop. The usage hint is not shown while the caret is in
  a comment or a string (completion already was not). An Alt chord drops only its own character from the text that
  follows (where the system sends no text for Alt+1, the first letter typed after it was lost).
- **Long cells** (U19.2). A cell of 2,000 bytes or more keeps its galley across frames, keyed by revision, width,
  zoom, theme, fonts, the matched brackets at the caret and the kernel's names; a probe galley laid out through
  egui's cache every frame shows when egui cleared its cache (atlas rebuilt, fonts or scale changed), and then every
  kept galley goes (the reason galleys were not kept before). Measured (release, self-test `type` probe, 1000x760):
  typing in the title above a 2,000-line Input cell 16.3 ms -> 0.34 ms median a frame; typing at the end of the
  2,000-line cell 14.8 ms median (recolouring and laying out 100 KB per keystroke; a cell split into line galleys
  would be the next step), 300 lines 2.7 ms; typing at the top of a 1,000-cell notebook 0.6 ms.
- **Typeset math.** n-th roots (`Ctrl+5`/`Ctrl+%`, see mc-boxes NOTES, U19.3); Down/Up from an insertion point
  enter a cell with boxes at its start/end (Down measured the canonical text and landed in an exponent). A
  systematic matrix (mc-boxes `tests/matrix.rs`, 10 constructs x 10 operations, 90 -> 100 of 100), random edit
  sequences through the model with exact undo and redo (`mc-nbmodel/tests/typeset_undo.rs`: 300 sequences, 9,000
  commands, every change undone and redone to the byte), `keys/roots.keys` (15 scripts) and a window self-test
  (`tests/typeset_selftest.rs`, ignored: clicks into a denominator, a radicand and an index, double-click, a drag
  across boxes, copy and paste between cells, undo/redo, other programs' text as text and as math, Backspace and
  undo; screenshots in both themes and at 150 %).
- **Paste.** Code pasted at an insertion point becomes one Input cell per block between blank lines outside
  brackets and strings (U19.4: copied cells give their texts joined by blank lines, so cells copied to another
  program and back come back as the same cells; prose stays one cell; one undo step). Other programs' CR LF and
  lone CR are line breaks, leading tabs in code become the indentation Tab types. **Paste as math**
  (`Ctrl+Shift+V`, Edit menu, U19.5): the clipboard's text read with `typeset_input`, so `x^2/sqrt(y) +
  surd(a, 3)` from anywhere pastes as the boxes it stands for; text that does not read pastes as text. Plain
  `Ctrl+V` of other programs' text stays text (the round 13 decision is kept).
- **Errors.** In an untitled notebook the kernel names a cell `<cell ID>` and counts lines from the cell's line in
  the notebook text (`<cell c5>:18:2` for the fourth line of a cell); the model writes the running cell's own
  positions as `line 4, column 2` (`eval::relative_positions`). Saved notebooks keep the file's name and lines.
- **Outputs for other windows**: `outputs::OutputPainter` (public) lays out and paints printed values as Output cells
  are (math_text, the cell font, the notebook's painter); the documentation window draws its page values with it.
- Self-test steps added: `type <label> <text>` (one character per frame, frame times logged), `longcell <n>`,
  `setclip <text>`. Scripts: `tests/selftest/typeset.txt`.

Decisions: U19.1 indentation on Return copies the line's leading spaces and tabs and adds `INDENT` after an opener
only (not by bracket depth: a line that continues a call keeps its own indentation). U19.2 only long cells keep
galleys (short ones are cheap through egui's cache; keeping all would hold every visible cell's mesh). U19.4 blank
lines split pasted code only at insertion points; pasted into a cell, text goes in as it is. U19.5 Ctrl+Shift+V,
not a changed Ctrl+V: text from elsewhere stays what it is unless asked.

Gaps: typing inside a 2,000-line cell is a frame's worth of work per keystroke (incremental colouring and line
galleys not done); the documentation window's typeset values are not selectable (their printed text is the tooltip;
example inputs have copy buttons); subscripts are not a cell construct (NBUI.5: `x_1` has no meaning in the
language).

## Round 20: long cells, F1 to a section, warnings

- **Typing in a long cell** (gap of round 19). A cell of 20,000 bytes or more is not coloured again on every
  keystroke: its last colouring is moved over the edit (`edit_between`, a common prefix and suffix;
  `mc_syntax::Colouring::shifted`) and the cell is coloured again 350 ms after typing pauses (the canvas asks
  for that frame; a colouring generation in the kept galley's key makes the galley follow). Colouring and the
  new warnings come from one parse (`mc_syntax::colour_lint`). Measured (release, self-test `longcell 2000` then
  `type`, 1000x760): typing at the end of the 2,000-line cell 16.7 ms median a frame (with the warnings pass
  added) -> 6.7 ms; 300 lines 2.8 -> 3.3 ms (under the threshold: coloured fully, with the warnings). What is
  left is building the 92 KB layout job and egui joining its 2,000 cached paragraph galleys: line galleys would
  be the next step.
- **F1 opens the section** (`help_target`): in a named argument of a documented function the place is
  `function#option` (on the option's name always; in its value unless the caret is on a documented name there),
  and the documentation window opens the example that gives the option (else the Details item naming it, else
  the Options group), scrolled into view and marked. Unit test `f1_in_a_named_argument_opens_the_option`.
- **Warnings** (user request: `integrate(sqrt(x(x - a)), x)` typed as typeset input came back unevaluated, since
  `x(x - a)` is a call of `x`): a code cell's colouring carries `mc_syntax::lint_calls` warnings (a name has a
  definition when the kernel lists it or the help index documents it). In plain text a wavy line in the warning
  colour under the called name; under boxes the name in the warning colour (no squiggle under boxes yet). The
  pointer over it shows the message ("x is called as a function; did you mean x*(x - a)?"); the context menu
  opened on it offers "Insert *: x*(x - a)" (one undo step, `SelectText` then `Insert`, so it works in typeset
  cells too). The kernel adds a note under a value that comes back holding such a call (mc-kernel NOTES).
- Tests: `edit_between` and a moved colouring (unit), `f1_in_a_named_argument_opens_the_option`, mc-syntax lint
  tests; the window self-test `tests/warnings_selftest.rs` (ignored: it opens a window; script
  `tests/selftest/warnings.txt`, steps `warnings` and `fixwarning` added): the warning on `x(x - a)`, its fix,
  no warning on a product and real calls, F1 in `plot_range:` asking for `plot#plot_range`.

## Round 32: the math input experience, tested

**Instrument.** Self-test steps `formula`, `expect <field> "<value>"`, `show`, `eval` (Shift+Enter, then wait for
the kernel) and `roundtrip` (select the output, copy, Ctrl+Shift+V into a new cell, evaluate it: the value must print
as the copied output did); fields `boxes`, `linear` (what Copy gives), `canon`, `output`, `messages`. Scripts
(`tests/input_selftest.rs`, ignored, one window each): `input-corpus1.txt` and `input-corpus2.txt` (97 formulas typed
key by key: fractions in exponents, nested and indexed roots, indexed names with powers, Greek and aliases, templates,
primes and derivatives, sums, integrals and limits written linearly, matrices as lists, units, long expressions,
everyday algebra; 55 outputs also copied back, pasted as math and evaluated again, all equal), `input-caret.txt`
(36 checks: Left/Right through each structure, Up/Down, Home/End, Tab and Shift+Tab, selection across boxes, typing over a selection, box keys on a
selection, Backspace/Delete at every slot edge, undo/redo after each kind of edit, linear and boxed text into each
other, typing latency) and `input-shots.txt` (both themes, 100/150/200 %, the Insert menu). Expected values were
checked from the mathematics; the kernel's printed forms (`2^(1/2)`, `(1/6)*pi^2`, `x^-1`) are kept as printed.

**Pass table** (corpus formulas, caret checks): before 90/97 and 26/36; after the mc-boxes and menu fixes 91/97 and
27/36; after R32.1 to R32.5 (mc-nbmodel and mc-syntax, taken over during the round) 98/98 and 37/37 (one corpus
formula, `greek/element`, and one caret check, `caret/home-end-twice`, added; restored selections now expected).
`tests/model_requests.rs` holds the traps as model tests.

**Findings** (screenshot or script line):
- F1 fixed (mc-boxes): a box key on a selection that starts or ends with a space put the space inside the box
  (`a +√( b)`); edge spaces now stay outside. `input-caret.txt` `select/box-key-on-spaced-selection`.
- F2 fixed (mc-boxes): Copy wrote `2(x^2)` for a number before a power; now `2x^2` (juxtaposition binds like `*`,
  same terms). `input-corpus2.txt` `algebra/coefficient`.
- F3 fixed (discoverability): Insert menu items Fraction, Power, Square root, Root with index, Leave the box (with
  their keys) and submenus Greek letter and Symbol (each with its `Esc` alias as the key hint, `aliases::alias_of`).
  Screenshots `s8-insert-menu`, `s9-greek-menu`.
- R32.1 fixed (mc-nbmodel `boxed.rs::sync_boxed`): undo or redo across a cell's first box or its last box (the
  cell is plain text in between) does not restore the box caret: `same_cell` is false because no editor existed, so
  the caret falls to the text's end, after the box; typing then goes outside (`(u^□)*2`). Fix: remember that the text
  changed by Undo/Redo (`undo_caret`) and restore the remembered caret then; the remembered caret now keeps the
  selection's anchor too, so undo after typing over a selection gives the selection back.
  `input-caret.txt` `delete/*`, `undo/structure-key`, `select/type-over`.
- R32.2 fixed (mc-nbmodel `edit.rs`: Alias keeps `closers` and shifts them): an alias finished in a plain cell between automatically closed brackets
  does not shift `closers`, so the `)` typed next is inserted: `sin(` Esc th Esc `)` gives `sin(θ))`, a syntax
  error. `input-corpus1.txt` `greek/pythagoras`, `greek/numeric-pi`; `input-corpus2.txt` `matrix/rotation`.
- R32.3 fixed (mc-nbmodel `keys.rs`, Gap bindings added): `Ctrl+5`/`Ctrl+%` have no Gap binding, so at an insertion point they switch
  to tab 5 and the typed index starts a plain cell. `input-corpus1.txt` `roots/cube27`, `roots/fourth`.
- R32.4 fixed (mc-nbmodel `boxed.rs`; a second press now reaches the line edges, `caret/home-end-twice`): Home/End inside a slot stop at the slot's edges, and pressing them again does not
  reach the line's edges (design 3.1 says visual line). `input-caret.txt` `caret/home-end`.
- R32.5 fixed (mc-syntax: `x ∈ s` reads as `element(x, s)`, `°` after an operand as `*(pi/180)`, `greek/element`): `∈` is listed as readable in design 3.9 but does not parse (left out of the Symbol
  menu); the alias `deg` inserts `°`, which the lexer rejects with a clear message (`greek/degree`).
- Not bugs: `x_i` is a name (no subscripts, NBUI.5); postfix `!` and the `.` operator are not MathCAS syntax
  (`factorial`, `dot`); assignments show no output (design 5.2); `-` before a fraction box lowers to `-(a/b)`, the
  tree as built.
- Visual check (all shots, both themes, 100/150/200 %): baselines, fraction bars, radical fit (nested roots share
  the right edge correctly), placeholders, caret height inside slots, selection highlight and clipping looked right.
  Left as designed: a fraction inside an exponent is drawn at script-script size (50 %, TeX's rule, decision NBB 7),
  about 6 px digits at 100 % (`s1-light-100`, `x^(3/2)`).
- Latency (release, `type` probe, 1000x700; before R32): typing at the end of a two-line typeset cell of 40 boxes 7.3 ms median a
  frame, at its start 7.7 ms; a 300-line plain cell 1.9 ms.

## Round 33: brackets, bases and the weight of code

User report: a power of a selection `2m` looked like `2m²`; while editing, brackets and the caret went wrong.
- **Bases** (mc-boxes `edit::compound_base`, `layout/engine.rs::fenced`): a base that is not one operand is drawn in
  parentheses. `input-brackets.txt` `base/*`.
- **Unmatched brackets in slots** (mc-boxes `scan::escape_slot`, design 4.4): the canonical text of a slot with a
  loose bracket did not read back, so the view (which reads the cell's text) drew another tree than the editor had:
  the bracket outside its fraction, the caret elsewhere. Loose characters are now escaped (U+2060); the kernel gets
  `mc_boxes::source_text`. The view keeps the unmatched-bracket colour for them. `input-brackets.txt` `slot/*`.
- **Pairs**: Backspace inside an empty pair removes both; an opener typed over a selection in a cell with boxes
  wraps it (as plain cells do); the power key on the inside of a pair takes the pair. `pair/*`.
- **Fonts**: Input and Code cells are drawn in the regular weight (the design's semibold does not exist in the code
  fonts; bold made exponents and syntax colours heavy). The caret in small slots (exponents, indices) is 1 px.

## Round 34: one editor, one painter for code cells (design 11)

- Every Input and Code cell is drawn by the box painter (`Shown::Boxes`, `mc_nbmodel::drawn_as_tree`) from the tree
  the model holds (`Cell::content`); the galley path is left to prose, outputs as text and cells showing their source.
  The window reads the editor through `mc_nbmodel::EditorRef` (`input_editor`, `output_caret`, `any_editor`).
- Caret (`View::paint_box_caret`): the layout's stop, a text line at the size of the row it is in (design 11, 3.1);
  2 px, 1 px when the stop is under 0.8 of the cell's text size; drawn right of the boundary when an item is on its
  left (left of it at a row's start), so it never covers a glyph. Selections: `Layout::selection_boxes`, the union of
  the selected items' boxes per visual line (the painter's `tighten` and `selection_rects` are gone).
- Code background and warning squiggles are drawn on the box path (`paint_box_warnings` maps each warning's bytes
  to items through the text map).
- `caret_rect` for a byte of a code cell maps the byte through the text map (the hover and the self-test's
  `screen_point_of` ask for other bytes than the caret's).
- Long cells: `typeset::lay_out_lines` lays out a code cell line by line with a `LineCache` per cell (an edit lays
  out the lines it changed); the painter puts the characters of one colour into one text section (it made one
  section per character).
- Self-test field and step `boxes` print `none` for a code cell without boxes (the old box editor did not exist
  then), so the scripts keep their meaning.
- Visual check (round 34, `input-shots` and `input-caret`, both themes, 100/150/200 %): the caret beside fractions
  and roots is one text line; in exponents and fraction slots it is the small text line, 1 px, left of the first
  glyph at a row's start; selections hug the selected items; wrapped lines and tall brackets as before.
- Latency (release, `input-caret` probes, median a frame, the two editors run back to back twice on a quiet
  machine, round 34): typing at the end of the 40-box cell 7.49 and 7.40 ms (old editor 7.83 and 7.58), at its start
  7.31 and 7.41 (7.62 and 7.59); a 300-line cell without boxes 2.61 and 2.26 ms (old editor's galley path 2.14 and
  2.04; its p90 and maximum are worse, up to 9.9 ms, the new path's maximum 3.1 ms). On a loaded machine the same
  pairs were 12.4 against 12.2 and 4.36 against 3.65. The first measurement of this round (1252 ms, 60.6 ms,
  4.37 ms) ran while another build used the machine and is not comparable.

## Round 35: `.svgz` pictures

- A picture file ending in `.svgz` (gzip-compressed SVG, how the documentation stores its plots) is an SVG drawing:
  `read_drawing` decompresses it (flate2; `usvg` is built without its `svgz` feature), then it is parsed and rasterised
  as an `.svg` (theme, device pixels, same cache key). Its natural size comes from the decompressed root element
  (`natural_size`), since `picture_size` reads the raw head. `SvgRaster` takes bytes. Gap: `mc-nbfile`'s save-time
  embedding does not know `.svgz` (the picture stays a file reference).
## Round 35: the faint multiplication sign

- Where white space between two operands is a product (`Colouring::products`, from the parser), a code cell draws a
  faint `×` (`View::paint_products`): two strokes in the theme's faint text colour, centred in the run of spaces
  at the row's axis height (0.27 of the text line above the baseline), half size 0.11 of the line (at most 0.3 of
  the gap); for a gap that starts with a line end or a comment (a product across lines inside brackets), just after
  the last character of the first line. It is drawn over the layout, not an item: never selected, copied or a caret
  stop. Placed through the canonical text's `TextMap`, so cells with boxes get it too (`product_mark`); cached per
  cell by layout key and gaps (`product_marks`); a cell without gaps costs nothing.
- Warnings: `colour_lint` gets a `function` predicate (the help index's functions and the kernel's names of kind
  `function`, `View::set_functions`), so `sin x` is flagged with the fix `sin(x)`; quick fixes are replacement lists
  applied as one selection and one insertion (one undo step; an empty replacement deletes); the menu and tooltip
  show `Lint::fix_label`.
- `Esc * Esc` was already in the alias table (`*` → `×`) and Insert > Symbol lists `×` with it; the lexer reads `×`
  as `*`.
- Self-test `tests/selftest/input-space.txt` (`input_selftest.rs` `space`): 13 formulas typed with spaces (`m c^2`,
  `2 pi r`, `a b + c d`, `(x + 1) (x - 1)`, `[a b, c]`, a product across a line in a list, beside boxes, `2x` with
  none, `f (x)`, `Esc * Esc`, the `sin x` warning and its fix), each with `canon`/`linear`/`gaps`/`output`; new step
  `products` and field `gaps`; screenshots `space-{light,dark}-{100,150,200}` checked by eye.
- Typing probes (`input-caret`, loaded machine, product marks on/off alternating): plain-300 median 5.1 / 3.5 ms on,
  3.9 / 3.8 ms off: within the noise.

## Round 37: long cells, prose in exports

- Typing in a long cell: `View::box_layout` lets go of the cell's last layout before asking the line cache, which
  then changes it in place (`mc_boxes::layout::layout_by_lines_shared`); `typeset::keep_baseline_shared` copies a
  layout only when a script reaches into the margins. The colour table of every character (`box_colours`, 2.4 ms a
  keystroke at 2,000 lines) is gone: `typeset::Ink::colours` is a function of (row, item) that `InputColours` answers
  from the colouring and the text map for the characters drawn. The text map is kept by revision (`text_map`) and
  shared by colours, warnings and the faint multiplication signs. A profile of one keystroke at 2,000 lines before:
  key handling 0.6 ms, composition 3.5 ms, colour table 2.4 ms, painting 0.2 ms.
- Latency (release, `type` probe, 1000 x 760, two runs each, shared machine): 2,000-line cell, typing at the end:
  10.96 / 11.48 ms median a frame before, 2.79 to 3.95 ms after (p90 4.4 ms); typing at the top 2.3 to 2.6 ms; Enter
  at the end 1.4 to 2.2 ms; 300-line cell 4.4 ms before, 2.2 ms after. Every window self-test passes
  (`-- --ignored --test-threads 1`: input corpus 1 and 2, caret, brackets, space, shots, clipboard, help, typeset,
  warnings, zoom, manipulate, rotate3d).
- Prose (planned item: hard-wrapped paragraphs in the PDF): the window keeps a text cell's line breaks (its galley is
  laid out from the text with its `\n`, wrapped only where a line is longer than the cell). Screenshots of
  `examples/notebook.mcn` at 1000 and 620 px: at 1000 px the four source lines show as four lines; at 620 px each
  long line wraps and the next source line starts a new line, mid-sentence, as the PDF does at 11.5 pt. The PDF
  export already does the same (`wrap_prose` keeps `\n`), so it is unchanged; whether paragraphs should reflow is
  the user's open question, for the window and the exports together.

## Round 38: the input editor tested as a user types

- `tests/selftest/input-user.txt` (test `input_selftest::user`): 72 formulas typed through the keyboard (fractions,
  powers, roots and brackets mixed, selections, typing over and wrapping them, undo and redo, cut, copy, paste as text
  and as math, evaluation of 11), each checking its box notation (`expect boxes`) and its stored text (`expect
  canon`), and with the new step `same` that the two agree: the tree the editor holds and the view lays out reads
  back from the stored text to the same boxes and linear text (75 checks, all agree).
- Found and fixed: (1) an automatic closer was forgotten on any caret motion, so `(`, a fraction filled with Down,
  then `)` gave `))` (also `[1, a/b, 3]]`); a motion now keeps the closers the caret stays inside of (`mc-edit`,
  test `closers_survive_motions_inside_their_pair`). (2) Automatic completion took `2a` (a coefficient times `a`) as
  a name and opened its popup (fuzzy match `hypergeometric_2f1_regularized`); a Down typed then went to the popup,
  not the formula. The name before the caret now starts at its first letter or `_` (`number_before_name`, unit
  test).
- New self-test steps: `select <cell> <from> <to>` (a byte selection in a cell, as a click would set it) and
  `same`. `tests/selftest/long-cell.txt` (test `long_cell`): the typing latency probes of a 2,000-line cell
  (mc-boxes NOTES, round 38).

## The layout for phones and tablets (Android)

`src/app/mobile.rs`, chosen by `layout::mobile()` (Android; `MCNB_LAYOUT=mobile` anywhere, for trying it and for
`tests/mobile_selftest.rs`). `App::ui` branches once: the desktop panels (menus, toolbar, tabs, status bar) or
`mobile_ui`. Model, kernel, canvas, dialogs and commands are shared; the mobile shell adds none.

- **Screens**: Home (New calculation, the notebooks folder newest first with a per-file sheet, Open a file) and
  Notebook (bar: back, name = rename, kernel dot or stop, more). One sheet (`Sheet::More` / `Sheet::File`).
- **Bottom of the notebook follows the selection** (`mobile_selection`): a caret in an editable cell → the key
  row (KEYS, caret left/right, tab, Run) with the completions in a strip above it (`completion_popup` fills
  `Mobile::completions` instead of drawing its list); a selected result or picture → Copy, LaTeX, Share, Delete;
  else the round "+".
- **Canvas**: `view::Metrics` (brackets, side margin, right pad, insertion line and "+", handle size and reach):
  `desktop()` is the design's numbers, `mobile()` has no brackets, 22/10 pt margins, no insertion line, 12 pt
  handles. The content column is at most 720 pt wide, in the middle of a wider screen.
- **One notebook at a time**: leaving it (`leave_notebook`) writes it and replaces it by an empty one (its kernel
  ends). No save question: `autosave` writes two seconds after a change; a notebook without a file becomes
  `Calculation N.mcn` in the notebooks folder once a cell has text.
- **Look: Material Design 3.** `theme::Material` holds the colour roles (a blue scheme, light and dark);
  `Palette::material()` maps them onto the palette the canvas and egui draw with (surfaces, text, outlines, the
  accent; code and mathematics colours stay). `mobile.rs` draws the components itself: a 64 pt top app bar on the
  page colour, icon buttons with round state layers, two-line list rows with a leading circle, pill buttons
  (filled for Run, tonal for Share and Open a file, text for the rest), outlined chips for completions, a floating
  action button (extended on Home), a modal bottom sheet with a handle that reaches under the navigation bar, and
  Material dialogs (`dialog.rs`: 28 pt corners, text buttons). Input cards have 12 pt corners
  (`Metrics::card_radius`). The interface font on Android is the system Roboto (`mc-ui` `fonts.rs`). The system
  theme and the colour of the status bar icons go through the activity (`platform::system_dark`, `light_bars`).
- **Sizes**: `theme.rs` has a mobile branch (44 pt targets, 16 pt text); bottom bars are 56 pt; dialogs take the screen's
  width when it is narrower than they are (`dialog::show`).
- The on-screen keyboard is asked for only by a caret in an editable cell (`canvas.rs` `ime`).
- Self-test steps `calc`, `mopen`, `mhome`, `mstate` drive the screens without coordinates.

Left out on purpose (the user's brief: minimal; open a notebook, a quick calculation, share a plot): cell styles
and grouping, find, preferences, tabs, the status bar, the documentation window. Known gaps: no paste key; the
per-cell running mark went with the brackets (the bar's kernel dot shows work); a long press on a list row or a cell
does nothing (the dots and the sheet do it).
