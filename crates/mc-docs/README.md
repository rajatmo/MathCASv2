# mc-docs

The documentation system: the function page format, the catalogue of every library function, the checker that runs the
examples of each page, and the documentation window (`mc-docs`).

## Modules
- `page`: reads a function page (`docs/functions/<name>.mcn`) and reports structural problems with their line.
- `catalogue`: every library function with its one-line description, from the libraries' `FUNCTIONS` lists.
- `runner`: runs a page's inputs through the engine, writes the outputs and the plots' images, and checks stored output.
- `compare`: how a stored output is compared with a fresh run (machine reals may differ in their last digits).
- `library`, `guide`, `index`, `links`, `search`, `inline`: where pages come from, topic guides, the help-index table,
  links between pages, incremental search, the inline markup of prose cells.
- `app`, `view`, `selftest`: the window, drawing, and a scripted self-test mode.
- Binaries: `mc-docs` (the window) and `mc-docs-pages` (`check`, `update`, `index` from the command line).

## Design
- A page has fixed parts in a fixed order: title, usage, details, examples, see also. Outputs are never typed by hand;
  `mc-docs-pages update` writes them from a run.
- Each page runs in a fresh session, and examples are deterministic and quick; text lines are compared exactly, images
  by existence and pixel size.
- The catalogue depends on all libraries through `mc-libraries`; the small `mc-help` crate cannot, so the
  help table is generated here and a test fails when it is out of date.
- Depends on `mc-engine`, `mc-kernel`, `mc-nbfile`, `mc-notebook`, `mc-syntax`, `mc-ui`, `mc-plot`, `eframe`.

## Tests
- Unit tests in `src`; integration tests in `tests/` check every page against the catalogue and a run, the guides and
  links, and drive the window through its scripted self-test.
- Run: `cargo test --release -p mc-docs`; check pages with `cargo run --release -p mc-docs --bin mc-docs-pages -- check`.
