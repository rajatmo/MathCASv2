//! MathCAS v2 documentation: the function page format, the catalogue of every library function, the checker that
//! runs the pages' examples, and the documentation window (`mc-docs`).
//!
//! # The page format
//!
//! A function's page is the notebook `docs/functions/<name>.mcn` in the `.mcn` format of `mc-nbfile`, where
//! `<name>` is the function's name exactly as it is called (`n_integrate.mcn`). Images that outputs show live in the
//! folder `docs/functions/<name>.files/` beside it. The cells come in this order:
//!
//! 1. **Title**: one `title` cell whose text is the name.
//! 2. **Usage**: one or more `text` cells, one per signature. Each starts with the call as a code span in backticks,
//!    the name followed by `(` (or the bare name for a constant), then what the call gives, as a sentence:
//!    `` `n_integrate(f, [x, a, b])` gives a numerical approximation to the integral of f from a to b. ``
//! 3. **Details**: a `section` cell "Details", then one or more `item` (or `subitem`) cells, one fact each.
//! 4. **Examples**: a `section` cell "Examples", then `subsection` groups. The first is "Basic examples"; any of
//!    "Scope", "Options", "Applications", "Properties and relations", "Possible issues" may follow, each at most once
//!    and in this order. A group holds short `text` cells and input cells (`cell`), at least one input. Inputs run in
//!    order in one session per page, so a group may use what an earlier input defined.
//! 5. **See also**: a `section` cell "See also", then one `text` cell listing related functions as code spans
//!    separated by commas: `` `d`, `integrate`, `n_integrate` ``. Nothing follows it.
//!
//! Prose cells (usage, items, texts) use one inline markup: a code span between backticks, shown in the code font
//! and coloured as source; everything else is plain text. A backtick that opens a span must close it in the same
//! cell.
//!
//! **Outputs** are the output blocks of the input cells, exactly as a run of the page writes them: `#=` lines of a
//! value, `#|` lines of `print` output, `#!` lines of failures and errors, `#?` lines of messages. They are never
//! typed by hand: `mc-docs-pages update [name ...]` runs the pages through the engine and writes them (see
//! [`runner`]). An image value is the line `#= image("<name>.files/<file>.png")`, a path relative to the page's
//! folder: the update copies each image a plot made into the page's `.files` folder (named
//! `<name>-<input number>.png`, so the names are stable) and removes images no output names any more. Examples are
//! deterministic (random examples set a seed) and each runs well under a second.
//!
//! # What the checker validates
//!
//! [`page::Page::read`] checks the structure above and reports each problem with its line: the title, the usage
//! cells (a leading code span calling the page's function, followed by text), the sections and their order, the
//! example group names and order, at least one input per group, cell kinds allowed in each part, balanced code
//! spans, the See also cell's form, and image lines (relative paths inside the page's own `.files` folder). The
//! test `tests/pages.rs` (and `mc-docs-pages check`) additionally checks every page against the catalogue (the page
//! documents a known function, See also names known functions), runs every input through the engine in a fresh
//! session per page and compares the outputs with the stored ones: text lines exactly, images by the file's
//! existence and its pixel size; and in release builds that no input takes more than [`runner::SLOW_SECS`].
//! Machine reals may differ in their last digits between systems ([`compare`]).
//!
//! # The window
//!
//! [`app::DocsApp`]: a home page with the five areas of the system as guide tiles, guide pages listing every
//! function of an area by topic with its one-line description, incremental search (name prefixes first, then
//! words of the usage and descriptions), function pages drawn from the `.mcn` files (usage block in the code font
//! with syntax colouring, collapsible Details and example groups (inputs at the group's edge in the code font, outputs indented behind a thin rule) and images, See
//! also links), back/forward/home, find in page (`Ctrl+F`), One Light and One Dark, selectable text, and "Open as
//! notebook" (a copy of the page in a temporary folder, opened in `mc-notebook`). Every function name in a page,
//! the guides and the index page is a link ([`links`]); `mc-docs <name>` shows the page in an open window.
pub mod app;
pub mod catalogue;
pub mod compare;
pub mod index;
pub mod inline;
pub mod library;
pub mod links;
pub mod page;
pub mod runner;
pub mod search;
pub mod selftest;
pub mod view;
