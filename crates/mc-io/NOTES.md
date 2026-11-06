# mc-io: data, image and PDF import and export

The format table is the crate documentation (`src/lib.rs`). Round 9 (package io) added PNG pixels and PDF.

## Modules

| module | contents |
|---|---|
| `lib.rs` | crate docs and format table, `FUNCTIONS`, `install`, `Format`, option parsing (`Opts`) |
| `tabular` | CSV, TSV, whitespace tables, lines, JSON |
| `raster` | PNG decode/encode, conversion to and from reals, resampling |
| `images` | image values: `image`, `image_data`, `image_dimensions`, `image_channels` |
| `import`, `export` | the two natives |
| `pdf_read` | opening, rendering and text extraction through `hayro` |
| `pdf_write` | a PDF writer: raster pages, page import as form XObjects |
| `hook` | the exporter registry |
| `interp` | `interpolation` (not-a-knot cubic spline) |

## PNG

- Reads gray, gray+alpha, RGB, RGBA, palette (expanded), 1/2/4/8/16-bit samples.
- `image(array)`: h x w (gray) or h x w x c (c = 1..4); values clipped to [0, 1], NaN to 0, exact numbers through `n`;
  `bit_depth: 16`.
- `image_data(img)`: packed h x w x c reals (sample/255 or /65535); `image_dimensions(img)` `[width, height]`;
  `image_channels(img)`; the last two read only the header.
- `import_data("a.png")` gives `image(abs path)` after checking the header; elements `"image"`, `"data"`,
  `"dimensions"`, `"channels"`, `"bit_depth"`.
- `export_data` to PNG: an image (byte copy without options), a plot or an array; options `scale` (triangle filter,
  premultiplied alpha), `bit_depth`, `dpi` (pHYs; a plot or an imported PDF page is re-rendered from its vector twin at
  that resolution: `export_data("p.png", plot, dpi: 216)` is 1920x1200).

## PDF

- `import_data("a.pdf")`: the pages as images (150 dpi default); elements `"page", k` (negative from the end),
  `"page_count"`, `"text"` (a string per page), `"text", k`; option `dpi`. Text follows each glyph's direction
  (rotated labels read as one line) via ToUnicode/glyph names.
- Rendered pages are PNGs in `temp/mathcas-images/`, each with the page alone as a PDF next to it (its vector twin);
  names hash document, page and dpi, so repeated imports reuse them.
- Export: a plot or imported page copies its vector twin; an image or array is one lossless raster page (Flate with
  PNG Paeth predictors, 16-bit kept, alpha as a soft mask; one pixel per point unless `dpi` or the PNG's own
  resolution says otherwise); a list of images, plots and hook-exportable values is a multi-page document (pages
  copied as form XObjects with every reachable object, references renumbered through a work list).
- Failures: `encrypted`, `damaged`, `io`, `too_large`, `out_of_range`, `unknown_element`, `unknown_option`,
  `bad_option`, `argument`.

## Exporter hook

`mc_io::register_exporter(head, formats, f)` with `f: for<'h> fn(&'h Heap, T<'h>, &Path, &str, &ExportOptions) ->
Result<(), String>`, `ExportOptions { dpi }`; process-wide, later registrations replace. `export_data` asks the hook
before its own cases; for a multi-page PDF the hook writes a temporary PDF whose pages are copied in; hook errors
become `failure(io, file, msg)`. `svg` is not yet one of the crate's formats (one line in `Format::named` when a hook
supplies it).

## Decisions (IO)

1. An image value stays `image("path")` naming a PNG: plots already are such values, front ends display them, a large
   image stays one small term; `image(array)` writes a PNG; `image_data` decodes on demand (no cache).
2. Files written by `image(array)` are named by a content hash in `temp/mathcas-images/` (equal arrays give `===`
   values; identical files are not rewritten).
3. `image_data` always gives rank 3 (gray is h x w x 1); `image()` also accepts rank 2.
4. `image_dimensions` is `[width, height]`; the pixel array is row first.
5. PDF import defaults to the pages as images at 150 dpi.
6. Every imported PDF page gets a vector twin, so exporting it back to PDF stays vector.
7. A second argument naming a format is the format, else an element; elements are strings or symbols; options last.
8. A multi-page list holds image values or hook-exportable values; a bare array inside a list is not a page (wrap it
   in `image(...)`), else a list of pages could not be told from an h x w x 3 array.
9. Every call into hayro runs under `catch_unwind` (a panic becomes `failure(damaged, ...)`); pages over 30000 px a side
   or 2^27 samples are refused.
10. Limits: 2^27 samples per image; the PNG decoder allocates at most 512 MiB.
11. PNG encoding uses fast compression.
12. Option failures: `unknown_option`, `bad_option`.

## Tests and measurements

22 tests: 11 unit (raster round trips in every layout at 8/16 bits with dpi, depth conversion, clipping, resizing,
truncated/flipped files; PDF number formatting; Paeth reversal; interpolation 4), `tests/io.rs` 4, `tests/media.rs` 7
(PNG round trips, plots to PNG/PDF with dpi, PDF text and pages from a hand-written 3-page file, lossless raster pages,
exporter hook, failures including encrypted and damaged files, 60 rounds of corrupted PNG/PDF through 6 readers).

`examples/media_measure.rs` (release): 4000x3000 RGB PNG header 5-11 ms, `image_data` 165-245 ms, `image(array)`
240-310 ms, export at scale 0.25 140-160 ms; 10 plots to a 10-page PDF 35-49 ms; rendering them at 150 dpi 58-63 ms
cold, 1.6 ms cached; text of 10 pages 1-2 ms (simple plot pages, not representative of heavy documents).

## Dependencies

`png` 0.18, `flate2` 1, `hayro` 0.8 (and its `hayro-*`, `vello_cpu`/`vello_common`, `fearless_simd`, `skrifa`,
`read-fonts`, `pic-scale`, `phf`, `brotli`): MIT, Apache-2.0, BSD-3-Clause. hayro-interpret embeds the standard-14
fonts (BSD-3-Clause, from PDFium), a CGATS ICC profile (CC0 1.0) and `LAB.icc` (licence not found in the crate:
check before a release).

## Known gaps

- Binding a 36M-sample rank-3 array to a name costs about 1.4 s (vs 0.17 s for `image_data` alone; a rank-1 array
  binds in 0.1 s): in the evaluator or store, not here.
- hayro: no password-protected files; blending and knockout groups partial; text in drawing order (multi-column
  layouts can interleave). Rendering one page is not interruptible (the stop flag is polled between pages).
- `image_data` re-reads the file each call; `temp/mathcas-images/` is never cleaned (as plots).
- Plot tests assume `plot` gives `image(path)` with a PDF twin.
