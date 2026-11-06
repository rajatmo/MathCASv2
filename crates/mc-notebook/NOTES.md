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
- Not yet portable: fonts are read from the Windows font folder (Linux/Android fall back to egui's fonts), the
  clipboard's cell format is Windows-only, the storm driver is Windows-only.

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
