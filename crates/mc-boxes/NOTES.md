# mc-boxes: two-dimensional typesetting without UI (notebook round, P4)

Design `docs/design/08-notebook.md` section 4 and 3.9. Dependencies: `mc-syntax`, `mc-mathfont` (dev: `mc-api`).

## Modules

| module | contents |
|---|---|
| `tree` | `Row(Vec<Item>)`, `Item::{Char, Placeholder, Box(Box<Node>)}`; `Node::{Fraction, Superscript, Subscript, Radical{index}, Overscript, Underscript, Underoverscript, Grid{rows}, BigOp{op, body, var, lower, upper}, Graphic{path}}`, `BigOp::{Sum, Product, Integral, Limit, Derivative}`; paths `Step{item, slot}`/`RowPath`; `slots()` in reading order; `at`/`at_mut`/`box_of`; `notation()` (the 4.2 test notation); `MAX_NESTING = 100` |
| `canonical` | `to_text`/`from_text` for the 4.4 forms plus `((sum(f, k)))` and `((image("p")))`; never fails |
| `linear` | `linear_text(&Row)` (Ctrl+Shift+I) with minimal parentheses |
| `printed` | `typeset_output(printed) -> Typeset{row, message}` (4.5), `typeset_input(text) -> Result<Row, Vec<Diagnostic>>` (Ctrl+Shift+N), `typeset_any` |
| `layout` | `layout(&Row, &dyn Metrics, &LayoutOptions) -> Layout` |
| `edit` | `Editor` |
| `aliases` | `data/aliases.tsv` (868 entries: Greek, TeX, HTML entities, operators, constants, letter-likes, spaces, 13 templates); `lookup(name) -> Option<Alias::{Text, Template}>`; `entries()` |
| `ops` | `OPERATOR_PAIRS` (`->`, `:>`, `<=`, `>=`, `!=`, `==`, `<->`, `=>`), `pair_at`, `pair_around`, character classes |

## API for the app

- `Metrics` trait (implemented by the app over egui measurement): `char(c, FontRole) -> CharMetrics{advance, ascent,
  descent}` (ems at 1 em), `line(role)`, `graphic(path)`; `FontRole::{Text, Math}`, `role_of(c)` (ASCII/Latin-1 to
  the cell font, everything else the math font has to STIX). Provided: `MonoMetrics` (tests), `MathFontMetrics`.
- `LayoutOptions { size, display, max_width, indent_continuations, operator_spacing, traditional,
  small_inline_rationals }` (`::input(size, width)`, `::output(size, width)`).
- `Layout { width, height, baseline, items: Vec<Draw>, lines: Vec<Line>, rows: Vec<RowGeom{path, stops}> }`, size
  units, y down, characters at baselines. `Draw::{Char{c, role, size, x, y, row, index}` (`row == u32::MAX` for
  symbols the layout adds), `Placeholder`, `Glyph{glyph, size, x, y}` (math-font glyph id: paint with `rasterize` or
  `outline`), `Rule`, `Graphic}`; whitespace draws nothing. `layout.caret(path, offset) -> Option<Stop>`,
  `layout.hit(x, y) -> (RowPath, offset)`; visual Up/Down is caret then hit at y +- line height; `snapshot()` as text.
- `Editor::new(Row)` / `from_text(canonical)`; state `root`, `text`, `linear`, `caret()`, `set_caret`,
  `selection()`, `select`, `select_all`, `notation()`; commands (bool where the model needs it to cross cells, beep or
  let Tab indent): `insert_char`, `insert_text`, `paste(canonical)`, `insert_template`, `selected_text`, `cut`,
  `backspace`, `delete`, `move_left/right(extend)`, `move_home/end`, `move_up/down` (structural),
  `structure(Structure::{Fraction, Superscript, Subscript, Radical, Overscript, Underscript, GridRow, GridColumn})`,
  `exit_structure` (Ctrl+Space), `switch_script` (Ctrl+5), `next/prev_placeholder` (Tab), `expand_selection`
  (Ctrl+.), `escape() -> AliasOutcome::{Started, Replaced, Unknown, Cancelled}`, `alias_mark()` (the painter draws the
  alias marker; the layout reserves no width for it).

## Decisions (NBB)

1. Slots are never empty: an empty slot holds `Item::Placeholder` (`□`); only the root may be empty; typing into a
   lone placeholder replaces it.
2. A call-shaped slot whose text has a top-level `,` (or `->` in a limit variable, or a `[..]` list where a list means
   limits) gets one extra pair of parentheses, recursively, so the text round trip is a bijection.
