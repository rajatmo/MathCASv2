# mc-edit: the input editor (round 34)

The editor of the notebook's code cells, written to design 11 (`docs/design/11-editor.md`). One document model (a
box tree; a cell without boxes is a row of characters), one editor written once over the tree, undo as transactions
with the selection before and after, geometry from the layout through a trait.

## Structure

- `pos.rs`: `Position { path, offset }`, `Selection { anchor, head }`; `Selection::range` gives the smallest run of
  items of the ends' common row (a box counts whole when an end is inside it), as the old editor did.
- `tx.rs`: `Splice { path, at, removed, inserted }`, `Transaction { splices, before, after, kind }`, `History`.
  Splices record what they removed, so the inverse is exact by construction; typing merges while it continues a
  word (`Transaction::continued_by`: typing after typing unless white space was followed by a non-space; deleting
  after deleting).
- `editor.rs`: `Editor::apply(&Command, &Context) -> Outcome { tx, status }`. The tree changes only through
  `Editor::edit` (one splice; a slot left empty gets its placeholder in the same splice), so every change is in the
  transaction. Marks that belong to positions (automatic closers, a pending alias) move with the splices.
- `rules.rs`: the text behaviour of a row of items: natural words (a box is a word of its own), lines, lexical place
  (code, string, comment), implicit multiplication, what an unwrapped box leaves.
- `geom.rs`: `Geometry` (locate a position on a visual line, a line's items, the nearest position on a line),
  implemented by `mc_boxes::layout::Layout`; `Logical` is the root row's line breaks with one unit per item (key
  scripts and tests; it answers as `FakeEnv` did for plain cells).

## Decisions where design 11 is silent

- E34.1 Operator pairs (`->`, `<=` ...) are two characters for the caret and Backspace in every row. The old box
  editor stepped over them as one, the plain editor did not; the notebook draws them as their two characters
  (NBUI.3), so the plain behaviour is the one that matches what is seen.
- E34.2 Automatic closers are remembered as positions of the closer item and kept through every editing command
  (typing, deleting, box keys, Ctrl+Space, aliases); motions, clicks and selection commands drop them. The old plain
  editor dropped them on any non-typing command, the box editor stepped over any matching closer. With the closers
  kept through box keys, `(` `a` Ctrl+6 `2` Ctrl+Space `)` steps over the automatic `)` as the box editor did.
- E34.3 Backspace between an opening bracket and its closer removes both when the pair is empty (round 33, box
  cells) or the closer came automatically; a quote pair only when its closer came automatically.
- E34.4 Esc with a selection does nothing (the plain editor's rule; the box editor deleted the selection).
- E34.5 Tab selects the next placeholder when the tree has any (beep when none is after the caret); without
  placeholders it indents (design 08, 3.1). Shift+Tab mirrors it.
- E34.6 Return keeps the line's indentation, one level more after an opener, and splits `(|)`, in every row (the
  box editor inserted a bare line break).
- E34.7 Home and End: in a slot the edge of the slot's line, pressed at that edge the edge of the visual line the
  box is on (round 32, R32.4, formerly in `mc-nbmodel`); on the root row the visual line from the geometry.
- E34.8 Ctrl+Backspace and Ctrl+Delete delete a word of the row; a box ends the word (the box is entered by the next
  press, as Backspace does), so one key never deletes a filled box.
- E34.9 An opener typed over a selected placeholder replaces it (design 08, 4.3) instead of wrapping it: `"□"` in a
  string would not read back.
- E34.10 Ctrl+Shift+B (and Ctrl+. in a tree without boxes) run mc-syntax over the canonical text and map the byte
  range back to items (`TextMap::find`), growing until the item selection grows: plain cells get exactly the old
  units; Ctrl+Shift+B now also works in cells with boxes (the box editor beeped).

- E34.11 Up from the insertion point below a code cell lands on its last visual line at the goal x (the line's
  start without one), Down from above on its first line: the plain cells' rule, now also for cells with boxes
  (they were entered at their end or start).
- E34.12 An alias in a code cell is a mark of the editor, never a character of the text: the plain editor wrote
  `⋮` into the cell (and into the file if saved mid-alias).
