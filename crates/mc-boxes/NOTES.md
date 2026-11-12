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
- Typeset output partial (no `sin x` without parentheses); no 4.9 large-output elision; no Copy as LaTeX; no
  typed-space collapsing at soft breaks; layout caching is the app's per-cell cache.
- Design guesses not yet tried by users (each one constant or match arm): Backspace/Delete structure
  removal, Ctrl+. growth, grid-cell caret placement, the 40% break fill.

## Round 11

- **Selections keep their anchor** (NB6.6): the `Editor` anchor is a caret position (path and offset) of its own;
  the selection is the item range of the deepest row holding both ends, a box counting whole when an end is inside
  it. Extending out of a slot selects the box; extending back into the box that holds the anchor re-enters it at the
  anchor's slot. Commands acting on a selection first bring it into one row (`settle`). `extend_to(path, offset)`
  (Shift+click, drags), `anchor()`.
- **Grid keys** (NB6.7): `Ctrl+,` / `Ctrl+Enter` in a grid cell whose lone placeholder is selected (as right after the
  previous column or row was added) add another column / row instead of nesting a grid.
- **Canonical text map**: `to_text_mapped(row) -> (String, TextMap)`, the byte offset of every item of every row
  (`offset`, `caret_at`, `find`); property-tested against `to_text` and the items' characters.
- **Typeset output** (`typeset_output_with(text, OutputOptions)`, `Typeset::map: SourceMap`): every item maps to the
  printed text it stands for (a substituted symbol to the text it replaces, a box to its expression, the thin space
  of a product to the `*` between the factors); `SourceMap::{row, span, contiguous, caret_at, byte_at, find}`;
  `output_input_text(row)` for parts whose text is not one stretch.
- **Rational coefficients** (NB6.2): in output, a product with a rational literal factor (the kernel prints
  `(2/3)*2^(1/2)`) is one fraction, the other factors of the numerator next to it (2√2 over 3); a coefficient
  numerator of 1 is dropped (`(1/2)*x` is x/2); a negative coefficient draws a minus before the fraction. Input
  conversion is unchanged.
- **Matrices** (NB6.3, `OutputOptions::matrices`, default on): a list of 2 to `MATRIX_MAX` (64) lists of equal length
  (1 to 64), none of whose elements is a list, is a grid between the list's own (stretched) brackets; a three-level
  array is a list of matrices. Rationale for on: in typeset output a list of equal-length lists is nearly always a
  matrix and reads as one; larger lists stay linear (data, not matrices); the per-output linear form and the
  preference turn it off.
- **LaTeX** (`latex::to_latex`, M7): math-mode LaTeX of a box tree: `\frac`, `{b}^{e}`, `{b}_{i}`, `\sqrt[n]{}`,
  `\overset`/`\underset`, `matrix`/`bmatrix`/`pmatrix`, `\sum_{k=a}^{b}`, `\int_{a}^{b} f \, dx`, `\lim_{x \to a}`,
  `\frac{\partial}{\partial x}`; Greek letters, arrows, relations, `\times`, `\infty`; known function names as
  commands (`\sin`), other called names `\operatorname{}`, other words `\text{}`, a letter with digits as a
  subscript (`x1` is `x_{1}`); strings `\text{}`; `*` and the thin space become a space.
- Tests: 21 unit (new: rational coefficients, matrices and their limits, source maps, LaTeX of boxes, characters
  and outputs), editing 15 (new: grid keys keep growing, extension keeps the anchor), canonical properties 4 (new:
  the text map places every item), layout snapshots (the matrix and a coefficient output re-blessed).
- **Sums and products read as written mathematics** (NB6.12): in output a term with a negative
  coefficient is a subtraction (`a + -2*b` is a − 2b; `-1*cos(x)*x^2` is −x² cos(x), never a visible coefficient 1
  or `+ −`); the factors of a product are ordered numbers, then names and their powers, then the rest (x² cos(x));
  negative integer powers are fractions (`x^-2` is 1/x², `a*x^-1` is a/x); a sum that is a polynomial in its main
  variable (the name in the most terms, with a power of degree 2 or more or a negative power somewhere) is ordered
  by descending degree (`integrate(x^2*sin(x), x)` reads −x² cos(x) + 2x sin(x) + 2cos(x)); linear sums keep the
  engine's order (`3 − x`). The source map follows: reordered parts copy through `output_input_text`.

## Round nb-ui