3. `((A)/(B))` and `((A)^(B))` are not boxes when `(A)` or `(B)` is itself box text (a typed `(x^2 / sqrt(y))` stays a
   parenthesised quotient of boxes; a slot typed as `(sqrt(x))` reloads as a radical; both evaluate the same).
4. Linear text reads to the identical terms as canonical text (property-tested through `mc_syntax::read`).
5. Input conversion keeps powers (`x^(1/2)` stays a superscript); outputs show `x^(1/2)`, `x^(1/n)` as radicals.
6. Output substitutions per 4.5 (thin space for `*`, `x` between numbers, constants, arrows and relations, strings
   without quotes, primes, `f^(n)`); `failure(...)` gives `message = true` with the plain text.
7. Fractions keep full size at the first level, shrink inside scripts; integer/integer fractions next to other items
   at script size (`small_inline_rationals`, guess 13.22).
8. Display style: limits above/below big operators, derivative as a partial fraction; inline: limits at the side,
   derivative subscripted; integrals always side limits and end in `d` (traditional: the double-struck d).
9. Line breaking priority: lowest bracket depth, after `,`/`;`, before relations, before `+`/`-`, before `*`/`/`,
   after a space; a break fills at least 40% of the line (`MIN_FILL`); never inside a token, an operator pair or
   right after an open bracket; fraction slots wrap separately; continuation indent 2 em per bracket level for inputs,
   0 for outputs (`INDENT_PER_LEVEL`).
10. Backspace/Delete: on an adjacent box enter it (all slots empty: delete it); at a first/last slot edge step out; at
    a junction between slots dissolve the box keeping contents (grids and big operators step to the neighbour slot).
11. A radical over a selection leaves the caret at the end of the radicand; grid commands inside a grid add a row or
    column after the current one and select the new cell; Ctrl+5 moves between sub/super (creating the missing one),
    under/over, radicand/index, lower/upper limit.
12. `((image("p")))` is the canonical form of a graphic box (missing from the 4.4 table).
13. `typeset_output`/`typeset_input` parse on a scoped thread with 32 MB stack; all other recursion is bounded by
    `MAX_NESTING`.
14. Implicit multiplication (user decision): wherever an insertion (typing, paste, template, structure command)
    makes a box touch an operand, the editor inserts a visible `*` (`Editor::implicit_times`): before a box after a
    name, closing bracket, prime, string or box; after a box before a name, number, `(` or string (spaces between
    skipped). A number before a box is left alone (`2√2` already multiplies) and `[` after a box stays an index.
    Without it `x((sqrt(Q)))` would read as the call `x(sqrt(Q))`, `((sqrt(Q)))(y)` would call the root and
    `((sqrt(Q)))x` would not read. The caret stays right after what was typed.

## Tests

36 tests + 1 doctest + 1 ignored measurement: units (canonical shapes, linear parentheses, every 4.5 row, input
conversion, 3.9 aliases, alias table validity, line breaker); `tests/editing.rs` (all 48 vectors of 4.2, selection,
Tab, Ctrl+Space, Ctrl+5, grids, Backspace/Delete, operator pairs, Ctrl+., aliases and templates, paste/cut);
`tests/canonical_props.rs` (`boxes(text(b)) == b` and `text(boxes(t)) == t` 3,000 cases each, same terms 400;
release campaigns 20,000 x 5 seeds clean); `tests/layout.rs` (27 snapshots in `tests/snapshots/layout.txt`, re-bless
with `MC_BOXES_BLESS=1`; stops; hit-test round trip; wrapped lines fit); `tests/fuzz.rs` (random editing keeps slots
filled, caret valid, round trip intact; hostile output text and deep nesting never panic).

## Measurements (release)

Typeset a 25,777-character printed polynomial 5.9 ms; lay it out (182 lines) 9.5 ms; a typical input cell 16 us;
read canonical text 6.6 us.

## Known gaps

- A slot whose own text has unbalanced brackets outside strings does not reload as the same tree.
- For "edit output as typeset input" use `typeset_input(printed)` (the output form has display substitutions that
  are not valid input); copy outputs from their printed text.
- Traditional form partial (no `sin x` without parentheses); no 4.9 large-output elision; no Copy as LaTeX; no
  typed-space collapsing at soft breaks; layout caching is the app's per-cell cache.
- Not checked against the reference front end (guesses, each one constant or match arm): Backspace/Delete structure
  removal, Ctrl+. growth, grid-cell caret placement, the 40% break fill.
