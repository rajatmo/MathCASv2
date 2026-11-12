# mc-typeset: output typesetting

Texts read through the parser into box trees of `mc-boxes` (design 08, sections 4.5 and 4.6). Dependencies:
`mc-boxes`, `mc-syntax` (dev: `mc-api`).

## Round 38: a crate of its own

The two modules were `mc-boxes`' `printed` and `math` until round 38 (design 12, section 4: architecture step 8); the
code, its decisions and its tests are unchanged, and their history is in `crates/mc-boxes/NOTES.md` (decisions NBB,
the rounds nb-ui, 13, 19 and 20).

| module | contents |
|---|---|
| `math` | `math_text(printed) -> Typeset`: the notebook's one look of an output (square roots, fractions, powers; labeled and legended values as `Node::Labeled`), `label_rows` |
| `printed` | `typeset_output(printed) -> Typeset{row, message, map}` (4.5), `typeset_output_with`, `typeset_input(text) -> Result<Row, Vec<Diagnostic>>` (Ctrl+Shift+N), `output_input_text`, `output_input_boxes` (a part of an output copied as input), `SourceMap` |

What stayed in `mc-boxes` because the tree needs it without a parser of outputs: `has_boxes`, `restrict`,
`linear_segments`, `ops::is_swatch` (LaTeX skips swatches), `is_one_group` (the bracket scan of the canonical text).

Tests: the unit tests of both modules, `tests/fuzz.rs` (random printed outputs through typesetting and layout,
moved from `mc-boxes`), `tests/latex.rs` (LaTeX of typeset outputs).
