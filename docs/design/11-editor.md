# MathCAS v2: the input editor, rewritten (design)

Status: design for round 34. It replaces the editing parts of design 08 sections 3 and 4.2
to 4.6; the box tree, the canonical text (4.4) and the layout engine stay.

## 1. Why

The editor of rounds 9 to 33 works, but it is two editors joined by a text round trip, and users feel the joints.

What is there now:

| Part | Where | What it does |
|---|---|---|
| Plain editing | `mc-nbmodel/src/edit.rs` (1,600 lines) | A code cell without boxes is a string; the caret is a byte offset; painting is an egui galley. |
| Box editing | `mc-boxes/src/edit.rs` (1,250 lines), `mc-nbmodel/src/boxed.rs` (560 lines) | A cell with a box gets a tree editor, attached while the caret is in it. |
| Truth | `Cell::text` | After every command the tree is written to canonical text; the view reads the text back into a second tree and lays that out. |

Defects that follow from this shape (each seen by a user or a self-test):

1. **Two sources of truth.** The view draws a tree read back from text while the caret lives in the editor's tree.
   Whenever the text does not read back to the same tree, the drawing and the caret disagree (round 33: an unmatched
   bracket in a numerator was drawn outside the fraction).
2. **Every command exists twice.** Insert, delete, move, select, Home/End, automatic closers, aliases and wrapping
   are written once for strings and once for trees, and they drift apart (round 33: Backspace in `()` and wrapping a
   selection worked in plain cells only). A cell changes editor when its first box appears and when its last one
   goes, with a caret conversion each way.
3. **Undo is text plus remembered carets.** The undo stack holds texts; carets are looked up in a side table keyed
   by (cell, text) (`box_carets`), with special cases when the cell was plain in between (R32.1).
4. **Caret geometry is the row's.** A caret stop has the extent of its whole row, so beside a fraction the caret was
   as tall as the fraction (patched in the painter in round 33; the layout still has no per-position height).
5. **Two ways to draw a character.** Text-font characters go through egui galleys, everything else through the
   notebook's glyph atlas, each with its own metrics. Small scripts and their carets do not line up exactly.

## 2. Principles

1. **One document model.** The content of every code cell (Input, Code) is a box tree (`Row`). A cell without
   boxes is a row of characters. There is no plain mode and no conversion.
2. **One source of truth while editing.** The tree is the truth. Canonical text is derived: for the file, the
   kernel, the clipboard and Find. The view lays out the tree the editor holds, never a second one.
3. **One editor.** Every command is written once, over the tree. Text behaviour (words, lines, automatic closers,
   indentation, aliases) is the behaviour of a row of characters.
