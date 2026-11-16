# mc-nbexport

Exports a saved `.mcn` notebook as a document to read, print or send: one self-contained HTML file or a paginated
PDF. `export` picks the format from the output file's extension.

## Modules
- `model`: what an export shows of a notebook, independent of the target format: the cells as a flat list of parts.
- `html`: the HTML export, one file with inline CSS, no scripts and no external links.
- `pdf`: the PDF export, blocks of lines flowed onto A4 pages with margins and page numbers.
- `gzip`: reading picture files that may be gzip-compressed (plots stored as `.svgz`).

## Design
- Only the file as saved is converted; nothing is evaluated.
- Both formats show what the notebook shows: headings, text and items in the prose font, code cells with syntax
  colouring, typeset inputs and outputs with roots, fractions and powers, and pictures at their `size=` or natural size.
- Typesetting uses the notebook's own renderers (`mc-boxes`, `mc-typeset`) and the bundled math font (`mc-mathfont`);
  the file is parsed by `mc-nbfile`; code colouring uses `mc-syntax`.
- PDF streams are Flate-compressed, PNG pictures are decoded and SVG drawings are parsed into vector paths.

## Tests
- Unit tests in `src` export generated notebooks and check the HTML's elements and nesting and the PDF's pages and
  text.
- Run them with `cargo test --release -p mc-nbexport`.
