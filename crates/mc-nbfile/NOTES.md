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

## Pictures

`images`: `image_path` / `image_value` (the value text `image("path")` with the language's escapes), `ImageFormat`
(PNG, SVG by extension or content), `picture_size` and `svg_size` (PNG header, SVG root element), `size_of` /
`size_value` (`size=WxH`), `resolve`, `sidecar_dir`, `store_images` (copy into `<name>.files/` under
`<word>-<12 hex of FNV-1a>.<ext>`, rewrite the output lines) and `remove_unreferenced`.

## Round 11

Panic audit: the reader has no `unwrap`/`expect` on input-dependent values (one `unreachable!` on a state the block
loop excludes). `tests/fuzz.rs` runs texts built from the format's pieces, damaged copies of `examples/notebook.mcn`
(lines dropped, repeated, swapped, cut inside characters, bytes flipped and read lossily) and random bytes through
`Notebook::read`, `print` (what was read prints and reads again), `parse`, `parse_cells`, `Attrs::parse` and the
picture helpers (`svg_size`, `image_path`, `ImageFormat::sniff`): 3,000 cases of each kind by default
(`MC_FUZZ_CASES`), 60,000 of each ran clean once. No panic was found.

## Round nb-svg: drawings embedded

- `embed`: a plot's SVG (the sibling of its PNG, or an SVG `is_ours`: `class="mc-graphic"` on the root) is embedded
  deflated (`@plot-<hash>.svg deflate`) and the value names it; an SVG deflating to more than `SVG_LIMIT` (256 KiB)
  becomes a PNG at `HIRES` (2) pixels per point made with resvg (no text features; light colours) and marked with a
  `pHYs` chunk (`images::with_density`); other pictures as before. A file is stored once per notebook, in the first
  block that shows it (`extract` reads all blocks first). `Embedded::stored` gives the bytes as stored.
- `images::png_density` (72 k dpi in `pHYs` for k = 2, 3, 4 within 1 %), `picture_size` divides by it.
- Dependency: `resvg` 0.49 (Apache-2.0 OR MIT, `raster-images` only; already in the lock).
- Tests (3 new): drawings embedded deflated and shared by two outputs, round trip through the cache and again without
  it; a dense drawing becomes a 1280 x 800 PNG of density 2 shown at 640 x 400; density chunks written and read (CRC
  checked against an independent implementation).

## Round 22

- ui B21-12: a code cell's gap is its trailing lines that are exactly empty (a line of white space is source);
  `Cell::exact` writes a content line for line. The model maps extra gap lines to line breaks ending the cell text.

## Round 37: the column twin

- `embed::COLUMN_TWIN` (`column.svg`): a plot embedded as its SVG carries its column twin, deflated, when the twin is
  a drawing of ours and deflates within `SVG_LIMIT`; a dense plot embedded as PNG does not. A notebook saved without
  its cache keeps a carried twin: siblings are matched by the stem plus a dot (`plot-h.column.svg` is a sibling of
  `plot-h.svg`). Cost about 4.6 KB of base64 per 2D plot. Test `a_plots_column_twin_travels_with_its_svg`.
