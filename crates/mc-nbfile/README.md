# mc-nbfile

The `.mcn` notebook file format: parsing, printing, clipboard fragments and atomic saving. The format is plain text, so
a notebook is also a script whose statements are its code cells.

## Modules
- `lib.rs`: `Notebook::parse` and `Notebook::print`, cells and their kinds and attributes, `print_cells` and
  `parse_cells` for the clipboard, and `write_atomic`.
- `embed`: pictures carried inside the file, in the `#~` lines of an output block.
- `images`: the value text of an image (`image("path")`) and the picture files it names.

## Design
- The file is UTF-8 text with LF or CR LF line ends (a rewrite keeps them) and an optional leading BOM.
- A header line `#! mathcas <version> notebook` is followed by cells; each starts with a `#% <kind> [attrs]` marker,
  and output blocks use the line prefixes `#|` (print), `#=` (value), `#!` (failure), `#?` (message), `#~` (data).
- Every marker, prose and output line is a comment, so a notebook runs as a script; a `#%` line whose first word is
  not a kind stays an ordinary comment line of its cell.
- Pictures live inside the one file; there is no sidecar folder.
- Parsing and printing round-trip a file unchanged.
- Dependencies are limited to `miniz_oxide` (compressed pictures) and `resvg` (SVG handling); `mc-syntax` is used
  by tests only.

## Tests
- Unit tests in `src`, and integration tests in `tests/` for old file spellings, documented examples, a fuzz test of
  the parser and printer, and sample notebooks in `tests/data`.
- Run them with `cargo test --release -p mc-nbfile`.
