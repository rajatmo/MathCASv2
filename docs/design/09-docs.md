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
[`runner`]). An image value is the line `#= image("<name>.files/<file>.svgz")`, a path relative to the page's
folder: the update writes each plot's vector drawing, gzip-compressed and without a time stamp (so the same plot
gives the same bytes), into the page's `.files` folder as `<name>-<input number>.svgz` (stable names), and an image
that is not a drawing (one read back from a file) as `.png`; it removes pictures no output names any more. A plot
has no PNG anywhere: the documentation window and a notebook made from a page draw the drawing in the theme. Examples are
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
with syntax colouring, collapsible Details and example groups (inputs on the left edge in the code font, each input's outputs indented
behind a thin rule; no `In`/`Out` labels) and images, See also links), back/forward/home, find in page, the two
themes, "Open as notebook".

**Links and the index (round 12).** Every function name in a page links to that function's page: the usage calls,
code spans in prose, the identifiers of example inputs and outputs (tokenised with `mc_syntax`), See also, the
guides and the index page (every function by area and topic). A name links when it is a global name the catalogue
knows and is called or longer than one letter; unknown names do not link. Links are underlined under the pointer
with a hand cursor and a tooltip giving the usage line; a click (also a middle-click or Ctrl+click, the window has
no tabs) opens the page, and back and forward move through the history. A test checks that every link of every page,
guide and the index resolves. `mc-docs <name>` shows the page in an open documentation window (single instance)
instead of opening a second one, which is how F1 in the notebook and the language server's hover links reach it.

**One help source (round 12).** The help the editors show comes from the crate `mc-help`: the catalogue table (name,
area, topic, the library's description; written by `mc-docs-pages index`, checked by a test) and the usage lines of
the pages (read at build time). The documentation window's tooltips, the notebook's completion, usage hint, hover and
F1, and the language server's completion, signature help and hover all read it. A usage line is the page's usage
call and text, or the catalogue description where a function has no page.

## Writing and checking pages

- Write the structure and the inputs, then `mc-docs-pages update <name>` writes the outputs and images;
  `mc-docs-pages check` (or `cargo test -p mc-docs`) fails on any drift.
- Pages made with `mc-repl --update` do not pass: their images have absolute temporary paths and the run stops at
  the first error. Use `mc-docs-pages update`.
- A new library crate registered in the engine must be added to `catalogue::sources()` in `mc-docs` (a test checks
  every engine native is catalogued or listed as internal).
