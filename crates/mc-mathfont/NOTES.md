# mc-mathfont: OpenType MATH data and the bundled math font (notebook round, P5)

A port of the previous project's font module. Bundles `fonts/STIXTwoMath-Regular.otf` with its licence
`fonts/OFL.txt` (SIL Open Font License). Dependencies: `ttf-parser` 0.25, `ab_glyph_rasterizer` 0.1.

API: `FONT_DATA`, `FONT_LICENSE`, `FONT_FAMILY`, `font() -> &'static MathFont`. `MathFont`: `constants` (MATH
constants in ems); per glyph `glyph(char)`, `metrics(g)`, `italic_correction`, `top_accent`, `ssty(g, level)`,
`x_height`, `ascent_descent`, `is_extended_shape`, `variants(g, vertical)`, `assembly(g, vertical)`,
`outline(g) -> Vec<PathCmd>`; `stretch(g, target_em, vertical) -> Stretch { pieces, advance, height, depth }` (a size
variant, else an assembly from parts) and `single(g)`. `rasterize(font, glyph, px_per_em) -> GlyphBitmap { width,
height, left, top, alpha }` for crisp glyph textures drawn by id.

Tests: 6.
