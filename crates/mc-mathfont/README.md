# mc-mathfont

The math font of the notebook: the bundled STIX Two Math (SIL Open Font License 1.1) and the parts of its OpenType
MATH table that typesetting needs.

## Modules
- `lib`: `MathFont`, `Constants` (fraction, script, radical, limit and stack parameters), per-glyph metrics, italic
  correction, top accent, script-size forms, size variants and assemblies; `font()` gives the process-wide parsed font.
- `stretch`: growing a glyph (delimiter, radical sign, arrow) to a target length from its variants or assembly.
- `raster`: glyph outlines and `rasterize`, for painters that draw glyphs by id.

## Design
- Every length is in ems of the font (design units divided by units per em); y is up from the baseline.
- Stretching returns a list of glyphs with offsets; size variants and assembly parts have no Unicode code point, so
  painters draw them by glyph id through `outline` and `rasterize`.
- `MathFont::new` parses any font that has a MATH table; the bundled font's licence text is exposed as
  `FONT_LICENSE`.
- Depends only on `ttf-parser` and `ab_glyph_rasterizer`.

## Tests
- Unit tests in `src` check constants, variants, assemblies and stretching on the bundled font.
- Run them with `cargo test --release -p mc-mathfont`.
