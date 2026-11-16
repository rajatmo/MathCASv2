# mc-typeset

Output typesetting: texts read through the parser into the box trees of `mc-boxes`, which lays them out.

## Modules
- `math`: the notebook's one look: `math_text` keeps linear text in the cell's font and turns exactly three constructs
  into two-dimensional boxes, `sqrt(x)`, `a/b` and `x^n`; `label_rows` finds labeled values.
- `printed`: text to boxes through the parser: `typeset_output` (printed output to the full set of boxes, also used for
  plot labels and Copy as LaTeX), `typeset_input` (linear input text to boxes), and `output_input_text` and
  `output_input_boxes` (a part of an output taken back as input).

## Design
- Every character of the text is kept as it is, except those three constructs; grouping follows the parse
  (`a/b*c` is (a/b)*c, `2^3^4` is 2^(3^4), `-x^2` is -(x^2)).
- Parentheses that only group a slot are not drawn (the numerator in `(x+1)/(x-1)`, the exponent in `x^(1/2)`); a
  power's base keeps its parentheses.
- Layout is not done here: this crate builds `mc-boxes` trees and a source map back to the text.
- Depends on `mc-boxes`, `mc-syntax` and `mc-api` only.

## Tests
- Unit tests in `src`, documentation examples, and integration tests in `tests/` for LaTeX output and a fuzz test over
  generated text.
- Run them with `cargo test --release -p mc-typeset`.
