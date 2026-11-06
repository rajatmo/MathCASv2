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

## Graphics and large outputs (milestone M4, package nb-m4)

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

## Typeset output and input, plot coordinates, closing brackets (round 10, package nb-m5)

- **Painter** (`typeset.rs`, NB5.4): box layouts from `mc_boxes` are measured and drawn entirely with the bundled
  math font (letters and digits too; characters it lacks fall back to the cell's text font through egui). Glyphs are
  rasterised at device pixels with four sub-pixel phases into one atlas texture of our own (1024², grows once to
  2048², cleared and refilled when full), uploaded once per glyph; each frame the visible glyphs and rules of a layout
  become one mesh.
- **Typeset output** (M5, NB5.5/NB5.6): Output cells show `mc_boxes::typeset_output(printed)` in display style at
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

## Round 11 (package nb-m6)

- **Brackets on the left, no labels** (user decision, NB6.1): the bracket column moved to the left margin, cell
  brackets nearest the content, enclosing groups further left (at least two group columns reserved so the content does
  not jump when the first group appears), hooks pointing right. The content is left-justified 8 pt (scaled with the
  magnification) right of the innermost bracket; the styles' left margins count from the Input margin (66), so Title,
  Section, Text, Input and Output share one edge and items keep their indents. `In[n]:=` / `Out[n]=` are no longer
  drawn (the numbers stay in the model and file; an output's bracket tooltip and the status bar say `out(n)`). The
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
- **No egui fonts** (coordinator request): `mc-ui`, `mc-notebook` and `mc-docs` declare eframe without
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

- Outputs are always drawn by the cell renderer over `mc_boxes::math_text` (failures, pictures, outputs over the
  typeset limit and multi-line tables stay text); layout options without operator spacing or small inline rationals,
  so the printed characters keep their places. Removed: Format > Output form, the preferences "Typeset output by
  default" and "Lists of equal-length lists as matrices" (`typeset_output`/`matrices` settings and view fields),
  `View::clear_boxes`.

## Help while writing code (round 12, package nb-docs)

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

## Round 13 (nb-mat): clipboard with boxes

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
- nb-mat: the box flavour is restricted to the three boxes when written and again when read (another program's box
  text with a subscript or a grid pastes those as their linear text).
- nb-mat: at an insertion point whose style was set to a non-code style (Text), pasted boxes become their linear
  text in that style: the cell style chosen wins over the boxes.
- nb-mat: pasted boxes get the box editor's implicit `*` where they touch a name (`z` + pasted `x^2` is
  `z*((x)^(2))`), as boxes made by the box keys do.
- nb-mat: a cell with boxes holding pasted text such as `((a)/(b))` keeps it as characters while edited; on reload or
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

## Round 13 (nb-mat): labeled and legended values

`labeled(x, l)` and `legended(x, l)` around a value that is not a picture (the kernel shows the printed call, gap
G80) are drawn by the notebook: `mc_boxes::math_text` reads the call into a `Node::Labeled` box (see mc-boxes NOTES),
the cell renderer lays it out like any output (right-justified as a whole) and paints it.

- The label row is painted in the theme's label colour (`Palette::label`, weak text: mono-2), a legend's swatch
  characters (`━━` for `line_legend`, `■` for `swatch_legend`, `●` for `point_legend`) in their own colours, the
  value in the output's text colour (`View::label_colours`: a per-layout-row colour table as typeset input has,
  cached in `box_colours` by revision, theme and width; outputs without `labeled(`/`legended(` skip it).
- Selection, the box caret, Find and copying go through the source map as for any output: copying the whole output
  gives the printed call, a part of the value the value's part, the label its characters.
- nb-mat: positions follow the reference conventions and the graphics package's layout of the same terms: the label
  below by default, `top`/`above`, `bottom`/`below`, `left`/`before`, `right`/`after` (a symbol or a list of one);
  legends at the right, entries one per line, centred on the value. Label and legend at the size of the output text.
- nb-mat: forms not recognised (other sides such as `center`, labels that are calls or lists, unknown legend heads,
  wrong arity, a call that is not the output itself or a top-level list element, values over 20,000 bytes) stay the
  printed text, typeset as before. Options (named arguments: `frame`, `label_style`, ...) are not drawn.
- Tests: `view::tests::labeled_outputs_draw_their_labels_at_the_right_edge` (right edge, label under the value's
  baseline, label and swatch colours, a click on the value lands in the value slot). Screenshots by the self-test
  (One Light and One Dark at 100 % and 150 %) of a notebook with these outputs, outside the repository.

## Round 13 (nb-mat): calm Ctrl+wheel magnification (user request)

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

## Round 13 (nb-mat): output alignment, a setting (user request)

- View > Output alignment > Left / Right (check marks; the other choice shows `Ctrl+Shift+J`, which switches), also in
  the preferences ("Outputs: on the left / on the right"); saved as `output_alignment` in `notebook.json`. **The default
  is Left** (user: "the left justification of outputs was probably better"). `View::set_outputs_right` makes every
  layout again (galleys, wrapped outputs, large-output samples, box layouts); everything that follows the geometry goes
  through `View::right_justified(&self, c)` (text anchoring `text_dx`, box layouts `align_right`, background typesetting
  jobs, picture rects and their resizing, large outputs, carets, hits, Find, plot read-outs), so both alignments share
  one code path. The switch keeps the content at the canvas's middle in place (the magnification's anchor).
- A View menu (no Alt letter: V is Evaluation's) holds Output alignment and Magnification.
- Test: `outputs_start_at_the_left_edge_by_default_and_switch` (typeset and linear outputs start at the left edge,
  clicks at it hit the start; Right and back re-lays out); the right-edge tests set Right explicitly.

## Round 13 (nb-mat): one file per notebook (user request)

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
