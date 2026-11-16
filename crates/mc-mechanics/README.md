# mc-mechanics

An analytical mechanics toolkit: Euler-Lagrange equations of point and field Lagrangians, Legendre transforms and
Hamilton's equations, Poisson brackets, Noether charges, constraints with Lagrange multipliers, small oscillations,
phase portraits and numerical integration of the equations of motion.

## Modules
- `lib.rs`: exposes the library source as `SOURCE`.
- `lib/mechanics.mc`: the library itself, written in MathCAS.

## Design
- Everything is MathCAS source, built on the calculus, solving, linear algebra, ODE and plot libraries and called
  through the evaluator; the Rust side only embeds the text.
- `SOURCE` is the file's text included at build time, so the library ships inside the binary.
- Helper functions and the pattern variables of its rules carry a library prefix, so a user's global value cannot
  shadow them.
- The crate has no Rust dependencies; the other crates appear only as dev-dependencies for tests.

## Tests
- Integration tests in `tests/` evaluate the library's functions on the assembled engine (`mc-engine`).
- Run them with `cargo test --release -p mc-mechanics`.
