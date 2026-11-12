# mc-libraries: notes

## Round 34

- New: the registry of the library crates. `install` is the installation sequence that was inline in
  `mc-engine/src/engine.rs` (same calls, same order); `SOURCES` the libraries written in MathCAS in the engine's load
  order (the core-language library text moved here from `mc-engine`); `FUNCTIONS` the engine's list of `FUNCTIONS` tables.
- The library crates are re-exported, so `mc-docs` (catalogue) and `mc-lsp` (library names) keep their own topic
  tables with one `use mc_libraries::{...}` line instead of a dependency per library. The language server's table
  still leaves out `mc-jit` and `mc-geom`, as before (no behaviour change).
- Step 5 (the calculus split): `install` calls `mc-calculus`, `mc-integrals`, `mc-dsolve`, `mc-transforms` and
  `mc-limit` where `mc-calculus` alone was installed; the calculus `FUNCTIONS` table stays one table.
- Step 7 (names by domain): the library files are named by area: `lib/calculus.mc` (the files of rounds 28, 29 and
  31 merged in this order; the round 31 rules and helper now load before the further functions, whose
  helpers they call only when they run), `lib/lists.mc`, `lib/language.mc`,
  `lib/special.mc`; `mc-cover30` is `mc-further-functions` (`lib/further.mc`), named after its
  catalogue topic: its functions span several areas, and moving them to the crates of those areas would change
  their topic in the catalogue and the help index. Helper prefixes (`calc_`, `cb_`, `cf_`, `b29_`, `c30_`, `ds_`,
  `l30_`) stay.
