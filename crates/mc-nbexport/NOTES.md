# mc-nbexport: a saved notebook as HTML or PDF (round 13)

A library crate and the `mc-repl --export NB.mcn OUT.html|OUT.pdf` flag. The file is converted as saved; nothing
is evaluated.

API (`src/lib.rs`):

- `Options { images: Images, title: Option<String> }`, `Images::{Embed (default), Folder}`;
- `to_html(nb, base, out, opts) -> io::Result<String>` (`base`: the notebook's folder, relative picture paths are
  resolved in it; `out`: the target file, which names the picture folder `<out stem>.files/` of `Images::Folder`
  and gives the default title);
- `to_pdf(nb, base) -> io::Result<Vec<u8>>`;
- `export(input_mcn, output) -> Result<(), String>`: `.html` / `.htm` (pictures embedded) or `.pdf` by the output's
  extension (any case); errors name the file (unreadable, malformed with its line, unknown extension, write error).

Layout of the crate: `model.rs` (cells to parts: headings, paragraphs, items with their numbers, code, output
blocks split into generated cells as the notebook groups them: consecutive lines of one prefix), `html.rs`,
`pdf/mod.rs` (blocks of lines, page flow), `pdf/fonts.rs` (fonts, fallback, metrics for `mc_boxes`),
`pdf/writer.rs` (objects, Type 3 fonts, images, pages).

## HTML

One file: inline CSS (the notebook's light theme), no scripts, no external links. Title, chapter, section,
subsection, subsubsection are `h1`..`h5` (subchapter an `h2` of its own class; subtitle and subsubtitle paragraphs),
text cells `p` with their line breaks kept (`white-space: pre-wrap`), items one `ul` per run of item cells with the
bullet or number as a marker span, code cells `pre` with syntax colouring from `mc_syntax::classify` (classes of the
light palette), program cells verbatim between rules. Prose code spans (backticks) are `code`.

- outputs and typeset inputs keep their characters as text in the code font and put **each box** (square
  root, fraction, power) in an inline MathML `<math>` element (`msqrt`, `mfrac`, `msup` over `mi`/`mn`/`mo`/`mtext`
  tokens), rather than one `<math>` per output. This is the notebook's own look (linear text plus three boxes,
  design 4.0), keeps long values wrapping like text (MathML Core has no line breaking) and copies as the printed
  characters. `math-style: normal` keeps fractions full size; the radical sign comes from a math font
  (`STIX Two Math`, `Cambria Math`, `math`), the tokens from the code font.
- Values right-justified; `print` output, failures (`#!`, and values that are `failure(...)`) and messages (`#?`)
  left, failures and messages in the message colours.
- pictures are `<img>` with `width`/`height` from `size=WxH` (CSS pixels = notebook points), else the
  file's natural size; PNG and SVG both as `data:` URIs (an SVG through `<img>` never runs scripts, which inline
  SVG could). `Images::Folder` copies each file once into `<out stem>.files/` (a name clash gets a `2-` prefix) and
  links it relatively (percent-encoded). A missing picture is a framed "picture not found: path".
- No fonts are embedded: the stylesheet names DejaVu Sans Mono / Noto Sans first and common system fonts after.

## PDF

- **A4** portrait (595.28 x 841.89 pt): the ISO paper size of most of the world; Letter printers scale it
  to fit (about 3 %), while the reverse would crop. Margins 2 cm left/right/top, 62 pt at the bottom with the page
  number `n / N` centred below the text.
