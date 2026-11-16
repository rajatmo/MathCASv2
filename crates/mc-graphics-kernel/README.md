# mc-graphics-kernel

The kernel side of graphics: the natives that build and check graphics terms (primitives, directives, `graphics`,
`show`, the colour functions), geometric transformations, and pictures made of pictures (`graphics_row`, `grid`,
`labeled`, ...).

## Modules
- `natives`: the registered functions, with `install` and `install_late` registering them in a kernel.
- `transform`: geometric transformations of graphics content.
- `compose`: `resolve` and `shown_layout`, the combination and layout of pictures.
- `lib.rs`: `rgb_term` (a colour as a term), `numeric_content`, `FUNCTIONS`.

## Design
- The model, colours, themes, renderers and the 3D viewer are in `mc-graphics`, which has no evaluator; this crate adds
  the evaluator-facing part on top, so the notebook can use `mc-graphics` without the kernel.
- A primitive with symbolic coordinates stays as it is; one with numbers is checked and a wrong argument gives
  `failure(argument, f, k)`.
- Depends on `mc-api`, `mc-core`, `mc-eval`, `mc-graphics` and `mc-syntax`.

## Tests
- An integration test in `tests/` runs graphics functions through a full engine session; `mc-engine` is a development
  dependency only.
- Run: `cargo test --release -p mc-graphics-kernel`
