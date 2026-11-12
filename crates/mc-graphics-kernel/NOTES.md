# mc-graphics-kernel: notes

## Round 34

- Split out of `mc-graphics`: the natives of the graphics term, `transform.rs` and `compose.rs` (the layouts of
  pictures), and the interop tests, moved unchanged. The model, renderers, colours and the plot memos stay in
  `mc-graphics`, which no longer depends on the evaluator.
