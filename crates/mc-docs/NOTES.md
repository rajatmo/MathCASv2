# mc-docs: the documentation window and the function pages (round 9)

Specification: `docs/design/09-docs.md` (also the crate's module documentation). A library plus two programs:
`mc-docs` (the window, eframe/egui 0.37) and `mc-docs-pages` (`check | update [name ...] [--dir <folder>]`, with
`[k/N %]` progress).

## Modules

| module | contents |
|---|---|
| `page` | reads a page into usage lines, Details items, example groups and See also names; reports every structural problem with its line |
| `inline` | code spans in backticks, the only inline markup |
| `catalogue` | every function with its one-line description, by area and topic, from the libraries' `FUNCTIONS` lists; `LANGUAGE` (the evaluator's own callable natives, described here), `INTERNAL` (lowering targets, not documented) |
| `search` | ranking: exact name, name prefix, a word of the name starting with the query, name contains it, text contains every word; ties shorter then alphabetical |
| `library` | the pages: embedded at build time (`build.rs` embeds `docs/functions/**/*.mcn|svgz|png`) or the folder `MCDOCS_DIR`; copies a page for "Open as notebook" |
| `runner` | `run`, `update`, `check` |
| `view`, `app` | the window |
| `selftest` | `MCDOCS_SELFTEST=<script>`: `out, size, theme, go, search, find, next, expand, scroll, back, forward, wait, shot, log, quit` |

The window: home page with five area tiles (documented functions as links, topics, "All N functions" to the guide);
guide pages by topic with descriptions; incremental search (arrows, Enter, Esc); function pages (usage panel in the
code font coloured by `mc_syntax::colour`, Details collapsed, example groups collapsible with Basic examples open,
inputs at the group's content edge in the bold code font, outputs indented behind a thin rule (round 11: no
input and output number labels, as in the notebook), print and message lines in their colours, PNG images, See also links); a
function without a page shows its catalogue description; back/forward/home (buttons, Alt+Left/Right/Home, mouse
buttons 4 and 5); `Ctrl+F` find in page (Enter/F3, Shift for previous; opens collapsed sections); F1 opens the name
typed in the search box; `mc-docs <name|guide:area|search:text>`; theme cycles system/light/dark starting from the
notebook's saved theme; selectable text; "Open as notebook" copies the page and its images to
`%TEMP%/mathcas-docs/<name>-<pid>/` and starts `mc-notebook` (beside the program, or `MATHCAS_NOTEBOOK`).

## Decisions (DOC)

1. Page format as in `09-docs.md`.
2. Pages are embedded at build time (self-contained program); `MCDOCS_DIR` overrides for page authors; a test checks
   the embedded copy equals the repository copy.
3. Descriptions come from the crate's own list of the library crates' `FUNCTIONS` consts (`mc-engine`'s list is
   private). A test fails when the engine has a native that is neither catalogued nor `INTERNAL`: **a crate added to
   the engine must also be added to `catalogue::sources()`.**
4. Areas per library: core language (lists, associations, strings, session, language), data (io, random, dist),
   visualisation (plot), symbolic and numeric (calculus, poly, simplify, solve, numeric, roots, quad, ode, jit), higher
   mathematics (numtheory, special, linalg).
5. Each page runs in a fresh engine, all inputs in one session; an error does not stop the page (unlike
   `mc-repl --update`); each input is stopped after 30 s (watchdog plus stop flag); messages name the page as
   `<name>.mcn`, so no absolute paths reach outputs.
