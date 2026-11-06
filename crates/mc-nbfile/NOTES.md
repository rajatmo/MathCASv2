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

## Pictures (package nb-m4)

`images`: `image_path` / `image_value` (the value text `image("path")` with the language's escapes), `ImageFormat`
(PNG, SVG by extension or content), `picture_size` and `svg_size` (PNG header, SVG root element), `size_of` /
`size_value` (`size=WxH`), `resolve`, `sidecar_dir`, `store_images` (copy into `<name>.files/` under
`<word>-<12 hex of FNV-1a>.<ext>`, rewrite the output lines) and `remove_unreferenced`.

## Round 11 (package nb-m6)

Panic audit: the reader has no `unwrap`/`expect` on input-dependent values (one `unreachable!` on a state the block
loop excludes). `tests/fuzz.rs` runs texts built from the format's pieces, damaged copies of `examples/notebook.mcn`
(lines dropped, repeated, swapped, cut inside characters, bytes flipped and read lossily) and random bytes through
`Notebook::read`, `print` (what was read prints and reads again), `parse`, `parse_cells`, `Attrs::parse` and the
picture helpers (`svg_size`, `image_path`, `ImageFormat::sniff`): 3,000 cases of each kind by default
(`MC_FUZZ_CASES`), 60,000 of each ran clean once. No panic was found.
