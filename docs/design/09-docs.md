# MathCAS v2: documentation pages and the documentation window

The specification of the function pages in `docs/functions/` and of the `mc-docs` window (crate `crates/mc-docs`;
the same text is the crate's module documentation). Names in square brackets are items of that crate.

## The page format

A function's page is the notebook `docs/functions/<name>.mcn` in the `.mcn` format of `mc-nbfile`, where
`<name>` is the function's name exactly as it is called (`n_integrate.mcn`). Images that outputs show live in the
folder `docs/functions/<name>.files/` beside it. The cells come in this order:

1. **Title**: one `title` cell whose text is the name.
2. **Usage**: one or more `text` cells, one per signature. Each starts with the call as a code span in backticks,
   the name followed by `(` (or the bare name for a constant), then what the call gives, as a sentence:
   `` `n_integrate(f, [x, a, b])` gives a numerical approximation to the integral of f from a to b. ``
3. **Details**: a `section` cell "Details", then one or more `item` (or `subitem`) cells, one fact each.
4. **Examples**: a `section` cell "Examples", then `subsection` groups. The first is "Basic examples"; any of
   "Scope", "Options", "Applications", "Properties and relations", "Possible issues" may follow, each at most once
   and in this order. A group holds short `text` cells and input cells (`cell`), at least one input. Inputs run in
   order in one session per page, so a group may use what an earlier input defined.
5. **See also**: a `section` cell "See also", then one `text` cell listing related functions as code spans
   separated by commas: `` `d`, `integrate`, `n_integrate` ``. Nothing follows it.

Prose cells (usage, items, texts) use one inline markup: a code span between backticks, shown in the code font
and coloured as source; everything else is plain text. A backtick that opens a span must close it in the same
cell.

**Outputs** are the output blocks of the input cells, exactly as a run of the page writes them: `#=` lines of a
value, `#|` lines of `print` output, `#!` lines of failures and errors, `#?` lines of messages. They are never
typed by hand: `mc-docs-pages update [name ...]` runs the pages through the engine and writes them (see
[`runner`]). An image value is the line `#= image("<name>.files/<file>.png")`, a path relative to the page's
folder: the update copies each image a plot made into the page's `.files` folder (named
`<name>-<input number>.png`, so the names are stable) and removes images no output names any more. Examples are
deterministic (random examples set a seed) and each runs well under a second.

## What the checker validates

[`page::Page::read`] checks the structure above and reports each problem with its line: the title, the usage
cells (a leading code span calling the page's function, followed by text), the sections and their order, the
example group names and order, at least one input per group, cell kinds allowed in each part, balanced code
spans, the See also cell's form, and image lines (relative paths inside the page's own `.files` folder). The
test `tests/pages.rs` (and `mc-docs-pages check`) additionally checks every page against the catalogue (the page
documents a known function, See also names known functions), runs every input through the engine in a fresh
session per page and compares the outputs with the stored ones: text lines exactly, images by the file's
existence and its pixel size; and in release builds that no input takes more than [`runner::SLOW_SECS`].

## The window

[`app::DocsApp`]: a home page with the five areas of the system as guide tiles, guide pages listing every
function of an area by topic with its one-line description, incremental search (name prefixes first, then
words of the usage and descriptions), function pages drawn from the `.mcn` files (usage block in the code font
with syntax colouring, collapsible Details and example groups with `In[n]:=`/`Out[n]=` labels and images, See

## Writing and checking pages

- Write the structure and the inputs, then `mc-docs-pages update <name>` writes the outputs and images;
  `mc-docs-pages check` (or `cargo test -p mc-docs`) fails on any drift.
- Pages made with `mc-repl --update` do not pass: their images have absolute temporary paths and the run stops at
  the first error. Use `mc-docs-pages update`.
- A new library crate registered in the engine must be added to `catalogue::sources()` in `mc-docs` (a test checks
  every engine native is catalogued or listed as internal).
