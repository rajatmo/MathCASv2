# mc-nbmodel: the notebook document model (notebook round, P3)

No UI and no kernel. Specification: `docs/design/08-notebook.md` sections 2, 3 and 5.

- **Styles** (2.1): all 24 with grouping rules, margins, fonts and Return behaviour.
- **Grouping** (2.3): one O(n) stack pass, plus groups made by hand and the manual mode; bracket columns with the
  outermost group rightmost. **Open/closed groups** (2.4): `shown=K`, hidden-cell rules; a cell typed after a closed
  group's head opens the group.
- **Selection**: caret or text range, cells, or insertion point; every change goes through a few primitives that keep
  selection, index and groups consistent.
- **Commands and key table**: one table (chord, context, command) used by the window, the key scripts and the
  keyboard help; reserved and typesetting keys bound to "do nothing" until M6.
- **Undo** (3.5): op-based, keyed by cell id; typing runs merge (a run ends after a 1 s pause, a word boundary after
  white space, or any other command); generated cells and labels are never restored.
- **Evaluation queue** (5.3-5.7) behind a `KernelLink` trait: one `eval` at a time; old generated cells deleted when an
  evaluation starts; responses stream in as Print/Output/Message cells; labels from the kernel's `n`/`first`/`next`
  (a client-side counter for old kernels); abort clears the queue; remove from queue; quit; a dead kernel leaves a
  Message cell; initialization-cells question; read errors mark the bracket.
- **Cell operations**: divide, merge, input/output from above, same style below, delete all output, show expression
  (round-trips through the file grammar), cell properties, group/ungroup, open/close all, move/copy cells, cell
  clipboard with the private flavour, edit-duplicates for Output cells.
- **M3 commands**: Esc aliases (`mc_boxes::aliases`; templates stay marked until M6), Alt+/ comment toggle, Ctrl+.
  (`mc_syntax::extend_selection`), Ctrl+Shift+B check balance.
- **Key-script runner** (`script`): notation `▮` caret, `⟦ ⟧` text selection, `‖` insertion point, ` ◆` selected
  cell, `--- groups` expected group notation; `FakeEnv` (fixed-width layout), `FakeKernel` (numbered, or the old
  protocol with `--- kernel old`).

## Decisions

1. Undo is op-based and keyed by cell id, so evaluation can delete and insert generated cells without breaking it.
2. Reading `#!`: a numbered block, or a failure/"aborted" text, gives an Output failure cell; otherwise a Message
   error. `#?` gives a Message warning.
3. A numbered Output closes a block; Print and Message lines go into the block of the value that follows.
4. Generated cells that follow no code cell cannot be written and are dropped on save.
5. Tab inserts 4 spaces; Shift+Tab removes up to 4.
6. A cell left in show-expression view is saved as the cell its source describes, else as a Program cell.
7. Old kernels (no numbering) get client-side numbers, one per evaluation.
8. Return in an Item splits the cell at the caret into a new Item.
9. Esc with a text selection does nothing; unknown aliases stay marked (design guess 11).
10. Shift+Enter on a cell selection keeps the cell selection.
11. Design section 13 guesses as specified, each in one place: label kept after editing, duplicate placed after the
    output group, typing ignored with cells selected, paste replaces selected cells, Ctrl+click toggles, gaps are
    Up/Down stops, abort clears the queue, divide with a selection makes up to three cells, Ctrl+A escalates.

## Tests

15 unit tests and 124 key scripts in `tests/keys/`: `typing.keys` (41), `eval.keys` (22: In[2]/Out[2]/Out[3]/In[4]
numbering, `;`, re-evaluation, Print/Message, read errors, failures, abort, queue rules, old kernel, kernel death,
quit, initialization cells, edit-duplicates, delete all output, undo vs outputs), `groups.keys` (20: every 2.3
grouping vector and 2.4 open/close vector, manual groups), `cells.keys` (25), `motion.keys` (16).

## Known gaps

Ctrl+click on brackets works in the model and scripts but is not wired to the mouse in the window; typesetting
commands (M6) are bound to nothing.

## Graphics and large outputs (milestone M4, package nb-m4)

- `KernelMsg::Display` (a picture's `image("path")` text becomes an Output cell); `Cell::graphic()` and
  `Cell::graphic_size()`; `Command::ResizeGraphic(id, Option<(w, h)>)` sets or removes the output's `size=` (undo
  unit); the sizes of a cell's old pictures pass to its new ones in order when it is evaluated again (design 6
  guess); `Notebook::rename_values` after a save moved pictures. Key-script action `Resize(i,w,h)` / `Resize(i)`;
  `graphics.keys` (7 scripts). The fake kernel answers `image("...")` with a display.
- `printed`: `wrap(text, cols)` (penalty line breaking of linear output, design 4.8), `wrapped` (same-length text with
  line ends), `sample(text, budget)` (large-output samples with `«n»`, design 4.9), `is_large`, `LARGE_OUTPUT`
  (100,000 characters) and `SAMPLE` (2,000).

## Round 10 (package nb-m5)

- **Automatic closing** (NB5.1, `auto_close`, off unless the window turns it on; key-script step `AutoClose`):
  `( [ {` pair (not in strings or comments, not before a letter, digit or `_`), `"` pairs in code cells before white
  space, a closer, `,`, `;` or the end; typed closers step over automatic ones; Backspace between an untouched pair
  deletes both; an opener with a selection wraps it. 20 scripts in `tests/keys/autoclose.keys`.
