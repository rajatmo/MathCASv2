# mc-edit

The input editor of the notebook's code cells: one document model (a box tree; a cell without boxes is a row of
characters), one editor written once over that tree, undo as transactions, and geometry obtained from a layout through
a small trait.

## Modules
- `editor`: `Editor`, `Command`, `Context`, `Motion`, `Structure`, `Outcome`: the tree, a selection, and the marks that
  belong to positions.
- `pos`: `Position` and `Selection` in a box tree.
- `rules`: the text behaviour of a row of items, written once for every row (words, lines, indentation).
- `tx`: `Transaction`, `History`, `Splice`: every command is one transaction, a list of primitive changes, with the
  selection before and after.
- `geom`: `Geometry`, what the editor asks of a layout (motions that need geometry, such as Up and Down).

## Design
- Every command produces one transaction that `revert` undoes exactly, including the selection.
- The editor holds no layout and no window: geometry comes through the `Geometry` trait.
- The box tree and its text forms are `mc-boxes`'; the reading of text is `mc-syntax`'s. The crate does not depend on
  the evaluator.
- Structures (fraction, root, power) are inserted around the selection by `Command::Structure`.

## Tests
- Unit tests in `src` and a documentation example on the crate page (apply a command, then revert it).
- Integration tests in `tests/`: editing commands, geometry-dependent motions, matrices, property tests, a fuzz test
  that applies random commands, and timing tests.
- Run: `cargo test --release -p mc-edit`
