# mc-nbfile: the `.mcn` notebook format (notebook round, P1)

The format half of the former `mc-kernel/src/notebook.rs`, extended with the grammar of
`docs/design/08-notebook.md` section 7.2:

- all 21 cell kinds (`input` is another spelling of `cell`); attributes on markers and on the header (unknown ones
  are kept and written back); several output blocks per cell, each with attributes (`n=`, `size=`); `#?` message
  lines;
- `Notebook::read` returns warnings (a misspelt marker by edit distance, malformed attributes) and always keeps the
  content;
- cell fragments for the clipboard: `print_cells` / `parse_cells` (first line `#% cells`);
- `write_atomic`, `first_lines`.

API: `Cell { kind, attrs, lines, outputs, gap }` replaces the old `CellKind::{Code, Text}`; `Cell::code/text/source/
prose` remain, plus `content()`, `output_lines()`, `set_output(Option<Vec<OutputLine>>)` (one unnumbered block:
exactly what the runner wrote before). A notebook with only cell and text cells, no attributes and at most one
unnumbered block per cell writes exactly the old format.

Tests (12): old-format round trip (LF and CR LF), the extended grammar, warnings, attributes, fragments, 500
generated notebooks, the example notebook, atomic writes.