- **Two-dimensional input** (`boxed.rs`, NB5.3): a typeset Input cell is edited through `mc_boxes::Editor` while it
  has the caret; each edit writes the canonical text back (undo, save, evaluation see text). Structure keys and
  template aliases make a plain Input cell typeset; `Ctrl+Shift+N`/`Ctrl+Shift+I` convert. Commands `Structure`,
  `ExitStructure`, `SwitchScript`, `ToTypeset`, `ToLinear`, `ClickBox`; `Env::box_line`; script section `--- boxes`.
  24 scripts in `tests/keys/typeset.keys`.
- **Output forms** (NB5.5): `OutputForm`, `Cell::output_form()` (output block attribute `form=`), command
  `OutputForm(f)`; `Env::atomic` makes character motions jump across a typeset output and double/triple clicks
  select it whole.

## Round 11 (package nb-m6)

- **Caret in typeset output** (`outbox.rs`, NB6.4): an Output cell shown typeset (`Env::atomic`) gets a read-only
  `mc_boxes::Editor` over `typeset_output_with(text, output_options())` while it has the caret. The model's
  selection stays bytes of the printed text: the box caret or selection is written as the bytes its items stand for
  (`SourceMap::span`, `byte_at`), and a byte selection made elsewhere (Find, a click before the output) moves the
  box caret onto the items it covers (`caret_at`, `find`). Handled: arrows (structural Up/Down inside, leaving at
  the ends), Shift (anchor kept), Home/End, `Ctrl+.`, Select all, Copy (the printed text of the part when it is one
  stretch, else its linear text with the output spellings turned back), `ClickBox` on outputs.
  `Notebook::output_editor`, `set_matrices`, `output_options`.
- **Typeset input**: undo and redo put the box caret back (NB6.5: each edit records the text and caret before and
  after, 256 entries; a rebuilt editor whose text matches takes that caret); Shift+click and drags keep the anchor
  (`Editor::extend_to`); a byte selection set in the text (`SelectText`, `ClickText`, a restored session) selects the
  items it covers through `TextMap::find`/`caret_at`; `box_word` (the name before the caret, for completion).
- **Commands**: `SelectText(id, anchor, pos)`, `CopyLatex` (the selected part of a typeset cell or output, the
  selected text or whole content of the caret's cell typeset, or the selected Input/Output cells one per line),
  `ReplaceWord(name)` (completion in plain and typeset cells), `AppAction::Preferences`.
- **Robustness**: the text helpers (`text.rs`) move any position back onto a character boundary first (a fuzz case
  reached `line_start` with a position inside `□`); `ShowExpression` on a source that parses to no cell returns
  `false` instead of panicking. `tests/fuzz.rs`: random command sequences (typing, all motions with and without
  Shift, clicks and box clicks at random places, byte selections, clipboard, undo/redo, structure keys, conversions,
  cell operations, styles, completions, outputs typeset) on five notebooks, checking after every command that the
  selection is valid and that the result writes and reads (400 sequences of 80 commands by default,
  `MC_FUZZ_CASES`; 20,000 ran clean once); damaged notebook texts read without panics.
- Tests: 26 unit (new: the caret walks through a typeset output; a completion replaces the name; Find in typeset
  input), key scripts (6 new in `typeset.keys`: undo/redo caret, undo in a denominator, Shift across a box and
  back, Shift after a box, repeated Ctrl+, / Ctrl+Enter), tab scripts, 2 fuzz tests.

## Round nb-ui

- Shift+Up / Shift+Down in typeset input keep the selection's anchor (structural move, else `Env::box_line`, else the
  cell's start or end); in typeset output they extend inside the output (structural, else its start or end).
- Home / End in typeset input and output: the visual line (`Env::box_line_edge(cell, offset, end)`, default `None`
  falls back to the editor, which stops at line breaks).
- Tests: 27 unit (new: selections in typeset cells extend by lines and keep their anchor).

## One kind of cell (round nb-ui, NBUI.5)

- No conversion commands (`ToTypeset`, `ToLinear`, `OutputForm`, `SwitchScript` and their keys removed; `OutputForm`
  and `Cell::output_form` gone; `set_matrices` gone). The `typeset` mark is derived: a box key sets it (the cell's
  text is read as canonical text with only the three boxes, `restrict`), and when the last box goes (`commit_box` ->
  `unbox`) the cell is plain again with the caret on the same character. Box keys at an insertion point make an Input
  cell first (key table: `Ctrl+/`, `Ctrl+6`, `Ctrl+^`, `Ctrl+2`, `Ctrl+@` in the Gap context). Copy in a cell with
  boxes gives linear text. Automatic closing in cells with boxes (`auto_pair`: a bracket pair, stepping over a typed
  closer).
- Reading: a typeset cell of an older notebook keeps the three boxes, the rest linear (plain when none remain); an
  output's `form=` is dropped. Outputs take the read-only box caret over `mc_boxes::math_text`.
- Tests: key scripts (typeset.keys: 9 removed with their features; new: linear text stays linear, Ctrl+2 on a
  selection, slots by arrows, Backspace unwraps a fraction and a root, selection across a box with copy and typing
  over it, a box key at an insertion point), unit (older notebooks round-trip, output forms dropped).
