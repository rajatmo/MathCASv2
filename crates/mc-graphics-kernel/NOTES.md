# mc-graphics-kernel: notes

## Round 34

- Split out of `mc-graphics`: the natives of the graphics term, `transform.rs` and `compose.rs` (the layouts of
  pictures), and the interop tests, moved unchanged. The model, renderers, colours and the plot memos stay in
  `mc-graphics`, which no longer depends on the evaluator.

## Round 39: images

- The colour natives (`color_negate`, `darker`, `lighter`, `color_convert`) leave an image value to the library
  rules for images in `lib/lists.mc` (they used to fail on it).
- `color_convert(hue(h, s, b), rgb_color)` with symbolic parts: each channel `b - b s max(0, min(k, 4 - k, 1))`,
  k = mod(n + 6 h, 6) with n = 5, 3, 1 (the hexcone model without cases; Smith, "Color gamut transform pairs",
  SIGGRAPH 1978), so a colour function of plot variables can be plotted.