4. **Undo is a history of edits.** Each undo step is a transaction on the tree with the selection before and after.
5. **Geometry belongs to the layout.** The layout gives each caret position its own box (x, baseline, ascent,
   descent of a text line at that position's size) and each item its ink box. The painter decides nothing.
6. **One text pipeline.** One measurement and one drawing path for every character of a code cell.

## 3. Structure

```
mc-boxes        tree, canonical text, layout            (kept; `edit.rs` leaves)
mc-edit  (new)  the editor: selection, commands, transactions, history
mc-nbmodel      notebook of cells; a code cell holds its tree and its history
mc-notebook     view: lays out and paints what the model holds; input mapping
```

### 3.1 Document (`mc-boxes`, unchanged types)

`Row`, `Item`, `Node` as today. Added to the layout's output, per caret position: `Stop { x, baseline, ascent,
descent }` where ascent and descent are those of a text line at the position's size (scale of its row), and per
item its ink rectangle. Selection rectangles and hit tests use the ink rectangles; the caret uses the stop.

### 3.2 Editor (`mc-edit`)

- `Position { path: RowPath, offset }`, `Selection { anchor: Position, head: Position }`. A selection whose ends are
  in different rows covers the smallest run of items of their common row that holds both (as today).
- `Transaction`: a list of primitive changes (`Splice { path, range, items }`) plus the selection before and after.
  Every command builds one transaction; applying its inverse undoes it exactly. Typing merges into the previous
  transaction while it continues a word (the existing coalescing rule).
- `History`: the undo and redo stacks of one cell. No carets remembered by text.
- `Editor::apply(command) -> Option<Transaction>`: a pure function of (tree, selection, command). Commands are data
  (`Command::Insert(text)`, `Delete(dir, unit)`, `Move(motion, extend)`, `Structure(kind)`, `Wrap(open, close)`,
  `Alias(...)`, `Paste(...)`, `Indent`, ...), so key bindings, menus, the self-test and tests drive the same thing.
- Rules that are now written once: automatic closers (a closer is remembered as a mark on the tree position, not a
  byte), pair deletion, wrapping a selection, implicit multiplication next to boxes, the operand a structure key
  takes, placeholders, leaving a box.
- Motions that need geometry (Up, Down, Home and End on a wrapped line, clicks, drags) take a `&Layout` through a
  small trait (`Geometry`), implemented by `mc_boxes::Layout`, so the editor has no painter dependency and tests can
  give a fake.

### 3.3 Model (`mc-nbmodel`)

- `Cell` of a code style holds `content: Arc<Row>` and a lazily derived `text` (canonical), invalidated by revision.
  Text cells (prose) keep strings and the existing prose editor; that split is by kind of content, not by whether a
  box is present.
- The file format does not change: `text` is written, the `typeset` mark stays for cells with boxes, so notebooks
  from earlier versions read and write byte for byte.
- `boxed.rs`, `box_carets`, `sync_boxed`, `unbox` and the byte-offset selection of code cells go. The notebook's
  selection for a code cell is the editor's `Selection`.
- Notebook-level undo (cell insertions, deletions, style changes) stays where it is and interleaves with the
  cells' histories by a shared sequence number.

### 3.4 View (`mc-notebook`)

- Lays out `cell.content` (cached by revision and width). One painter for code cells (`typeset::paint`); the
  galley path for code goes.
- Caret: the stop's box, 2 px wide (1 px in rows smaller than 0.8 of the text size), drawn to the right of the
  boundary when the item on its left ends there, so it never covers a glyph.
- Syntax colours come from the canonical text through the text map, as today.
- Round 35: where white space between two operands multiplies (design 04, 3.1), a faint `×` in the theme's faint
  text colour is drawn centred in that space (after the first line's last character for a product across a line
  end); it is not an item: never selected, copied or a caret stop. The box editor inserts `*` only where a box
  touches an operand directly.
- Hit testing: nearest ink box first, then the nearest stop of its row.

## 4. Behaviour kept and behaviour changed

Kept: every key binding, the box keys, aliases, the Insert menu, clipboard flavours, Find, wrapping and indentation of
long lines, the canonical text and the file format.

Changed (on purpose):
- A code cell never changes mode: the caret, selection and undo behave the same before and after a box exists.
- Undo restores the exact selection, always.
- The caret is one text line tall everywhere, at the size of the row it is in.
- Automatic closers, pair deletion and wrapping work the same in every row.

## 5. Tests that must pass before the old editor is deleted

- The scripted window checks: `input-corpus1`, `input-corpus2` (98 formulas), `input-caret` (37), `input-brackets`
  (11), `typeset`, `clipboard`, `warnings`, `help`, `zoom`, `manipulate`.
- `mc-nbmodel` and `mc-boxes` unit tests, moved to the new API where they tested the old one's internals.
- New: property tests of the editor (a random command sequence then full undo gives the first tree and selection;
  redo gives the last; `from_text(to_text(tree)) == tree` after every command), a caret-geometry test (every stop is
  one line tall at its row's scale), and a plain-text equivalence script (typing, word motions, line motions, automatic
  closers and indentation in a cell without boxes give the texts the old editor gave).
- Typing latency in a 300-line cell and in a 40-box cell: no slower than now (2.4 ms and 9.1 ms per frame).

## 6. Order of work

1. `mc-edit`: positions, transactions, history, commands, property tests (no window).
2. Layout: per-position stops and ink boxes; the `Geometry` trait.
3. Model: code cells hold trees; commands route to `mc-edit`; the old paths behind a switch for comparison.
4. View: one painter, caret and selection from the layout.
5. The scripted checks, the equivalence script, latency; then delete `mc-boxes/src/edit.rs`, `boxed.rs` and the plain
   code-cell paths of `mc-nbmodel/src/edit.rs`.

## 7. Long cells (round 37)

The open item of round 34 (a keystroke still cost work linear in the cell: composing the line layouts into one, and
a colour for every character) is closed:

- `mc_boxes::layout::layout_by_lines_shared`: the line cache keeps the layout it composed last and where each logical
  line is in it. The next call finds the lines the row shares with the last one at its start and its end, lays out
  or fetches the lines between, splices their pieces in place and shifts the item numbers of the lines after them
  (integers only); when the first line after the change moves (an Enter, a line that grew taller) the lines after
  it are composed again from their own layouts. The result is a composition from the start, number for number (a
  random-edit test compares them after every edit). The view lets go of its layout before asking for the next.
- Colours are asked for the characters drawn only (`typeset::Ink::colours` is a function of row and item): the
  syntax colouring and the text map, kept by revision, give the colour of a visible character.
- Typing latency (release, `type` probe, 1000 x 760, median a frame on a shared machine): at the end of a 2,000-line
  cell 11.0 and 11.5 ms before, 2.8 to 3.9 ms after; at its top 2.3 to 2.6 ms; a 300-line cell 4.4 ms before,
  2.2 ms after.

## 8. A bracket at the top of a long cell (round 38)

An opening bracket typed at the top of a long cell changes the bracket depth every following line starts at, and
the line cache was keyed by depth: every line was laid out again. A logical line that fits on one line depends on
its depth only through its first indent, so the cache now keys lines by their items, lays a line out once at depth
0 and draws that layout at any depth where the line still fits, moved right by the depth's indent; a line that wraps
at its depth is laid out at that depth. Lines after an edit whose depth changed are moved sideways in place, from
their own layouts, so the composed layout is still the fresh composition, number for number. The faint
multiplication signs were placed again by a scan of the cell for each sign whenever an edit before them moved them:
the text map now finds a byte's item by binary search. Window probe (`tests/selftest/long-cell.txt`, a 2,000-line
cell, median frame): brackets typed at its top 24.5 and 25.3 ms before, 3.8 to 4.7 ms after; typing at its top and
its end unchanged (2.7 to 4.0 ms, within the machine's load).
