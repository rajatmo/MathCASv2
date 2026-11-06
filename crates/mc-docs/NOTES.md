# mc-docs: the documentation window and the function pages (round 9, package docs)

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
| `library` | the pages: embedded at build time (`build.rs` embeds `docs/functions/**/*.mcn|png`) or the folder `MCDOCS_DIR`; copies a page for "Open as notebook" |
| `runner` | `run`, `update`, `check` |
| `view`, `app` | the window |
| `selftest` | `MCDOCS_SELFTEST=<script>`: `out, size, theme, go, search, find, next, expand, scroll, back, forward, wait, shot, log, quit` |

The window: home page with five area tiles (documented functions as links, topics, "All N functions" to the guide);
guide pages by topic with descriptions; incremental search (arrows, Enter, Esc); function pages (usage panel in the
code font coloured by `mc_syntax::colour`, Details collapsed, example groups collapsible with Basic examples open,
`In[n]:=`/`Out[n]=` numbered per group, print and message lines in their colours, PNG images, See also links); a
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
6. Plot images are copied to `<name>.files/<name>-<k>.png` (k = the input's number, stable names); images no output
   names are removed; the output line is `image("<name>.files/...")`, relative to the page (the notebook's sidecar
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

## Tests and measurements

11 tests: 8 unit; `tests/pages.rs` 3 (every page against a fresh run, the required pages complete, embedded equals
repository). The checker was confirmed to catch an edited output, a missing image and an unknown See also name.
Window work per frame 0.1-0.6 ms; GUI peak memory about 500 MB.

## Known gaps

- `mc-docs name` while a docs window is open starts a second window (no single instance).
- The theme choice is not saved.
- "Open as notebook" tested for the copy only.
- Plot images use system fonts, so pixels may differ between machines (sizes are what is checked).
- If `plot` stops returning `image("path")`, `page::image_path` and the display need the new form.
