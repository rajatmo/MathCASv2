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

15 unit tests and 124 key scripts in `tests/keys/`: `typing.keys` (41), `eval.keys` (22: input 2, outputs 2 and 3, input 4
numbering, `;`, re-evaluation, print/message, read errors, failures, abort, queue rules, old kernel, kernel death,
quit, initialization cells, edit-duplicates, delete all output, undo vs outputs), `groups.keys` (20: every 2.3
grouping vector and 2.4 open/close vector, manual groups), `cells.keys` (25), `motion.keys` (16).

## Known gaps

Ctrl+click on brackets works in the model and scripts but is not wired to the mouse in the window; typesetting
commands (M6) are bound to nothing.

## Graphics and large outputs (milestone M4)

- `KernelMsg::Display` (a picture's `image("path")` text becomes an Output cell); `Cell::graphic()` and
  `Cell::graphic_size()`; `Command::ResizeGraphic(id, Option<(w, h)>)` sets or removes the output's `size=` (undo
  unit); the sizes of a cell's old pictures pass to its new ones in order when it is evaluated again (design 6
  guess); `Notebook::rename_values` after a save moved pictures. Key-script action `Resize(i,w,h)` / `Resize(i)`;
  `graphics.keys` (7 scripts). The fake kernel answers `image("...")` with a display.
- `printed`: `wrap(text, cols)` (penalty line breaking of linear output, design 4.8), `wrapped` (same-length text with
  line ends), `sample(text, budget)` (large-output samples with `«n»`, design 4.9), `is_large`, `LARGE_OUTPUT`
  (100,000 characters) and `SAMPLE` (2,000).

## Round 10

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

## Round 11

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
  output's `form=` is dropped. Outputs take the read-only box caret over `mc_typeset::math_text`.
- Tests: key scripts (typeset.keys: 9 removed with their features; new: linear text stays linear, Ctrl+2 on a
  selection, slots by arrows, Backspace unwraps a fraction and a root, selection across a box with copy and typing
  over it, a box key at an insertion point), unit (older notebooks round-trip, output forms dropped).

## Round 21

- `tests/bugs_round21.rs`: six ignored failing tests: B21-8 (a typeset cell given a prose style is written
  `typeset`, which the reader drops), B21-9 (undo of Divide and of a style change loses `label=` / `n=`), B21-11
  (a Move onto its own place is an undo step), B21-12 (trailing blank lines are not saved).
- Method that found them: a seeded command fuzz with exact comparison (save/reopen identity, undo-all and redo-all,
  every undone text a recorded state) and a delta-debugging shrinker; compare cells' raw text as well as the saved
  text, since the saved text hides trailing line breaks (comparing only `to_text` makes Undo of Enter look broken).

## Round 22 (bug fixing)

- B21-8: a style change of a typeset cell to a prose style turns its boxes into linear text and drops the mark
  (`set_style`; boxes exist in code cells only, so file and window agree).
- B21-9: undo and redo of a cell replacement restore the label when the operation itself changed it (Divide, a style
  change to a style without labels); otherwise the label stays the evaluation's, never restored.
- B21-11: a Move onto the cell's own place (`to == lo` included) is not an undo step.
- B21-12: line breaks ending a cell without output are written as blank lines after it (gap = 1 + breaks) and read
  back (`gap - 1` extra breaks); a source ending in a line of white space keeps it (mc-nbfile reads only empty
  lines as the gap of a code cell; `Cell::exact` keeps every line when an output follows).
- The unreadable-canonical-text variant of B21-8 does not reproduce through the model (test added, passes).

## Round 32: input traps found by the math input self-tests

- R32.1: undo and redo across a cell's first or last box (the cell plain text on one side) restore the box caret:
  `Notebook::undo_caret` is set by Undo/Redo and lets `sync_boxed` take the remembered caret although no editor was
  open. The remembered caret (`boxed::BoxCaret`) keeps the selection's anchor, so undo after typing over a
  selection gives the selection back.
- R32.2: `Alias` keeps the automatic closers and an alias replaced in plain text shifts them (`shift_closers`), so
  `sin(` Esc th Esc `)` steps over the closer instead of doubling it (in plain and boxed cells).
- R32.3: `Ctrl+5` / `Ctrl+%` at an insertion point make an Input cell holding an n-th root.
- R32.4: Home/End in a slot go to the slot's edge; pressed again there, to the line's edge (design 3.1), with
  Shift extending.
- Tests: `mc-notebook/tests/model_requests.rs` (7) and the window self-tests `mc-notebook/tests/input_selftest.rs`.

## Round 34: code cells are trees (design 11)

- Every Input and Code cell holds its content as a box tree, `Cell::content`, kept in step with `text` by the
  notebook's primitives (`with_cells`, `insert_cells`, `set_text`, `update_cell` call `Cell::fix_content`); the
  editor's changes go through `Notebook::set_tree`, which sets the tree, derives the canonical text and the
  `typeset` mark (`has_boxes`), and records `Op::Edit` (the `mc_edit::Transaction`, the text and mark before and
  after). Notebook-level undo stays one history: an edit is one more kind of operation in it, so cell operations and
  edits interleave in their order (design 11, 3.3, says "a shared sequence number" between per-cell histories; one
  history holding both is the same order with less machinery). Typing merges into the last unit as before
  (`merge_text` merges the transactions).
- `code.rs` routes the editing commands of a code cell with the caret to `mc_edit::Editor::apply` (one editor for
  cells with and without boxes). `boxed.rs`, the code-cell paths of `edit.rs` and the `MC_EDITOR` switch are deleted;
  `edit.rs` keeps the prose editor (text cells, a cell's source view). The output caret (`outbox.rs`) uses
  `mc_edit::Editor` too.
- The notebook's selection of a code cell stays a byte range, now the exact canonical-text bytes of the tree's
  selection (the box editor of rounds 9 to 33 parked it at the text's end); a byte selection set elsewhere (Find, a
  click on text, a cell entered from an insertion point) is read into the tree through `TextMap`. Undo and redo put
  back the transaction's own selection (`tree_sel`), not a caret remembered by text (`box_carets` is not used).
- Up and Down from an insertion point into a code cell land on its first or last visual line at the goal x (the
  layout's, through `Env::layout`; key scripts use the line breaks). A cell with boxes was entered at its start or
  end before (E34.11 in `mc-edit/NOTES.md`).
- Divide in a cell with boxes splits between top-level items only (beeps with a caret inside a box); each piece
  keeps the `typeset` mark if it still holds a box.
- Tests: `tests/parity.rs` compares the 174 key scripts without boxes or aliases with the old editor's results,
  recorded before its deletion in `tests/parity/old-editor.txt`; `tests/docs_roundtrip.rs` reads and writes every
  notebook under `docs/`. Key scripts whose expected text put the caret at the end of a
  typeset cell now show the exact caret (7 tests in `clipboard`, `roots`, `typeset`); the unknown alias of a code cell
  is the editor's mark, no `⋮` character in the text (`typing`).