- `Metrics` gains default methods (the defaults keep every existing layout, the pictures of `mc-graphics` included):
  `role(c)` (which font draws `c`; default `role_of`), `axis_height()` (the painter's math axis in ems, `None`: the
  MATH constant), `line_pitch()` (baseline distance of plain lines; lines no taller than the strut are exactly that
  far apart), `italic_letters()` (traditional form's math italics; false keeps the text font's letters).
- `Draw::Char` carries its `advance`; `Line` its `right` end; `Layout::align_right(width)` right-justifies every line
  (items, caret stops, `indent`/`right` follow; a line's items are found by their y within the line's extent),
  `Layout::translate(dx, dy)`, `Layout::line_at(y)`.
- `Editor::move_home` / `move_end` stop at line breaks in the row (the line's start or its end before the break).
- Tests: layout 7 (new: painter metrics choose fonts, axis and line pitch; right-justified lines keep hit-testing).

## One kind of cell (round nb-ui, NBUI.5 and NBUI.6)

- `math::math_text(printed) -> Typeset`: printed text with only square roots, fractions and powers as boxes, every
  other character as printed, grouped by the parse (`a/b*c`, `a/b/c`, `2^3^4`, `-x^2`, `e^(i*pi)`, `x^-1`, rational
  literals), slot parentheses hidden except a power's base; a `SourceMap` from every item to its bytes.
  `math::restrict(row)` keeps the three boxes and writes every other as its linear text; `has_boxes`.
- `Structure` has three variants (Fraction, Superscript, Radical); subscripts, over/underscripts, grids and
  `switch_script` are gone from the editor. Backspace at the start of a slot (Delete at the end) unwraps the box into
  linear text (`unwrap_box`: radicand alone, `num/den`, `base^exp` with parentheses where needed) instead of stepping
  out of it. `aliases::lookup_cell` offers only the templates of the three boxes. `typeset_any` removed.
- The full output typesetting (`typeset_output`) stays for plot labels (`mc-graphics`) and Copy as LaTeX.
- Tests: unit 26 (new: grouping, verbatim text, source map, restriction), editing (the structure table for the three
  keys, unwrapping), fuzz with the three keys.
- NBUI.7 (renderer of outputs): `math_text` draws `^(1/2)` as a root and `^(-1/2)` as one over it, puts a product's
  negative integer powers below a fraction bar (`ratio`: the other factors above in order, `1` when none; a moved
  factor's `*` stands for its own byte), and turns `*` into a thin space before a name or a box (`thin_products`).
  Slots are built from pieces (text ranges, added characters with the span they stand for, nested boxes).
  `linear_segments(canonical)` gives a cell's linear text with the canonical range of each top-level item (Find).

## Round 13: labeled and legended values

- New box `Node::Labeled { value, label, side: Side::{Below, Above, Left, Right}, swatches: Vec<(usize, [u8; 3])>,
  text }` (slots: value 0, label 1), made only by `math_text`: an output that is `labeled(x, l)`,
  `labeled(x, l, side)` (`top`/`above`, `bottom`/`below`, `left`/`before`, `right`/`after`, alone or in a list of
  one) or `legended(x, l)` (a label, a list of labels, `line_legend` / `swatch_legend` / `point_legend([colours],
  [labels])`), or a list with such calls among its elements (gap G80). The label row holds the label's characters (a
  string without quotes, escapes `\"`, `\`, `\n`, `\t` read; a raw string or TeX between `$` as written; a short
  term without calls or lists as printed); a legend's entries one per line, each after its swatch characters
  (`━━`, `■`, `●`) when its colour reads (named colours as the graphics crate has them, `rgb_color`, `hue`,
  `gray_level`, `"#rrggbb"` with number components). Every other form, a picture value, a value over 20,000 bytes
  or a call that is not the output or a top-level list element stays text.
- The source map: the box stands for the whole call, the value slot's items for the value's bytes, each label
  character for its bytes inside the quotes (an escape for its two), a swatch for its colour's text, a line break of
  a legend for the separator between entries.
- Linear text, canonical text (no box marks: it reads back as the call), `restrict` and `output_input_boxes` give
  the printed call; LaTeX `\underset{\text{l}}{x}` / `\overset`, `\text{l}\quad x`, `x\quad\text{l}`.
- Layout (`Engine::labeled`): the label at text size (not script size) without wrapping, below or above the value
  centred (gap 0.3 em), before or after it on the value's baseline (gap 1 em); a label of several lines beside the
  value (a legend) is centred on the value's middle; the value keeps its baseline and wraps in the width the label
  leaves (at least 40 %).
- `label_rows(&Row) -> Vec<LabelRow { path, swatches }>`: the label rows of a tree, for a painter's colours.
- labels are drawn at the size of the text, as the graphics crate's layout of the same terms does
  (`mc_graphics::compose::shown_layout`): a label is read as text, and a script-size caption in the code font is
  hard to read; the colour (the painter's) sets it apart.
- swatches are characters of the label row (`━`, `■`, `●` are in the bundled mono font), not rules: they
  take part in the source map, selection and copying like any character, and the painter colours them through the
  per-row colour table it already has for typeset input; `Draw` is unchanged (other crates match on it).
- Tests: unit `labeled_values_draw_their_labels`, `unknown_label_forms_stay_as_printed`,
  `legend_swatches_have_their_colours`, `a_labeled_value_maps_to_its_bytes`, `a_labeled_value_reads_back_as_printed`,
  `labels_are_placed_around_the_value`.

## Tall brackets drawn as strokes (2026-11-08)

Brackets grown around tall content (fractions, grids, nested powers) were the math font's glyph assembly (STIX Two
Math) in outputs and the text font's bracket scaled vertically in inputs. Next to the sans-serif cell fonts the
assembly looked heavy (thick stems, blob-like feet on square brackets, tapered parentheses) and the scaled glyph
distorted its strokes. They are now `Draw::Delim`: a box plus a stroke width, which painters draw from
`delimiter_paths` (`( ) [ ] { } | ‖ ⟨ ⟩ ⌈ ⌉ ⌊ ⌋`).

- One stroke weight, 1.25 times the fraction rule (the weight of the text's own brackets); square brackets with short
  arms; parentheses as one cubic whose controls sit at a height tied to the width, so tall ones curve near their ends
  and run straight through the middle; braces with a centre cusp. Closing brackets mirror opening ones.
- The width is the text bracket's advance (inputs) or the math font's grown bracket's (outputs), so spacing does not
  change; the height is the target the layout already computed.
- `LayoutOptions::vector_delimiters` (on by default): plot labels turn it off, since their painters draw only glyphs
  and rules, and keep the math font's assembly.
- Painters: the notebook strokes them with egui, PDF export with a stroke operation; brackets keep their row and item
  index, so inputs colour them as before.

## Round 19: n-th roots, the editing matrix

- **n-th roots** (U19.3): `Structure::Root`, the notebook's `Ctrl+5` / `Ctrl+%`: in a root it moves between the
  index and the radicand (a square root gets an empty index first), elsewhere it makes a root of the selection with
  the caret in a new index. The canonical text is `((surd(x, n)))`, the real n-th root (`surd(-8, 3)` is -2, as the
  radical sign reads; `root(p, k)` is a root of a polynomial in the language, so `((root(x, n)))` was never the
  right term; it is still read). Linear text `surd(x, n)`; `typeset_input` reads `surd(x, n)` as the root and no
  longer `root(...)`; `restrict` keeps indexed roots. Backspace at the radicand's start steps to the index's end and
  Delete at the index's end to the radicand's start (nothing deleted); an empty index goes (a square root stays);
  at the outer edges a filled root becomes its linear text `surd(x, n)` (as a fraction or power unwraps), so no
  character typed is lost and the term is kept.
- **Matrix** (`tests/matrix.rs`): power, fraction, square root, n-th root and six nestings (power of a sum,
  fraction of a power over a root, root in an exponent, power of a root, n-th root of a fraction, fraction in a
  fraction) against: entering with the keys (canonical text, read back, linear text, same term); walking Right
  through every caret position and Left back over the same ones; Home/End in every slot; selections (Shift+arrows
  over everything, every slot alone, leaving a slot selects the box, Ctrl+. to the whole); Backspace/Delete into a
  box from outside (nothing deleted); Backspace/Delete at every slot edge (the term kept for fractions and powers, no
  typed character lost); erasing everything; six operators typed after the box (a box is one operand); copy and
  paste into text and into a numerator, cut and paste back; box -> linear text -> `typeset_input` giving the
  identical tree. Terms are compared as read by `mc_syntax::read`. Before this round 90 of 100 (the n-th root rows:
  no key, `root` read as a polynomial root, the index lost at slot edges), now 100 of 100. `tests/fuzz.rs` uses the
  four box keys.
- `math.rs`: the coefficient-parentheses helper moved before the test module (clippy).

## Round 20: calls and products told apart in output

A call that reads like a product (a plain name that is also an operand of its own arguments, `x(x - a)`) keeps
its brackets as a power's base, `(x(x − a))²` (the printer writes them too); in an output product a name before a
bracketed factor is joined by a centred dot, `x⋅(x − a)`, so that the product and the call `x(x − a)` look
different (also under a radical: `√(x⋅(x − a))` against `√(x(x − a))`). `f(x)²` is unchanged. Unit tests in
`printed.rs`; the editing matrix stays at 100 of 100 (no construct of it changed).

## Round 21

- `tests/matrix.rs` has 7 more constructs: n-th root with a root index, power in a power, fraction in an exponent,
  power of an n-th root, fraction of fractions, root of a root of a root, n-th root of a power over a root (17
  constructs x 10 operations, all pass). `(x^2)^3` typed as boxes does not survive the linear-text trip as the same
  structure (the read-back base is a parenthesised box, decision 3: same term), so it is not a construct.
- `tests/bugs_round21.rs`: one ignored failing test, B21-10 (a box key after an operator takes the operator as its
  operand), kept ignored until fixed.

## Round 22

- B21-10: a box key after an operator takes no operand from it: the box gets a placeholder operand (`a+□^2`).
  `x+` with Ctrl+6 was `x*(+^Q)` before, now `x+(Q^□)`; the structure table of `tests/editing.rs` says so.

## Round 32

- A box key on a selection leaves spaces at the selection's edges outside the box (a selection of spaces only is
  taken whole). Test: `editing.rs` `selection_becomes_the_first_slot`.
- Linear text: a number literal directly before a power or call box whose text starts with a name or `(` is written
  without parentheses (`2x^2`, `2sqrt(x)`, `2(1 + x)^2`; still `2(3^2)`, `x2(x^2)`, `2 (x^2)`): juxtaposition has
  the precedence of `*`, so the terms are the same.
- `aliases::alias_of(result)`: the shortest alias that inserts a character (the notebook's Insert menu hints).

## Round 34: geometry for the editor of design 11

- Caret stops are one text line tall at their row's scale (`Stop::ascent`, `Stop::descent`; design 11, 3.1), not the
  extent of the line around them. Every item has an `ItemBox` (its ink extent, at least a text line of its row's
  size) in `RowGeom::ink`; `Layout::item_box`, `Layout::selection_boxes`; `Layout::hit` finds the nearest item box
  (the deepest row on ties, then the one centred nearest), then the nearest stop of that row.
- `layout_by_lines` with a `LineCache`: each logical line laid out on its own at the bracket depth it starts at
  (`Engine::starting_at_depth`), stacked with the engine's own line skip (`engine::line_skip`); equal to `layout`
  (test `line_by_line_layout_is_the_whole_layout`) except that a bracket spanning lines is not grown for tall
  content on another line.
- `ops.rs` holds the operand rules both editors share (`primary_before`, `word_start`, `matching_open`,
  `compound_base`, `closer_of`); `write_notation` and `Pos` are public for the new editor's notation;
  `TextMap::row_offsets`.
- `edit.rs` (the editor of rounds 9 to 33) is deleted; its tests moved to `mc-edit` with the new editor.

## Round 37: layouts changed in place

- `layout_by_lines_shared` returns the composed layout as an `Arc` the `LineCache` keeps with the place of every
  logical line (`Composed`, `Part`, `Spans`). The next call matches the lines shared at the start and the end
  (items and starting depth), lays out or fetches the lines between (the replaced lines' own layouts first, then
  the hash cache), splices their pieces into the layout (taken back with `Arc::try_unwrap`: no copy when the caller
  let go of it) and shifts the root item numbers, row numbers, row paths and line ranges of the lines after them.
  When the first of those lines would move (its offset differs bit for bit) they are composed again from their own
  layouts, so the result is always the composition from the start. `layout_by_lines` keeps its signature (a copy).
- Test `layouts_changed_in_place_are_layouts_composed_from_the_start`: 400 random edits (typing, deleting, line
  breaks, brackets that change the depth of the lines after, boxes, multi-line pastes, no change) at two option sets,
  exact equality with a fresh composition after each; a layout still held is copied, never changed under its holder.
  Seeds by `MC_BOXES_SEED` (6 seeds run).
- Timing (`mc-edit/tests/timing.rs`, `MC_LINES=2000`, loaded machine): composing by lines 2.2 to 3.3 ms a keystroke,
  in place 0.33 ms typing at the end, 0.82 ms at the top.
