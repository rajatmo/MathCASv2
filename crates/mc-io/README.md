# mc-io

Import and export of data files: lists as CSV, TSV, whitespace tables, lines and JSON; PNG images; PDF documents. Also
image values and functions, `interpolation`, `put` and `get`, binary data, and file queries.

## Modules
- `import`, `export`: `import_data` and `export_data` with their elements and options.
- `tabular`: the text formats; `dataset`: datasets (`mc-assoc`) from and to text files.
- `raster`: PNG decoding and encoding, conversion to and from reals in [0, 1], resampling; `images`: the image values
  and `image`, `image_data`, `image_dimensions`, `image_channels`.
- `pdf_read`, `pdf_write`: reading pages through `hayro`, writing PDF with losslessly embedded rasters.
- `hook`: `register_exporter` and `register_importer`, which let other crates supply formats for their own terms.
- `persist`: `put(value, file)` and `get(file)`; `binary`: `byte_array`, `binary_write`, `binary_read_list`.
- `interp`: `interpolation(data)`, a not-a-knot cubic spline or piecewise-linear interpolant; `files`: `file_exists`,
  `file_names`.

## Design
- The format comes from the file extension, or from a format name after the file; paths are relative to the current
  directory.
- Registered exporters are tried before the built-in cases, so graphics export to PNG, SVG and PDF without this crate
  knowing the graphics model beyond `mc-graphics`'s interface.
- PDF pages are rendered on white at `dpi`; an imported page keeps a vector twin so re-exporting stays vector. Encrypted
  and damaged files give `failure(encrypted, file)` and `failure(damaged, file, message)`.
- PNG: every layout is read; written files are 8-bit unless `bit_depth: 16`.
- Depends on `mc-api`, `mc-core`, `mc-eval`, `mc-assoc`, `mc-numeric`, `mc-graphics`, `png`, `flate2`, `hayro`, `serde`.

## Tests
- Integration tests in `tests/`: text formats, datasets, binary data, images and PDF, files, interaction with other
  libraries; example programs in `examples/`.
- Run: `cargo test --release -p mc-io`
