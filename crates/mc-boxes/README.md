# mc-boxes

Two-dimensional typesetting without a user interface: the box tree, its canonical and linear text forms, the alias
table, LaTeX output, and layout with line breaking over a metrics trait.

## Modules
- `tree`: the box tree (`Row`, `Item`, `Node`, big operators), positions and paths, `notation`.
- `canonical`: `from_text` and `to_text`, the lossless text form of a box tree, with a position map.
- `linear`: `linear_text`, `linear_segments` and `restrict`, which reduces a tree to the boxes cells use.
- `aliases`: the escape-name table (`data/aliases.tsv`): characters and box templates inserted by name.
- `latex`: `to_latex`.
- `ops`: operations on trees.
- `scan`: reading the escape-delimited text form.
- `layout`: `engine`, `lines`, `metrics`: positioned characters, math-font glyphs and rules; painting is the caller's.

## Design
- Notebook cells use three boxes only: the square root, the fraction and the power.
- The text form is canonical: a tree read from its text and printed again gives the same text.
- Nesting is bounded by `MAX_NESTING`.
- Layout is measured through a metrics trait, so it needs no font or window system.
- Depends on `mc-syntax` and `mc-mathfont`; it does not depend on the evaluator.

## Tests
- Unit tests in `src`.
- Integration tests in `tests/`: property tests of the canonical text form, layout tests, and snapshot files.
- A documentation example on the crate page.
- Run: `cargo test --release -p mc-boxes`