- fonts are **Type 3 fonts built from the glyph outlines** (as `mc-graphics` does for plots), one series per
  source font (Noto Sans regular/bold, DejaVu Sans Mono regular/bold, the DejaVu symbol subset, STIX Two Math), 256
  glyphs each: a subset by construction (only glyphs drawn), one code path for TrueType (`glyf`) and CFF (the math
  font), no font subsetter needed. Each font has a `ToUnicode` CMap, so text is selectable and searchable (checked by
  `pdftotext` and by the tests' own extractor). The cost is no hinting (fine at print resolutions) and a little
  more size per glyph than embedded TrueType; the bundled fonts are already subsets of ~100 KB each, which a
  CIDFontType2 embedding would carry whole.
- Fallback per character: the asked font, then the math font, the symbol font, the code font, else `?`.
- Prose (Noto Sans 10.5 pt, headings 12 to 24 pt bold) is wrapped at spaces (a word wider than the column breaks
  between characters); code (DejaVu Sans Mono bold 9 pt for inputs, regular for `print`) is wrapped at the
  character, coloured by token class, on a tinted background (initialization cells bluish), tabs as four spaces.
- Typeset content (values, typeset inputs, display formulas) goes through `mc_boxes::layout` with `CodeMetrics`
  (the code font for every character it has, the math font for the rest, no math italics, the same options as the
  notebook: no operator spacing, no small inline rationals); its `Draw`s become text runs (merged while contiguous),
  math glyphs (radical signs), rules (fraction bars, overbars). Values are right-aligned with `Layout::align_right`.
- Pages: a document is a list of blocks of lines; a page break may fall between any two lines (a long code cell or
  paragraph continues; the background is drawn per page fragment) but never inside a line (a picture is one line,
  scaled down to fit the column and the page). A heading is kept with the first line after it, and a code
  cell of at most 12 lines with the first line of its output (no input stranded at a page bottom above its plot).
- Pictures: PNG decoded with `png` (palette, grey, 16-bit expanded to 8-bit RGB), written as an RGB image XObject
  with a DeviceGray soft mask when some pixel is not opaque; one XObject per file however often it is shown. An SVG
  picture uses its `.png` sibling when there is one, else (and for unreadable files) a framed "picture not found:
  path". Size: `size=WxH` (else the natural size) times 0.75 pt per notebook point.
- Info dictionary: `/Title` (the first title cell, UTF-16 when not ASCII), `/Producer (MathCAS)`.

## Dependencies

mc-nbfile, mc-boxes, mc-syntax, mc-mathfont (workspace); `ttf-parser` 0.25 (MIT / Apache-2.0), `png` 0.18
(MIT / Apache-2.0), `miniz_oxide` 0.8 (MIT / Zlib / Apache-2.0); all were already in `Cargo.lock`. The font files
are `include_bytes!` from `crates/mc-ui/fonts/` (DejaVu licence, Apache-2.0 for Noto; see the licence files there)
and STIX Two Math through `mc-mathfont` (SIL OFL 1.1). No dependency on mc-graphics, mc-engine, eframe or mc-ui.

## Tests (20 unit + 1 doc test here, 2 in `mc-repl/tests/export.rs`, 1 assertion block in `mc-repl` `cli` tests)

- HTML of a generated notebook (title with `<` and `&`, section, prose with a code span, items, coloured code,
  print, a value with power/fraction/root, a typeset input fraction, a failure, a message, an embedded PNG made in
  the test, a missing picture): every expected element, balanced and properly nested tags, no `<script>`, no URL;
  `Images::Folder` writes the picture and links it percent-encoded.
- PDF: `%PDF-1.4` ... `%%EOF`; the text read back through the `ToUnicode` maps (a small extractor in the tests);
  one image XObject with its soft mask; the title; 100 one-line code cells give exactly 4 pages (25 per page) with
  `n / 4` on each; a 200-line code cell continues over pages; a 400-character line wraps; fraction bars are drawn;
  more than 256 distinct glyphs of one font (two Type 3 fonts) read back.
- `export` by extension (`.html`, `.HTM`, `.pdf`), unknown extension, missing file, malformed notebook.
- Units: code spans, numbered items, output grouping, MathML of boxes, colouring and escaping, URL encoding, number
  and string formatting, ToUnicode surrogate pairs, prose and code wrapping, font fallback.
- `mc-repl --export` on the binary: HTML and PDF written, exit 0, nothing on stdout/stderr; unknown extension,
  missing notebook and a missing argument exit 1 with a message on stderr.

## Measurements (release build, `mc-repl --export`, wall time including ~28 ms process start)

| notebook | HTML | PDF |
|---|---|---|
| `examples/notebook.mcn` (5 code cells) | 36 ms, 5.7 KB | 39 ms, 42 KB, 1 page |
| `docs/functions/plot.mcn` (10 PNG plots) | 68 ms, 1.4 MB | 82 ms, 297 KB, 10 pages |
| generated, 1,000 cells (prose, items, code, typeset values, prints, failures; 4,500 lines) | 92 ms, 261 KB | 131 ms, 166 KB, 68 pages |

The PDFs open and render in poppler (`pdftoppm`, `pdfinfo`: A4, page count, title) and their text extracts with
`pdftotext`; the HTML renders in a Chromium-based browser with MathML.

## Gaps

- HTML embeds no fonts (the look depends on the reader's fonts); PDF has no document outline (bookmarks) for the
  headings and no links.
- Cell labels (input and output numbers) are not shown; closed groups and hidden cells are exported open (an export is the
  whole document).
- Typeset inputs are not syntax-coloured (plain inputs are); matrices and other boxes than root, fraction, power
  (older notebooks) show as their linear text, as in the notebook.
- SVG pictures of other programs in PDF are drawn as vectors only as far as flat colours go (gradients, patterns,
  masks, filters and text left out; text needs usvg's text feature and fonts). A compressed `.svgz` is embedded in
  HTML as it is, which browsers do not show from a `data:` URI.
- PNG pictures are decoded and recompressed (not passed through with predictors), so very large plots cost time.
- Right-to-left and complex scripts are drawn glyph by glyph without shaping or kerning.

## Round nb-svg: drawings as vectors

- HTML: a plot (the SVG sibling of its PNG, or an embedded `.svg` written by `mc-graphics`) is an inline `<svg>` when
  pictures are embedded: ids prefixed `p<n>-` per drawing, namespace declarations dropped (no URLs), `width`/`height`
  from `size=` or the drawing, `role="img"` with the path as label. With `Images::Folder` the SVG file is copied and
  linked. Light colours kept.
- PDF: `pdf/vector.rs` parses the drawing with `usvg` (no text features) and writes it as a form XObject: paths with
  flat fills and strokes (width, caps, joins, miter limit, dashes, even-odd), opacity through `ExtGState`
  (`/GS<thousandths>`), clip paths, transforms, embedded PNG images (`/Interpolate false` for pixelated cells);
  gradients, patterns, masks and filters (never written by `mc-graphics`) are left out. Forms share the document's
  resource dictionary; `/Matrix [1 0 0 -1 0 h]` flips the y-down content. A drawing that cannot be read falls back to
  its PNG sibling as before.
- Dependency: `usvg` 0.49 (Apache-2.0 OR MIT, default features off; already in the lock).
- Tests (2 new): operators of paths, clips and opacity; plots exported as inline SVG (ids unique per drawing, no URLs)
  and as forms (colour, width, clip, glyph fill; the other program's PNG still an image).

## Round 35: `.svgz` pictures

- `gzip.rs`: `read_picture` decompresses a gzip file (header flags handled, `miniz_oxide` for the deflate part) and
  `picture_size` reads a `.svgz` drawing's size from its text. HTML inlines it as it does an `.svg` (a folder export
  writes the plain `.svg`); PDF draws it as a form. A documentation page opened as a notebook exports with no PNG.

## Round 37: plots at the column's width

- A plot with a column twin (`plot-N.column.svg` beside its picture, from `mc-plot`; embedded in saved notebooks by
  `mc-nbfile`) is drawn from the twin at the width of the picture column (459.9 pt): its 13-point text prints at
  10.5 pt, the size of the code, the proportion the window shows. Measured in the PDF of a notebook with
  `plot(sin(x), [x, 0, 6])` and `plot(exp(x), [x, 0, 20])`: digit outlines 6.86 pt high (10.5 pt type; 6.10 pt,
  9.3 pt type, from the screen drawing), exponents 4.85 pt high (7.4 pt type; 6.6 pt before). A picture resized in
  the window (`size=`), a plot of its own size (no twin), an older notebook and an unreadable twin: the screen
  drawing at 0.75 as before. Decision u37.1 in design 08, 7.5.
- Prose keeps its line breaks, as the window does (see `mc-notebook/NOTES.md`, round 37).
- Test `plots_with_a_column_twin_are_drawn_from_it_at_the_column_width`.