6. A plot is stored as its vector drawing, gzip-compressed without a time stamp, `<name>.files/<name>-<k>.svgz`
   (k = the input's number, stable names; no size limit); an image that is not a drawing (read back from a file) is a
   `.png`; pictures no output names are removed (a stale PNG of a plot goes); the output line is `image("<name>.files/...")`, relative to the page (the notebook's sidecar
   folder idea).
7. The checker compares text lines exactly (round 10: machine reals within a relative 1e-9 and data fingerprints
   loosely, since each system's mathematics library rounds last bits its own way; `compare`) and images by existence
   and pixel size; in release builds an input over 1 s is a problem.
8. GPU set-up and fonts come from `mc-ui` (round 10: `mc_ui::gpu::run_native`, integrated GPU first, Direct3D 12
   then GL on Windows, `gpu.json` shared with the notebook; `mc_ui::fonts`, the bundled fonts on every system);
   theme uses `mc_notebook::theme::Palette` read-only.
9. Usage lines are templates (`f(a, ...)`): syntax-error colouring is off in the docs; colourings are cached across
   frames (each `mc_syntax::colour` call starts a thread).
10. "Simplest" in `simplify`'s page means fewest leaves; `simplify` has no options, so its page has Applications
    (with `assuming`) instead of Options.

## Pages

`table`, `plot`, `d`, `integrate`, `solve`, `simplify`, `expand`, `n_integrate`, `n_dsolve`, `find_root`: about 175
inputs, every output written by the engine, 10 plot images; every input under 0.1 s. They show unevaluated cases,
failures, generic solutions and `x /. solve(...)` staying as `rewrite(...)`.

## Round 11

- 112 new pages (177 in all): the functions the research scripts in `examples/` use, the everyday list, string,
  association and session functions, and the core functions of each guide (data, plots, symbolic, numbers, special
  functions, linear algebra). Pages show real behaviour, including the surprising cases, under "Possible issues".
- The example cells have no input and output number labels (as in the notebook): inputs start at the group's edge,
  outputs are indented behind a thin rule.
- Search ties: functions with a page first, then by how many pages' examples call the function, then shorter names.
- Guides: "k of n with a page" per topic and an "Without a page yet" list per area; function pages end with
  previous/next links in their area's guide order and a link to the guide.
- The catalogue lists the prelude's functions (`say`, `count_if`, `zip`, ...) under Language, checked against the
  engine.
- The runner runs each page with its current directory set to a fresh temporary folder (`runner::scratch`), so files
  a page writes by relative name stay out of the repository; the previous directory is restored after the page.
- `mc-docs-pages missing`: the catalogued functions without a page, by area and topic.

Decisions: DOC.11 inputs and outputs without labels, outputs marked by an indent and a 2-point rule in the border
colour (both themes). DOC.12 search tie order as above ("usage" is counted from the pages' inputs, not from user
sessions). DOC.13 prelude functions are catalogued in `mc-docs` (no library lists them). DOC.14 the page runner owns
the process's current directory while a page runs (pages run one at a time in every caller).

## Tests and measurements

11 tests: 8 unit; `tests/pages.rs` 3 (every page against a fresh run, the required pages complete, embedded equals
repository). The checker was confirmed to catch an edited output, a missing image and an unknown See also name.
Window work per frame 0.1-0.6 ms; GUI peak memory about 500 MB.

## Known gaps

- `mc-docs name` while a docs window is open starts a second window (no single instance).
- "Open as notebook" tested for the copy only.
- Plot images use system fonts, so pixels may differ between machines (sizes are what is checked).
- If `plot` stops returning `image("path")`, `page::image_path` and the display need the new form.

## Round 12: links, index, one help source

- **Links** (`links.rs`, decision NBDOCS.3): every function name in a page is a link: the usage calls, the code
  spans of usage texts, Details and example texts, the identifiers of example inputs and outputs (tokenised with
  `mc_syntax::colour`), See also, the guides and the index. A token links when it is a global name (not a local,
  pattern or keyword) the catalogue knows and is called (`name(`) or longer than one letter (single letters are
  variables far more often: `n` in `table(n^2, [n, 1, 5])`). Unknown names do not link.
- **Look** (NBDOCS.4): in prose the linked names take the accent colour; in code they keep the syntax colours. Under
  the pointer a link is underlined, the pointer is a hand and a tooltip gives the usage line (help index); a click,
  middle-click or Ctrl+click opens the page (the window has no tabs, so all three navigate; back and forward as
  before). Labels are laid out as galleys and the link rectangles are found from the glyphs.
- **Index** (`Loc::Index`, `mc-docs index`, the "Index" button and a link on the home page): every function by
  area and topic as links (accent: with a page).
- **Single instance**: `mc-docs <place>` hands the place to an open documentation window (loopback socket and token
  in `docs-instance.json`, the notebook's mechanism generalised: `instance::Server::start_as`, `instance::send_to`)
  and exits; that window shows it and comes to the front. Self-test windows do not take the role unless the script
  says `instance`. (Closes the gap "mc-docs name while a docs window is open starts a second window".)
- **Help index**: `index.rs` writes the catalogue table of `mc-help` (`mc-docs-pages index`); the test
  `the_help_index_is_current` checks it and that every page's usage lines reached the index.
- Self-test steps added: `ppp`, `move`, `click`, `ctrlclick`, `mclick`, `hoverlink <name>`, `clicklink <name>`,
  `links`, `instance`, `waitpage <name>`; the log line of each step counts the links drawn and names the one under
  the pointer.
- Tests: 16 unit tests (new: links, the help index), `tests/links.rs` 2 (every link of every page, the guides and the
  index resolves to a catalogued function: 10,153 links over 385 pages; the link rule), `tests/pages.rs` 3.

## Round 18 (phase 9E): a page for every function

- 291 new pages: every function of the catalogue now has one (1,075 pages for 1,075 functions; `mc-docs-pages
  missing` lists none). Families: 93 special functions, 49 number theory, 20 linear algebra, 15 polynomials,
  24 numbers and precision, 21 simplification functions and domains, 12 equations, calculus, ODE and compiler
  functions, 56 plot options, graphics options and directives.
- Each page has a usage block, Details, Basic examples (3 to 6 inputs) and further groups where the function has
  scope, options or relations; every output was written by `mc-docs-pages update`. Examples are our own, with
  the identities checked numerically in the examples themselves (DLMF numbers cited for definitions); random
  examples are seeded, no example depends on the machine or on timing (compiled_info shows only its stable fields).
- Behaviour that is missing or wrong was not hidden: unevaluated cases are shown under "Possible issues" and the
  Details only claim what the engine does. Wrong values found while writing (for example `hypergeometric_u` with a
  non-positive integer first parameter) were left out of the pages and recorded as bugs instead.
- Option pages for directives that the renderer accepts and does not draw (lights, shading, line caps and joins)
  say so, and show that the picture is the same without them.

## Round 19: search, copying examples, outputs as in the notebook

- **Search** (`search::Searcher`, built once): names match ignoring case and underscores (`nintegrate`,
  `NIntegrate`, `besselj`); a query of two or more letters also matches the initials of the name's words (`fr`:
  `find_root`); the one-line description ranks above the page's text (`Summary` before `Text`). When the exact kinds
  find fewer than 8 functions, near misses follow under a "Similar" heading: names within one edit (4 to 7 letters)
  or two (8 or more; a transposition is one edit), also as a prefix (`intgr`), fewest edits first; then texts with a
  word within one edit of each query word of 5 letters or more (a vocabulary of the texts' words built once).
  Measured per keystroke (`typesearch`, 1,075 functions): median 0.5-0.7 ms, at most about 1 ms (the first search
  1.8 ms). Before: `nintegrate`, `intgrate` and `besselj` found nothing.
- **Copy buttons**: every example input has a copy button at the right of its first line (faint until the pointer
  is over the input, a check mark for two seconds after copying); the input goes onto the clipboard as text, which
  pastes into a cell as it reads (Ctrl+Shift+V in the notebook pastes its powers and fractions as boxes).
- **Outputs as the notebook draws them**: values holding a root, fraction or
  power are drawn by `mc_notebook::outputs::OutputPainter` (math_text, the code font, the notebook's painter); find
  in page marks a typeset value holding a match; other values stay linear text with links. The pages' texts still
  come from the engine's undisplayed printed form (`1 + -10*x^2`, Q19-36): the page runner uses `CoreEngine::new()`,
  whose `display` setting is off, where `mc-kernel` and `mc-repl` set `display: true` (`-27 - 36*x^2`). One line in
  `runner.rs` (`CoreEngine::with_config(EngineConfig { display: true, .. })`) plus a regeneration of every page fixes
  it; left open for the pages.
- Self-test steps added: `typesearch <text>`, `copybutton <k>`, `clip`. `tests/usability_selftest.rs` (ignored: opens
  a window).

## Round 20: sections, copyable values

- **Places with an anchor**: `mc-docs name#anchor` (F1 on an option in the notebook sends `plot#plot_range`)
  opens the page at `Page::target(anchor)`: the group titled `anchor`, else the first input giving the option
  (`anchor:`; in the Options group first), else the Details item naming it, else the Options group. The group
  opens, the part scrolls into view for a few frames (groups animate open) and has an accent bar at its left
  until the page is left. Unit test `anchors_point_at_the_example_of_an_option` (on `plot.mcn`).
- **Typeset values are selectable and copyable** (gap of round 19): a click selects a value (drawn with the
  selection colour), Ctrl+C copies its printed text, which reads back as the value; a copy button at its right
  while the pointer is over it or it is selected, and a context menu with Copy. Dragging a text selection across
  a page still does not take typeset values into it.
- Self-test steps added: `place <name#anchor>`, `clickvalue <k>`, `copy`.
- Pages: `rasterize` documents its properties; `mul` explains that a name before a bracket is a call
  (`x(x - a)`), and products need `*`.

## Round 21: topic guides

- **Guides** (`docs/guides/<NN>-<slug>.mcn`, module `guide`): eight short notebooks, one per topic (getting started,
  solving equations and inequalities, calculus and transforms, simplification and assumptions, differential
  equations, precision and numerics, plotting and graphics, lists and data). Same `.mcn` format as the function
  pages but a free body: a title, an intro text, sections and subsections, `text` and `item` cells, input cells whose
  outputs the engine wrote. Required: a section "Common mistakes", a last section "Related functions" holding one
  text cell of code spans (each must be a catalogued function; they are drawn as links), at least 6 inputs. Images in
  `<stem>.files/`, as for pages. The number prefix orders the guides; the window names a guide by its slug.
- **Window**: `Loc::Topic(slug)` (`mc-docs topic:<slug>`, key `topic:<slug>`); the home page lists the guides under
  the index link (title as a link, the intro as description), so a guide is one click from the start page; a guide
  has "Open as notebook" (copies `guides/<stem>.mcn` and its images) and find in page. Guides are embedded at build
  time under `guides/` of the library (`build.rs`); `MCDOCS_GUIDES` (else `guides` beside `MCDOCS_DIR`) overrides.
  The existing area "guide" view (every function by topic) is unchanged.
- **Outputs are never typed**: `mc-docs-pages update --guides [stem ...]` runs a guide with the page runner
  (`runner::run`, fresh engine, display mode) and writes outputs and images; `check --guides` is `runner::check_guide`.
  `tests/guides.rs`: the eight topics exist in order, structure, every guide matches a fresh run (Windows), embedded
  equals repository, no local paths. `tests/guides_selftest.rs` (ignored, opens a window): home lists the guides, a
  click opens one, both themes.
- **Quickstart** (`docs/guide/quickstart.tex`, PDF rebuilt): all 100 REPL inputs were rerun on this build and compared
  with the stored outputs; changed: complex numbers now print `a + bim` (not `complex(a, b)`), `n(exp(100), 20)` has
  22 digits, a few last digits of machine reals (`n_integrate`, the failure estimate), the compile time, and
  `integrate(exp(-x^2), [x, 0, 1])` now has a closed form (the unevaluated example is `x^x`). `working_precision:` is
  the documented name of `digits:` (both work); a final section points to the guides.
- Gaps: guides are not searched (search covers functions only); the guide prose is not run through the link-resolution
  test beyond "Related functions"; outputs of guides are checked on Windows only (as the pages).
- Engine findings while writing: `plot` accepts unknown options silently (`plot_label:`, `axes_label:`, `plot_style:`
  all return an image); `expand((x + 1)^4)` prints terms as `1 + 4*x + 4*x^3 + 6*x^2 + x^4` (canonical order mixes the
  degrees); `table(k^2, k, 1, 3)` gives `[[1, 1, 1]]` (two loops over one name, the outer one wins); `n_sum(1/k^2,
  [k, 1, inf], working_precision: 30)` takes about 2.5 s (20 digits 1.9 s).

## Round 21

- `tests/bugs_round21.rs`: seven ignored failing tests that run one input through `runner::run` ((the claims of
  the pages the engine did not meet: B21-1 to B21-6, B21-14); the crates of the fixes are named in each reason.
- `tests/usability_selftest.rs`: the `numericaly` assertion tested `contains("0 results")`, which "170 results"
  matches; it now tests for exactly `0 results` (B21-13). Window self-tests run in this round: warnings, typeset,
  clipboard, help, zoom (mc-notebook) and sections, usability (mc-docs).

## Round 23: two guides, guide search, Help menu

- **Guides** `09-special-functions` (exact values and reductions, any precision, complex and huge arguments,
  hypergeometric functions, `function_expand`, `meijer_g`, which function for which job) and `10-transforms-and-series`
  (Laplace, Fourier, Mellin, z and their inverses, conditions and strips, `series`, `asymptotic`). Outputs written
  by `mc-docs-pages update --guides`; `tests/guides.rs` now expects ten topics.
- **Search covers guides** (`search::search_guides`, `Guide::search_text`): a query matches a guide when every word
  is in its title (first) or in its title, headings, prose, example inputs or related names (underscores and case
  ignored, exact substrings, no edit distance). Matching guides are listed under a "Guides" heading above the
  functions (title as a link, the intro as description); Up/Down and Enter walk guides first, then functions
  (`selected` counts guides first). The self-test log line of a search ends with `guides [slug, ...]`.
- **Notebook Help menu**: "Guides" opens the documentation window at its start page (`mc-docs home`), which lists
  the guides.
- Gaps: guide search has no fuzzy matching (functions do); a guide result does not say which part matched.
- Engine findings while writing (guide examples avoid them): `n(hypergeometric_pfq([1, 1, 1], [2, 2], -1), 30)` returns
  the call with float arguments instead of a number (also with `-1.0`); `hypergeometric_2f1(a, b, b, x)`,
  `hypergeometric_2f1(1, 1, 2, -x)` (log(1 + x)/x) and `hypergeometric_0f1(1/2, -x^2/4)` (cos) do not reduce;
  `laplace_transform(exp(t^2), t, s)` returns an erfc expression though the integral diverges for every `s`;
  `mellin_transform(1/(1 + x), x, s, generate_conditions: true)` returns no condition (exp(-x) does);
  `asymptotic(bessel_j(0, x), x -> inf)` and `series(zeta(x), x, 1, 2)` stay unevaluated; `normal(series(cos(x), x,
  0, 6))` and `laguerre_l(3, x)` print terms in mixed degree order; `hurwitz_zeta(2, 1/3)` stays unevaluated.
- The Help menu entry has no self-test step (the notebook's self-test cannot click a menu); `help_state` logs the
  place asked for as before.

## Round 26: two guides, lists and getting-started updates

- **Guides** `11-function-properties` (domain and range, continuity, singularities, poles, sign, monotonicity,
  convexity, periods, injective/surjective/bijective, parameters and assumptions, `piecewise_expand`) and
  `12-finite-fields-and-number-theory` (primes and residues, `finite_field`, elements, roots, orders, discrete logs,
  trace/norm/minimal polynomial/Frobenius, polynomials over a field, embeddings). `tests/guides.rs` expects twelve
  topics. `01-getting-started` gained `table` iterator lists and `threaded`; `08-lists-and-data` a section on iterator
  lists and one on `elementwise`/`threaded`, and its stale "Common mistakes" item on `table(k^2, k, 1, 8)` now says what
  the engine does (a second loop over `k`, with a warning). Quickstart text and PDF name twelve guides.
- Engine findings while writing (guide examples avoid them): WRONG VALUE `function_range(exp(x), x, y)` gives `y >= 0`
  (exp never takes 0: `y > 0`); `function_period(x + sin(x), x)` and `function_surjective([exp(x), true, y > 0], x, y)`
  stay unevaluated (the page says polynomial parts give 0); `function_bijective(a*x, x, complexes, assumptions: a > 0)`
  keeps the condition `a != 0`; `function_continuous(1/(x - a), x, assumptions: a > 0)` and `function_poles(gamma(x), x)`
  stay unevaluated; `finite_field(3, 2)(-1)` (negative index) stays unevaluated; `expand(factor(p, extension: F)) == p`
  does not reduce to `true` (element coefficients against integers); `factor(p, modulus: q)` is an argument failure;
  `multiplicative_order` takes integers only (an element of a field stays unevaluated).

## Round 35: pictures are drawings only

- No PNG for a plot anywhere (user decision 2026-11-11). `runner::update` writes `<name>-<k>.svgz` (gzip of the SVG
  the plot wrote; `mtime` 0, best compression, so the same drawing is the same bytes) and removes the stale PNG;
  `check` compares the drawing's size (`mc_nbfile::images::svg_size` of the decompressed text) as it compared PNG
  sizes. The page's output line names the `.svgz`.
- Images that are not drawings stay PNG (`-<k>.png`); `mc-docs-pages update` prints how many per page.
- `build.rs` embeds the files as they are (the header-only PNG stubs of round 34 are gone); `Library::copy_page`
  copies bytes as they are, and `mc-notebook` reads `.svgz` itself (decompress, theme, natural size), so "Open as
  notebook" needs no conversion. `view::Images` rasterises the drawing at the display's pixels in the window's theme
  and takes the natural size from the drawing.
- The page and guide checkers accept `.svgz` and `.png` picture lines directly in `<name>.files/`.