- E34.13 A Backspace or Delete that removes a box (a filled box unwrapped at a slot's edge, a blank box, a
  selection holding a box) is an undo step of its own (`Kind::Other`), not merged into the deleting run around it
  (`input-caret` `delete/denominator`: the old editor made it a step by its `typeset` mark change).
- E34.14 Ctrl+. grows by mc-syntax's units in a tree without boxes (the plain cells' rule) and by word, row, box in a
  tree with boxes (the box editor's rule); running the grammar over the canonical text of a tree with boxes selected
  the whole box first (`typeset` self-test).
- E34.15 Undo history: one notebook history whose edit steps carry the transaction (design 11, 3.3, speaks of
  per-cell histories joined by a sequence number; one history gives the same order).

## Tests

- `tests/editing.rs`, `tests/matrix.rs`, `tests/bugs_round21.rs`: the box editor's tests of rounds 9 to 33, run on
  the new editor through `tests/common` (one method per command). Changed expectations: E34.1 (`a->b`), E34.7
  (Home and End at a slot's edge go on to the line's edges).
- `tests/props.rs`: random command sequences (typing with and without automatic closing, deletions, motions, box
  keys, aliases, clicks, paste, comment, select all, Ctrl+.): each transaction redone on the tree before gives the
  tree after and reverted gives the tree before, its `after` is the selection; all undone gives the first tree and
  the first change's selection, all redone the last; `from_text(to_text(tree)) == tree` after every command except
  where the canonical text cannot say it (a box or placeholder in a comment or string, a box a deletion left touching
  a name, characters spelling a box shape: the known limits of design 08, 4.4, unchanged). 8000 cases pass;
  `MC_EDIT_SEED`, `MC_EDIT_CASES`.

## Status at the end of round 34

- Done: design 11 steps 1 to 5. The editor of rounds 9 to 33 is deleted (`mc-boxes/src/edit.rs`,
  `mc-nbmodel/src/boxed.rs`, the code-cell paths of `mc-nbmodel/src/edit.rs`: code newlines, comments, the code
  rules of automatic closing and outdenting, the grammar's Ctrl+., templates in plain cells; the `MC_EDITOR` switch).
  Code cells and the read-only output caret (`outbox.rs`, through `Editor::slot_vertical` for structural Up and Down)
  use this crate only. The box editor's tests (`editing`, `matrix`, `bugs_round21`, `fuzz`) run here.
- Latency bar (decision of round 34): the 300-line cell without boxes stays at or under 3.0 ms median a
  frame with a worst frame no worse than the old editor's; measured 2.26 to 2.61 ms (old 2.04 to 2.14) with the worst
  frame 3.1 ms (old up to 9.9). The 40-box cell is faster than before (7.4 against 7.6 to 7.8 ms).
- Agreement with the old editor: its results for the 174 key scripts without boxes or aliases are frozen in
  `mc-nbmodel/tests/parity/old-editor.txt` (recorded before the deletion); `tests/parity.rs` compares against it.
- Left: painting and hit-testing long cells from the cached line layouts without composing one layout, and a colour
  table kept per line (the remaining cost linear in the cell per keystroke).

## Round 35: a space multiplies

- `rules::needs_times` puts the box editor's `*` only where a box touches an operand directly (`x((sqrt(Q)))`
  would be a call, `((sqrt(Q)))x` would not read). With white space between them nothing is inserted: the space
  multiplies now (design 04, 3.1) and the notebook draws a faint `×` there. Tests `editing.rs`
  (`implicit_times_between_boxes_and_operands`, `operands_are_primaries`) changed accordingly.

## Round 37

- The gap left at round 34 (composition and the colour table linear in the cell) is closed in `mc-boxes`
  (`layout_by_lines_shared`) and `mc-notebook` (colours of drawn characters only); `tests/timing.rs` takes the
  number of lines from `MC_LINES` and times the in-place composition at the end and at the top of the cell.

## Round 38: closers kept inside their pair

- `apply` cleared the automatic closers on every caret motion. A motion without Shift now keeps those the caret is
  still inside of (`keep_closers_around_caret`: in the closer's row between it and its opener, or in a box between
  them); selections, clicks and motions out of the pair forget them as before. Test
  `tests/editing.rs::closers_survive_motions_inside_their_pair` (three bracket kinds through a fraction filled with
  Down, Left and Right inside a call, a motion out of the pair).
