# MathCAS v2: the notebook application

This document specifies the notebook window of MathCAS v2: the document model, editing, two-dimensional
typesetting, evaluation over the kernel protocol, graphics, the `.mcn` file format, themes, the technology choice and
an ordered implementation plan. It is the implementation spec: where it says *must*, tests check it.

**Fidelity rule.** The notebook part (cells, groups, brackets, labels, the insertion point, keys, typing, selection,
evaluation, typesetting) behaves exactly like the reference notebook front end that MathCAS measures itself against
(`02-benchmarks.md` section 3: reference behaviour is studied with private tooling; only conclusions enter this
repository). The window chrome (menus, toolbar, dialogs, status bar) may differ and is MathCAS's own (section 10).
Colours are not reference behaviour: they come from the Atom One Light and One Dark themes (section 8).

**How the behaviour was established.** Three sources, marked in the text:

- *(observed)*: the reference front end was driven headlessly (cells created, commands sent, the resulting document
  read back). These results are test vectors (section 11) and are reproduced literally where they matter.
- *(spec)*: read from the reference's key bindings, menus and style definitions.
- *(guess)*: neither of the above settles it; best guess, listed again in section 13 for checking against the
  reference by hand.

Unmarked statements are MathCAS design decisions.

## 1. Goals and non-goals

Goals:

1. A notebook that a user of the reference front end can work in without relearning: the same cell styles and
   grouping, the same keys, the same evaluation flow and labels, the same two-dimensional input keys.
2. Plain-text, diffable, hand-editable `.mcn` files that remain MathCAS scripts (decision 2.6 of `mc-kernel`).
3. A separate kernel process (`mc-kernel`, protocol 1): a crash or a runaway evaluation never takes the window down.
4. Responsive with 1,000-cell notebooks: typing latency under one frame (16 ms) at any notebook size.
5. Windows first, built offline from crates already in the local cargo caches; Linux and macOS later, not precluded.

Non-goals (now): dynamic/interactive content, palettes, slide shows, printing styles and page layout, chat cells,
free-form input, hyperlinks and cell tags, spelling, debugger, multiple kernels per window, collaborative editing.
They may be added later without changing the model.

## 2. Document model

A notebook is a flat, ordered list of cells. Groups are derived from the cell styles (automatic grouping) or marked by
hand (manual grouping); they are a tree over the list, not a second copy of it.

### 2.1 Cell styles

Each style has a grouping rule (2.3), a set of default attributes (2.2) and a look. Keys are `Alt+digit` *(spec)*;
styles without a key are reached through the style menu (`Alt+0` opens "Other style...", a list with type-ahead).
Margins are left/right/top/bottom in points at 100 % magnification and follow the reference *(spec)*; fonts are
MathCAS's (4.10).

| Style | Key | Grouping rule | Evaluatable | Generated | Label shown | Size | Margins (l, r, t, b) | Return key |
|---|---|---|---|---|---|---|---|---|
| Title | Alt+1 | title 0 | no | no | no | 45 | 27, -, 30, 10 | new line in the cell |
| Subtitle | Alt+2 | title 10 | no | no | no | 24 | 27, -, 2, 20 | new line |
| Subsubtitle | | title 20 | no | no | no | 16 | 27, -, 2, 8 | new line |
| Chapter | Alt+3 | section 10 | no | no | no | 34 | 27, -, 15, 10 | new line |
| Subchapter | | section 20 | no | no | no | 28 | 63, -, 20, 8 | new line |
| Section | Alt+4 | section 30 | no | no | no | 28, rule above | 27, -, 18, 8 | new line |
| Subsection | Alt+5 | section 40 | no | no | no | 20 | 50, 3, 20, 8 | new line |
| Subsubsection | Alt+6 | section 50 | no | no | no | 19 | 66, -, 12, 8 | new line |
| Text | Alt+7 | normal | no | no | no | 15 | 66, 10, 8, 7 | new line |
| Code | Alt+8 | input | yes, initialization | no | yes | 13, bold, code font, tinted background | 66, 10, 10, 10 | new line |
| Input | Alt+9 | input | yes | no | yes | 13, semibold, code font | 66, 10, 10, 5 | new line |
| Item | | item 70 | no | no | no | 15, bullet `▪` | 81, 10, 8, 4 | **new Item cell** |
| ItemParagraph | | item 80 | no | no | no | 15 | 81, 10, 1, 4 | new cell of the same style |
| Subitem | | item 80 | no | no | no | 14, bullet | 105, 12, 4, 4 | new Subitem |
| SubitemParagraph | | item 90 | no | no | no | 14 | 105, 12, 1, 4 | new cell, same style |
| Subsubitem | | item 90 | no | no | no | 13, bullet | 129, 12, 4, 4 | new Subsubitem |
| ItemNumbered, SubitemNumbered, SubsubitemNumbered | | item 70/80/90 | no | no | no | as Item, with `1.` / `1.1.` numbering | as Item | new cell, same style |
| DisplayFormula | | normal | no | no | no | 14, typeset | 66, -, -, - | new line |
| Program | | normal | no | no | no | code font, rules above and below | 66, 10, 8, 8 | new line |
| Output | | output | no | yes | yes | 13, code font, typeset | 66, 10, 5, 10 | (edit duplicates, 2.8) |
| Print | | output | no | yes | no | 13, code font | 60, -, -, - | (edit allowed) |
| Message | | output | no | yes | no | 11, UI font, message colour | 66, -, -, - | not editable |

Notes:

- The Return-key column is *(observed)* for Title, Section and Item ("Return in a Title or Section inserts a line
  break in the same cell; Return at the end of an Item creates a new Item cell") and *(spec)* for the rest.
- The new-cell style is **Input** everywhere *(observed: typing between two Text cells, or after a Section, creates an
  Input cell)*.
- Numbered items count within their enclosing section; counters reset at each Title, Chapter and Section *(spec)*.
- Output, Print and Message are created only by evaluation. The style menu does not list them *(spec)*.

### 2.2 Cell attributes

| Attribute | Meaning | Default |
|---|---|---|
| `editable` | the content can be changed; `false` makes the caret skip the cell's content | true, false for Message |
| `evaluatable` | Shift+Enter sends the cell to the kernel | per style (Input, Code) |
| `deletable` | the cell can be deleted (Delete with its bracket selected, cut, merge) | true |
| `open` | the cell's content is shown (a closed cell shows a thin bar) | true |
| `generated` | made by evaluation; deleted when its input is evaluated again ("auto-overwrite") | per style |
| `initialization` | evaluated automatically before the first evaluation in a fresh kernel (5.7) | true for Code |
| `label` | `In[n]:=` for an evaluated input, `Out[n]=` for an output; Print and Message carry a hidden label linking them to their input | none |
| `typeset` | the content is a box tree (section 4), not plain text | false |
| `grouping` | `together` for cells grouped by hand (2.3) | automatic |

Cell properties are toggled from the Cell menu (`Ctrl+8` toggles `initialization` *(spec)*). The attributes are stored
in the file only when they differ from the style default (7.2).

### 2.3 Grouping

Automatic grouping is computed from the grouping rules in one left-to-right pass with a stack of open groups. The
rules, as established *(observed)*:

- **title n** (Title 0, Subtitle 10, Subsubtitle 20): a Title starts a group that runs to the next Title; Subtitle and
  Subsubtitle are plain members of the Title's group (they start no group of their own). Title groups enclose all
  section groups.
- **section n**: starts a group that runs until the next cell with a title rule or a section rule of level <= n.
  Chapter (10) encloses Sections (30); a Section closes a Subsection's group; a Chapter after a Section closes the
  Section's group.
- **item n**: starts a group that collects the following item cells of higher level and the following input/output
  groups; it ends at an item of level <= n, at a **normal** cell (Text) or at any section or title. An item with
  nothing to collect is a plain cell.
- **input** (Input, Code): starts a group with the immediately following **output** cells (Output, Print, Message, in
  any number and order). An input without outputs is a plain cell; an input never collects the next input.
- **output**: joins the input group before it; with no input before it (after Text, say) it is a plain cell, and
  consecutive orphan outputs are not grouped with each other.
- **normal** (Text and the rest): never starts a group; ends item groups.
- **together** (manual): `Ctrl+Shift+G` on selected cells marks them `grouping = together` and they form one group at
  the position of the first; `Ctrl+Shift+U` removes the mark *(observed for the mark; spec for the keys)*. Manual
  grouping as a notebook-wide mode ("Manual grouping" in the Cell menu) disables the automatic rules and keeps only
  marks.

Test vectors *(observed; braces are groups)*:

```
Title T, Subtitle st, Subsubtitle sst, Text x, Title T2, Text y
  -> {T, st, sst, x} {T2, y}
Text t0, Section S, Subsection SS, Subsubsection SSS, Text a, Subsection SS2, Text b, Section S2
  -> t0 {S {SS {SSS a}} {SS2 b}} S2
Input i, Output o, Text t, Input i2, Print p, Message m, Output o2, Output o3, Input i3, Input i4, Output o4
  -> {i o} t {i2 p m o2 o3} i3 {i4 o4}
Text t, Output o, Print p, Output o2      -> t o p o2
Item i, ItemParagraph ip, Subitem s, Subsubitem ss, Subitem s2, Item i2, Text t, ItemNumbered n, SubitemNumbered sn
  -> {i ip {s ss} s2} i2 t {n sn}
Section S, Item i, Subitem s, Input c, Output o, Subsection SS
  -> {S {i {s {c o}}} SS}
Code c, Output o, Input i, Text t, Output o2  -> {c o} i t o2
Title, Section S1, Text, Input, Output, Input x, Subsection, Item, Subitem, Item, Text, Section S2, Code, Message,
Print, Output, Chapter, Section
  -> {Title {S1 t {In Out} x {Sub {i1 s1} i2 t2}} {S2 {c m pr o}} {cc sec}}
```

The group tree is recomputed after every structural change. It is O(number of cells) and takes microseconds for
1,000 cells, so it is not maintained incrementally.

### 2.4 Open and closed groups

A group is open or closed. A closed group shows **one** of its cells, by default the first (the head); the others are
hidden *(observed)*:

- Double-clicking a group's bracket closes it showing its first cell; double-clicking the bracket of a closed group
  opens it.
- "Open/Close group" (`Ctrl+'`) with the caret or selection in cell *k* of a group closes the innermost group around
  it **showing cell k** *(observed: closing an Input/Print/Output group from the Output leaves the Output visible,
  from the Input leaves the Input visible)*. This is how an input is hidden under its output.
- Double-clicking a single cell's bracket (not a group) does nothing *(observed for the command; guess for the mouse)*.
- `Ctrl+{` / `Ctrl+}` (that is `Ctrl+Shift+[` / `Ctrl+Shift+]`) open / close all subgroups of the selection; applied
  to the whole notebook (`Ctrl+A` first) they fold to the outline *(observed: close-all closes nested groups too)*.
- Typing into, or moving the caret into, a hidden cell is impossible; inserting a cell next to a closed group's visible
  cell opens the group *(observed: a cell typed after a closed group's head lands in the group and opens it)*.
- A closed group's bracket carries a "closed" mark (8.2).

### 2.5 Cell brackets

Brackets are drawn in a column at the **left** edge of the cell area *(user decision, round 11; the reference draws them
on the right)*. Every cell has its own bracket spanning the cell's height (with its top and bottom margins), drawn
nearest the content; every group has a bracket spanning all its visible cells, one column further left: nesting depth
*d* is drawn at column *d*, so the outermost group is leftmost. Hooks point right, towards the content. At least two
group columns are reserved so the content does not move sideways when the first group appears. Bracket geometry: line
thickness 1 px, column pitch 7 px, hooks 3 px, the content 8 pt right of the innermost bracket (scaled with the
magnification) *(spec)*.

The content is **left-justified** against that edge: the styles' left margins of the table in 2.1 are taken relative to
the Input margin (66), so Title, Section, Text, Input and Output start at the edge and items keep their indents. The
insertion line runs from the content edge to the right, its "+" button at its right end (clear of the group
brackets).

Mouse on brackets:

- Click: select that cell or group (a *cell selection*, 2.9). Shift+click: extend the selection to the clicked bracket
  (contiguous range of cells). Ctrl+click: add or remove one bracket *(guess, section 13)*.
- Drag across brackets: select the range. Drag a selected bracket: move the cells (3.7).
- Double-click: open/close (2.4).
- Hover: the bracket is drawn in the hover colour; a tooltip shows the style name *(guess)*.
- Right-click: the bracket context menu (10.4).

Bracket states drawn: normal, hover, selected (filled with the selection colour), queued, evaluating, error (the
input's bracket after a read error), closed group. Section 8 gives the colours.

### 2.6 Labels and the line counter

- The kernel owns a **line counter** *n*, starting at 1 in a fresh kernel. Every top-level statement that is
  evaluated consumes one number *(observed: a cell `1+1 ⏎ 3+3` evaluated as the second input is labelled `In[2]:=`
  and produces `Out[2]= 2` and `Out[3]= 6`; the next cell is `In[4]:=`; a statement ending in `;` consumes a number
  and shows no output)*.
- An evaluated input cell is labelled `In[n]:=` with the number of its **first** statement. Each value shown gets an
  Output cell labelled `Out[k]=` with its statement's number. Print and Message cells carry a hidden label naming the
  input they came from (shown in their tooltip as "during evaluation of In[n]").
- Labels are **not drawn** *(user decision, round 11)*: the numbers stay in the model and the file (`label=`), so
  `out(n)` and the history work; an Output cell's bracket tooltip and the status bar name it as `out(n)`. (Before
  round 11 they sat in the left margin, right-aligned against the content, on the first baseline.)
- Editing an input after evaluation keeps its label until the next evaluation *(observed for programmatic edits;
  section 13 for typing)*. Re-evaluating replaces the label with the new number.
- Labels are saved (7.2) and shown again when the notebook is reopened; the counter of the new kernel starts at 1
  regardless.
- Quitting the kernel (5.6) leaves all labels; the next evaluation starts again at 1.

### 2.7 The insertion point and new cells

Between any two cells, before the first and after the last, there is a **cell insertion point**. When the selection is
an insertion point it is drawn as a horizontal line across the cell area (insertion-line colour), with a small round
"+" button at its left end *(spec)*.

- Clicking in the vertical gap between two cells, or below the last cell, sets the insertion point there.
- Typing a character at an insertion point creates a new cell of the new-cell style (**Input**) containing that
  character, inserted at that point; grouping is recomputed, so a cell typed after a Section's text joins the section
  *(observed)*.
- A typesetting key at an insertion point (for example `Ctrl+/`) creates a new Input cell holding the empty
  structure *(observed: a new Input with an empty fraction, both slots placeholders)*.
- `Alt+digit` at an insertion point sets the style of the cell that typing will create (the line shows the pending
  style name at its left) *(guess)*; typing then creates a cell of that style.
- Paste at an insertion point inserts the clipboard's cells, or one new Input cell holding the clipboard text (3.6).
- The "+" button opens a menu: **Input** (default), **Text** (`Alt+7`), **Other style...** (the style list). Choosing
  one creates an empty cell of that style with the caret in it *(spec)*.
- Up/Down arrows stop at insertion points between cells (3.3).

### 2.8 Generated cells

Output, Print and Message cells are generated: evaluating an input deletes the run of generated cells directly after
it (the auto-overwrite rule) and inserts the new results in their place *(spec, observed for "delete all output",
which removes Output and Print and keeps Input and Text)*.

- Editing an **Output** cell does not change it: the first edit creates a new **Input** cell right after the output's
  group, containing a copy of the output with the edit applied, and moves the caret there ("edit duplicates") *(spec;
  where exactly the copy goes is a guess, section 13)*. The copy is typeset input if the output was typeset.
- Print cells are editable in place; Message cells are not editable.
- `Cell ▸ Delete all output` deletes every generated cell of the notebook.

### 2.9 Selection model

Exactly one of these is current in a notebook window:

| Selection | What it is | Shown as |
|---|---|---|
| caret | a position in one cell's content (a row path and offset for typeset content) | blinking bar, 2 px, caret colour |
| text selection | a range in one cell's content; for typeset content a range inside one row (selections never cross a structure boundary partially: extending across a box selects the whole box) | selection colour behind the text |
| cell selection | a non-empty set of whole cells, chosen by brackets | brackets filled with the selection colour |
| insertion point | a position between cells | horizontal line with "+" |
| graphic | one image in an output (section 6) | frame with eight handles |

Selections never span several cells' contents: dragging the mouse from one cell's text into another's switches to a
cell selection of both cells *(guess)*. `Ctrl+A` with a caret selects the cell's whole content; with a cell selection,
insertion point or nothing, it selects all cells *(spec for the command, guess for the escalation)*.

## 3. Editing

### 3.1 Key bindings

All bindings come from one data table (key chord, context, command) in the model crate; the Help ▸ Keyboard window is
generated from it. Contexts: **text** (caret or text selection in a plain cell), **box** (caret in typeset content),
**cells** (cell selection), **gap** (insertion point), **any**. `Alt` is the reference's "command" modifier on
Windows *(spec)*. The milestone (section 11) that delivers each binding is in the last column.

Evaluation:

| Key | Context | Command | M |
|---|---|---|---|
| Shift+Enter | any | evaluate the selected cells (the cell with the caret, or every evaluatable cell in the cell selection, in order); then move the insertion point below the cell's output group | M1 |
| Keypad Enter | any | as Shift+Enter | M1 |
| Shift+Keypad Enter | any | evaluate, then put the caret at the start of the next evaluatable cell *(spec: "evaluate next cell")* | M2 |
| Ctrl+Shift+Enter | text, box | evaluate the selection in place (replace the selected subexpression by its value) | M6 |
| Alt+. | any | abort the running evaluation and clear the queue (5.6) | M1 |
| Alt+Shift+. | cells | remove the selected cells from the queue | M2 |
| Alt+, | any | interrupt: dialog "Abort / Continue" (5.6) | M2 |

Movement and selection (Shift extends the selection with every motion):

| Key | Command | M |
|---|---|---|
| Left / Right | previous / next character; in typeset content, into and out of structures in reading order | M1 |
| Ctrl+Left / Ctrl+Right | previous / next word ("natural word": identifiers, numbers and operator runs are words) | M1 |
| Up / Down | previous / next visual line; at the first/last line, to the insertion point above/below the cell (3.3) | M1 |
| Home / End | start / end of the visual line | M1 |
| Ctrl+Home / Ctrl+End | start / end of the notebook (insertion point before the first / after the last cell) and scroll there | M1 |
| Ctrl+Up / Ctrl+Down | scroll one line without moving the caret | M1 |
| PageUp / PageDown | scroll one page | M1 |
| Ctrl+. (and Ctrl+Keypad .) | extend the selection to the enclosing expression (3.4) | M3 |
| Ctrl+Shift+B | check balance: select the smallest bracketed expression around the selection; again: the next one out | M3 |
| Ctrl+A | select all (2.9) | M1 |

Editing:

| Key | Command | M |
|---|---|---|
| Enter | line break (indented, 3.2); in Item-like styles a new cell of the style (2.1) | M1 |
| Alt+Enter | new cell of the same style as the current one, directly below it *(observed)* | M2 |
| Tab / Shift+Tab | next / previous placeholder (4.3); without placeholders: indent / outdent the selected lines *(guess, 13)* | M1 (indent), M6 |
| Backspace / Delete | delete previous / next character; with a cell selection: delete the cells | M1 |
| Ctrl+Backspace / Ctrl+Delete | delete previous / next word | M1 |
| Esc | start or finish an alias (3.9) | M3 |
| Ctrl+K, F2 | complete the name before the caret (3.10) | M3 |
| Ctrl+Shift+K | make template: replace a function name by a call with placeholders for its parameters | M6 |
| Alt+/ | comment / uncomment the selected lines (`#`) | M3 |
| Ctrl+Z, Ctrl+Y, Ctrl+Shift+Z | undo, redo, redo (3.5) | M1 |
| Ctrl+X, Ctrl+C, Ctrl+V; Shift+Delete, Ctrl+Insert, Shift+Insert | cut, copy, paste (3.6) | M1 |
| Ctrl+Shift+C | copy as plain text | M2 |
| Ctrl+L | input from above: copy the nearest input cell above into a new cell at the selection *(observed)* | M2 |
| Ctrl+Shift+L | output from above: copy the nearest output above into a new **Input** cell *(observed)* | M2 |

Typesetting (two-dimensional input, section 4; all M6):

| Key | Command |
|---|---|
| Ctrl+/ | fraction |
| Ctrl+6, Ctrl+^ | superscript |
| Ctrl+-, Ctrl+_ | subscript |
| Ctrl+2, Ctrl+@ | square root |
| Ctrl+7, Ctrl+& | overscript ("above") |
| Ctrl+Shift+, | underscript ("below") |
| Ctrl+5, Ctrl+% | opposite position: from a superscript slot to the matching subscript slot and back; from an overscript to the underscript |
| Ctrl+Space | leave the innermost structure: caret just after it |
| Ctrl+Enter | add a row: wrap the operand before the caret in a one-column grid and add a row below |
| Ctrl+, | add a column |
| Ctrl+Tab / Ctrl+Shift+Tab | next / previous expression (caret to the next sibling subexpression) |

Cells, styles and grouping:

| Key | Command | M |
|---|---|---|
| Alt+1 ... Alt+9 | set the style of the selected cells (or of the cell with the caret, or of the cell typing will create) per 2.1 | M2 |
| Alt+0 | other style: style list with type-ahead | M2 |
| Ctrl+Shift+D | divide the cell at the caret (3.8) | M2 |
| Ctrl+Shift+M | merge the selected cells (3.8) | M2 |
| Ctrl+Shift+G / Ctrl+Shift+U | group together / ungroup | M2 |
| Ctrl+' | open/close the group (2.4) | M2 |
| Ctrl+{ / Ctrl+} | open / close all subgroups | M2 |
| Ctrl+8 | toggle initialization cell | M2 |
| Ctrl+Shift+I | convert the selected cells to linear input text (4.6) | M6 |
| Ctrl+Shift+N | convert to typeset input (4.6) | M6 |
| Ctrl+Shift+T | convert to traditional typeset form (4.6) | M6 |
| Ctrl+Shift+E | show expression: toggle between the cell's display and its exact `.mcn` source (3.8) | M2 |

Find: `Ctrl+F` find bar, `F3` / `Shift+F3` next / previous match, `Ctrl+E` use the selection as the search text *(spec)*
(M3).

Reserved by the reference and not bound in MathCAS now: `Ctrl+9`/`Ctrl+0` (inline cells), `Ctrl+4`/`Ctrl+$` (TeX
input), `Ctrl+1` (drawing canvas), `Ctrl+J` (cell tags), `Ctrl+PageUp/PageDown` (template cycling), `Alt+Left/Right`
(hyperlink history), `F7` (subsession), debugger keys. They do nothing rather than something else, so muscle memory
never triggers a different action.

Precedence: when a completion or alias popup is open it takes Up, Down, Enter, Tab and Esc; otherwise the context's
bindings apply; unbound printable keys insert text (or create a cell at an insertion point).

### 3.2 Typing rules

- **Line breaks and indentation.** Enter in an Input or Code cell inserts a line break. Indentation is **visual**: the
  editor indents a continuation line by the bracket depth at its start (two em-spaces per level) without inserting
  characters, as the reference does *(observed: Return inside an open `f[x` inserts only a newline into the content)*.
  Spaces the user types are kept. The file stores exactly what was typed (7.4).
- **Spacing.** In Input cells binary operators are displayed with operator spacing (thin space around `+`, `-`, `=`,
  `->`, comparisons; none around `^`, `/` in linear text, `.`, calls) without changing the text; Code cells show the
  text verbatim *(spec: automatic spacing on for Input, off for Code)*.
- **Operator rendering.** In Input and Output cells the character pairs `->` `:>` `<=` `>=` `!=` `==` `<->` are drawn as
  single glyphs (→ ⧴ ≤ ≥ ≠ ⩵ ↔) while the text stays ASCII; the caret steps over the pair as one unit and Backspace
  removes both characters *(spec for the substitution; guess for caret and Backspace)*. A preference turns it off.
  `=>` (MathCAS's lambda) is drawn as `⇒`.
- **Quotes in prose.** In Text-like cells a typed `"` or `'` becomes a curly quote pair character (“ ” ‘ ’) by context;
  in Input/Code cells quotes stay straight *(spec)*.
- **Closing brackets** are not inserted automatically, matching the reference *(guess, 13)*; a preference enables
  auto-closing of `()`, `[]`, `{}` and `""`.
- **Backspace at the start of a cell** does nothing; Delete at the end does nothing (cells are never merged by
  deletion) *(guess)*.
- **Typing with a cell selection** (brackets selected) does nothing except the cell-level keys (style, delete,
  evaluate, copy, group) *(guess, 13)*.
- **Read-only cells.** Keys that would change a non-editable cell beep (system "asterisk" sound) and do nothing.

### 3.3 Cursor movement across cells

- Down on the last visual line of a cell moves to the insertion point below the cell; Down again moves to the first
  line of the next visible cell, keeping the goal x position. Up mirrors it *(guess for the exact stops, 13)*.
- Right at the end of a cell's content moves to the insertion point after it, then into the next cell's start; Left
  mirrors it *(guess)*.
- Movement skips hidden cells (closed groups) and non-editable content (Message cells): the caret lands on the
  insertion point after them.
- Ctrl+Home / Ctrl+End go to the insertion point before the first / after the last cell *(spec)*.

### 3.4 Extend selection (Ctrl+.)

Each press selects the smallest syntactic unit strictly larger than the current selection: token, then the smallest
expression containing it, then each enclosing expression (call with its arguments, argument sequence, whole call,
operator expressions by precedence), then the cell's whole content, then the cell itself (a cell selection); further
presses keep the cell selected *(observed)*.

From a caret, the first press selects the token touching the caret; if the caret touches only a bracket or an
operator, the smallest expression that bracket or operator belongs to. Observed vectors on the reference's
equivalent of `f(aa + b*c, d)`, by caret position:

```
before f              f
after f, before (     f(aa + b*c, d)
inside aa             aa
after aa, before +    aa
after +, before b     aa + b*c
after b               b*c
after c, before ,     b*c
after ,, before d     aa + b*c, d          (the argument sequence)
then from "b*c":      aa + b*c  ->  aa + b*c, d  ->  f(aa + b*c, d)  ->  whole content  ->  the cell
```

The selection grammar is MathCAS's (mc-syntax token spans plus a bracket/precedence tree), so the units differ where
the syntaxes differ (the reference multiplies by juxtaposition, MathCAS writes `*`); the algorithm is the same. Note
the observed irregularity "after b selects `b*c`, not `b`": a caret between a token and a multiplication operator
selects the product.

### 3.5 Undo and redo

- One undo history per notebook, unlimited depth within a session, not saved.
- Undo units: a run of typing in one cell (closed by a pause of 1 s, a caret jump, a non-typing command or a word
  boundary after whitespace); each structural command (create, delete, move, merge, divide, style change, group,
  convert, paste) is one unit, also when it touches many cells.
- Evaluation results are not undo units: undo never removes or restores generated cells, and edits made before an
  evaluation remain undoable after it (the generated cells are left as they are). Cell labels are not restored by undo.
- Redo is cleared by a new edit. `Ctrl+Y` and `Ctrl+Shift+Z` redo *(spec)*.

### 3.6 Clipboard

What is copied:

| Selection | Clipboard flavours written |
|---|---|
| text in a plain cell | text |
| a range in typeset content | text (linear input text, 4.6) and the private flavour (box tree) |
| cells | text (the cells' contents: input source, text, outputs as printed, separated by blank lines) and the private flavour (the `.mcn` fragment of the cells, exactly as saved) |
| a graphic | image (PNG) and text (`image("...")` with an absolute path) |

The private flavour is a registered Windows clipboard format "MathCAS.Cells" carrying UTF-8 `.mcn` text (with
`clipboard-win`); on other platforms the text flavour is used with a recognisable first line (`#% cells`), and paste
recognises it.

Paste:

- Into a caret or text selection: replaces the selection. From the private flavour into typeset content: the boxes; a
  cells fragment pasted into text pastes the cells' text. Plain text into typeset content is inserted as text (it is
  not re-typeset; `Ctrl+Shift+N` does that).
- At an insertion point: the private flavour inserts its cells; plain text creates **one** Input cell holding it *(guess
  for multi-paragraph text)*; an image creates an Output-less Print-style picture cell *(later; M4 ignores images)*.
- With a cell selection: replaces the selected cells *(guess, 13)*.
- Copy as plain text (`Ctrl+Shift+C`) writes only the text flavour (linear input text, no box markers).

### 3.7 Drag and drop

- Dragging a selected bracket moves the selected cells; an insertion line follows the pointer between visible cells.
  Ctrl while dropping copies instead of moving. Dropping onto a closed group's position inserts after the whole group.
  Dragging to the window's top or bottom edge autoscrolls.
- Dragging selected text inside or between cells moves it (copies with Ctrl) *(spec for the existence; M7)*.
- Files dropped on the window: `.mcn` opens a notebook; `.mc` is inserted as one Input cell per top-level paragraph
  *(MathCAS)*; images insert a picture *(M7)*.

### 3.8 Cell operations

- **Divide** (`Ctrl+Shift+D`): split the cell at the caret into two cells of the same style; with a text selection,
  split before and after it (up to three cells; empty pieces are dropped) *(observed for the caret; guess for the
  selection, 13)*. A divided input loses its label.
- **Merge** (`Ctrl+Shift+M`): join the selected cells into one cell of the **first** cell's style, contents joined with
  a line break *(observed: `a=1`, `b=2` and Text `c` merge into one Input `a=1⏎b=2⏎c`)*. Generated cells in the
  selection are dropped. A merge of one cell does nothing.
- **Input from above** (`Ctrl+L`): a new Input cell at the selection holding a copy of the nearest input cell above
  *(observed)*. **Output from above** (`Ctrl+Shift+L`): the same with the nearest output, as an Input cell *(observed)*.
- **Same style below** (`Alt+Enter`): an empty cell of the current cell's style directly after the current cell (not
  after its group); grouping then applies *(observed)*.
- **Show expression** (`Ctrl+Shift+E`): the cell is shown as its exact `.mcn` text (marker line, attributes, canonical
  box text, output lines) in the code font; it can be edited there, and `Ctrl+Shift+E` again parses it back (a parse
  error keeps the source view and marks the error). The reference's equivalent shows the cell's underlying expression.
- **Delete all output** (Cell menu): 2.8.

### 3.9 Aliases (Esc)

Mechanics *(spec, details guessed where marked)*:

1. `Esc` in an editable cell inserts an alias start marker, drawn as `⋮` in the alias colour.
2. Letters and symbols typed after it are drawn in the alias colour.
3. `Esc` again looks the text between the markers up: found, the whole alias is replaced by the character or template;
   not found, the text stays marked as an unknown alias (error colour) until edited *(guess)*.
4. Backspace right after the start marker removes it; moving the caret out of an unfinished alias leaves it as plain
   text *(guess)*.
5. Inside an alias, `Esc` never reaches the completion popup.

The table is MathCAS's own, one entry per line in `crates/mc-boxes/data/aliases.tsv`: `alias <TAB> result <TAB>
kind`. Result is a Unicode character string or a template in canonical box text (4.4) with `□` placeholders and
`▮` marking where the caret goes. It is generated once from public data (Unicode character names, the TeX/amssymb
macro names already in `texmath`'s symbol table, HTML entity names) and then edited by hand; kinds:

| Kind | Rule | Examples |
|---|---|---|
| Greek short | one or two letters of the transliteration; capitals with a capital letter | `a` α, `b` β, `g` γ, `d` δ, `e` ϵ, `ce` ε, `z` ζ, `et`/`h` η, `th`/`q` θ, `i` ι, `k` κ, `l` λ, `m` μ, `n` ν, `x` ξ, `p` π, `r` ρ, `s` σ, `t` τ, `u` υ, `ph`/`f` ϕ, `cph` φ, `ch`/`c` χ, `ps`/`y` ψ, `o`/`w` ω; `G` Γ, `D` Δ, `Th`/`Q` Θ, `L` Λ, `P` Π, `S` Σ, `Ph`/`F` Φ, `Ps`/`Y` Ψ, `O`/`W` Ω |
| Greek full | the letter's name | `alpha`, `Alpha`, `theta`, `vartheta` ... |
| TeX | `\` + the TeX macro name | `\alpha`, `\infty`, `\leq`, `\rightarrow`, `\int` |
| HTML | `&` + entity + `;` | `&alpha;`, `&le;` |
| Operators | ASCII look-alike | `->` →, `:>` ⧴, `<=` ≤, `>=` ≥, `!=` ≠, `==` ⩵, `<->` ↔, `*` ×, `.` ·, `+-` ±, `-+` ∓, `inf` ∞, `deg` °, `elem`/`el` ∈, `!elem` ∉, `sub` ⊂, `sup` ⊃, `sub=` ⊆, `un` ∪, `inter` ∩, `and` ∧, `or` ∨, `not` ¬, `fa` ∀, `ex` ∃, `pd` ∂, `del` ∇, `int` ∫, `sum` ∑, `prod` ∏, `===` ≡, `~~` ≈, `prop` ∝ |
| Constants | double letters | `ee` ⅇ, `ii` ⅈ, `dd` ⅆ (differential d), `pi` π |
| Letter-likes | prefix + letter | `dsR` ℝ (double-struck), `scl` ℓ (script), `goR` ℜ (gothic), `hb` ℏ, `al` ℵ, `es` ∅ |
| Spaces | spaces | `  ` thin space, `   ` medium |
| Templates | name ending in `t` | `intt` ∫ □ d□ (indefinite integral), `dintt` definite integral with limits, `sumt` Σ with `□ = □` below and `□` above, `prodt`, `dt` partial derivative ∂_□ □, `cbrt` cube root, `surd` n-th root, `mat` 2×2 grid |

Every inserted character must read as valid MathCAS syntax where it has a meaning: the mc-syntax lexer accepts the
Unicode operator equivalents (→ for `->`, ⧴ for `:>`, ≤ ≥ ≠ ⩵, × for `*`, · for `*`, ∞, π, ⅇ, ⅈ, ∈) and
Greek letters are identifier characters (already true). Characters without a meaning (∀, ℏ ...) are identifier-like
symbols or remain syntax errors with a clear message (work package P7).

### 3.10 Completion

- An automatic popup opens after typing two identifier characters with no other key for 300 ms *(the reference opens it
  without delay; MathCAS waits a little: guess-level divergence, 13)*; `Ctrl+K` or `F2` opens it at once.
- Candidates: kernel names (`complete` request, cached and refreshed after each evaluation), local names in scope
  (from mc-syntax), and keywords. Matching is case-insensitive prefix first, then fuzzy subsequence, ranked by
  match quality then name length *(the reference uses fuzzy completion; spec)*.
- Up/Down move, Enter or Tab accept, Esc closes. Accepting a function name from the library may insert a template
  with `Ctrl+Shift+K` (M6).
- The popup shows the name's one-line usage (from the library documentation strings) beside the list.

### 3.11 Syntax colouring

Colouring classes follow the reference's set of highlighted conditions *(spec)*; colours are Atom's (8.2). Classes and
how MathCAS computes them:

| Class | Applies to | Source |
|---|---|---|
| plain | defined global names (library functions, built-ins, session definitions), operators, punctuation | default text colour |
| undefined name | a global name with no definition in the kernel and not local | kernel name set (3.10) |
| local variable | names bound by `let`, `var`, lambda and function parameters, `for` variables, inside their scope | mc-syntax scope analysis |
| pattern variable | `x_` names and their uses on the right-hand side of a `rule`/`fn` | mc-syntax |
| keyword | `let var fn rule sym module pub import if then else while for in match when held and or not` | lexer |
| constant | `true false null` | lexer |
| number | numeric literals including balls and `im` | lexer |
| string | string literals with their quotes | lexer |
| comment | `#` to end of line | lexer |
| syntax error | the token at a parse error position, with an error background | parser diagnostics |
| unmatched bracket | a bracket with no partner (also drawn bold) | bracket pass |
| shadowing | a local name that hides a global definition | scope analysis |
| excess argument / unknown option | calls with more arguments than any signature accepts, unknown named arguments | kernel signature data (M7) |

Colouring is recomputed for a cell on each edit (the lexer and parser are fast enough for cells of thousands of lines;
cells over 100,000 characters colour only lexically). The undefined-name class needs the kernel; with no kernel
running every global name is plain *(the reference has a "no kernel" style; MathCAS keeps it simple)*.

### 3.12 Bracket matching

- When the caret is next to a bracket, it and its partner are highlighted with the match background for as long as the
  caret stays *(spec: the reference highlights matching delimiters dynamically)*.
- Double-clicking a bracket selects the bracketed expression with its brackets; double-clicking a word selects the
  word; triple-click selects the line *(guess for triple)*.
- Typing a closing bracket briefly flashes its partner if it is off-screen line, via a tooltip-like peek *(MathCAS)*.

## 4. Two-dimensional typesetting

### 4.1 Box model

Typeset content is a tree of **rows**; a row is a sequence of **items**: characters and boxes. Box kinds:

| Box | Slots | Drawn as |
|---|---|---|
| Fraction | numerator, denominator | stacked with a rule |
| Superscript | base, exponent | exponent raised, smaller |
| Subscript | base, index | index lowered, smaller |
| (sub+super) | a Superscript whose base is a Subscript | both scripts stacked on the right |
| Radical | radicand, optional index | radical sign over the radicand, index in the notch |
| Overscript / Underscript / Underoverscript | base, over and/or under | stacked centred |
| Grid | rows of cells | aligned columns, no delimiters (delimiters are characters around it) |
| Big operator | operator (∑ ∏ ∫ lim), lower, upper, body, (∫: variable) | operator with limits above/below (display) or at the side (inline) |
| Placeholder | none | `□` (empty slot), selected placeholder `▮` *(spec: the reference uses two placeholder kinds, the selected one first)* |
| Graphic | image reference | the image (section 6) |

A caret position is (row path, offset). Bases of scripts are **boxes of their own** (they wrap the operand), as the
reference does: the base is a row slot, not "the preceding character".

### 4.2 Structure commands

When a structure key is pressed with a caret, the structure takes the **operand before the caret** as its first slot
and the caret moves to the next slot. The operand is the last *primary* ending at the caret: a name, a number, a
bracketed group, a list, a call (name with its argument brackets), or an existing box (including a superscript); never
a whole product or sum (`a*b` gives `b`, as `1+x` gives `x`). Rules *(observed on the reference's equivalent inputs;
caret at the end of the given text; the result shows where a typed `Q` lands, then `Ctrl+Space` and a typed `Z`;
`grid` is a grid box, `over` an overscript box, `□` a placeholder)*:

```
text         Ctrl+/           Ctrl+^           Ctrl+_             Ctrl+2          Ctrl+Enter            Ctrl+,
1+x          1+(x/Q)Z         1+(x^Q)Z         1+(x_Q)Z           1+x√(Q)Z        1+grid[[x],[Q]]Z      1+grid[[x,Q]]Z
f(x)         (f(x)/Q)Z        (f(x)^Q)Z        (f(x)_Q)Z          f(x)√(Q)Z       grid[[f(x)],[Q]]Z     grid[[f(x),Q]]Z
a*b          a*(b/Q)Z         a*(b^Q)Z         a*(b_Q)Z           a*b√(Q)Z        a*grid[[b],[Q]]Z      a*grid[[b,Q]]Z
(a+b)        ((a+b)/Q)Z       ((a+b)^Q)Z       ((a+b)_Q)Z         (a+b)√(Q)Z      grid[[(a+b)],[Q]]Z    grid[[(a+b),Q]]Z
[a, b]       ([a, b]/Q)Z      ([a, b]^Q)Z      ([a, b]_Q)Z        [a, b]√(Q)Z     grid[[[a, b]],[Q]]Z   grid[[[a, b],Q]]Z
x^2 (box)    (x^2/Q)Z         ((x^2)^Q)Z       ((x^2)_Q)Z  *      x^2√(Q)Z        grid[[x^2],[Q]]Z      grid[[x^2,Q]]Z
(empty)      (Q/□)Z           (Q^□)Z           (Q_□)Z             √(Q)Z           grid[[□],[Q]]Z        grid[[□,Q]]Z
x+           x+(Q/□)Z         x(+^Q)Z          x(+_Q)Z            x+√(Q)Z         grid[[x+],[Q]]Z       grid[[x+,Q]]Z
```

- `Ctrl+7` (over) and `Ctrl+Shift+,` (under) take exactly the operands `Ctrl+^` takes, giving `(x over Q)`,
  `(f(x) over Q)`, ..., `(Q over □)` for an empty start, `x(+ over Q)` after an operator.
- `*`: a subscript on a superscript box wraps the base in parentheses first (`(x²)_Q`); a superscript on a superscript
  box nests (`(x²)^Q`).
- With nothing to take (empty, or after a binary operator for Ctrl+/), the caret goes into the **first** slot and the
  other slot is a placeholder. Superscript, subscript and over/underscript after an operator take the operator as
  base *(observed; harmless)*.
- **Radical never takes an operand**: it inserts an empty radical at the caret *(observed)*.
- Ctrl+Enter and Ctrl+, take the whole current row segment when there is no operand boundary (`x+` becomes one grid
  cell) *(observed)*. Inside an existing grid they add a row / column to it.
- **Ctrl+5** with no script slot to switch to does nothing *(observed)*.
- With a **selection**, the selection becomes the first slot and the caret goes to the second *(observed: selecting
  `1+x` then Ctrl+/ gives `(1+x)/Q`)*.
- `Ctrl+Space` moves the caret just after the innermost enclosing structure *(observed)*.

### 4.3 Placeholders and templates

- Empty slots show a placeholder. `Tab` moves to the next placeholder in reading order and selects it, `Shift+Tab` to
  the previous; typing replaces a selected placeholder *(observed: Ctrl+/, `A`, Tab, `B` gives `A/B`; Ctrl+/, Tab, `B`
  gives `□/B`)*.
- A cell containing placeholders evaluates to a read error pointing at the first placeholder (the canonical text
  carries `□`, which is not valid syntax).
- Templates (alias kind "Templates", `Ctrl+Shift+K`) insert boxes with placeholders and put the caret in the first.

### 4.4 Canonical text: boxes to `.mc` source and back

Typeset content is saved, sent to the kernel and copied as **canonical box text**: ordinary MathCAS source in which
each box is written as a **double-parenthesised** expression of a fixed shape. Double parentheses are mere grouping
for the parser, so the text evaluates exactly as the box means; for the editor they mark "this was a box". Rules:

| Box | Canonical text | Evaluates as |
|---|---|---|
| Fraction(N, D) | `((N)/(D))` | `N / D` |
| Superscript(B, E) | `((B)^(E))` | `B ^ E` |
| Subscript(B, S) | `((subscript(B, S)))` | inert `subscript(B, S)` (new library head) |
| Radical(X) | `((sqrt(X)))` | `sqrt(X)` |
| Radical(X, n) | `((root(X, n)))` | real n-th root (new library function `root`) |
| Overscript(B, O) / Underscript(B, U) / Underoverscript | `((overscript(B, O)))`, `((underscript(B, U)))`, `((underoverscript(B, U, O)))` | inert heads (new) |
| Grid | `(([[a, b], [c, d]]))` | the list of rows |
| Sum / product with limits | `((sum(f, [k, a, b])))`, `((product(f, [k, a, b])))` | the library call |
| Integral | `((integrate(f, x)))`, `((integrate(f, [x, a, b])))` | the library call |
| Limit | `((limit(f, x -> a)))` | the library call |
| Partial derivative template | `((d(f, x)))` | the library call |
| Placeholder | `□` | read error |
| Special characters | the Unicode character itself | per lexer (3.9) |

Reading back: a cell marked `typeset` (7.2) is tokenised with mc-syntax's lexer (which keeps spans and brackets); every
`((` ... `))` pair whose inside has one of the shapes above becomes the box, recursively; everything else stays linear
text. Inside a slot, a single pair of parentheses around the slot text is the slot delimiter (`((N)/(D))`: N and D);
parentheses the user typed are additional pairs and are kept.

Properties (tested): `text(boxes(t)) == t` for every canonical text the editor writes, and `boxes(text(b)) == b` for
every box tree. Linear text the user typed never changes on a round trip; the only ambiguity is a user typing a
canonical form like `((a)/(b))` by hand in a typeset cell, which then shows as a fraction on reload: same meaning, so
benign, and documented.

Why this form: it is valid MathCAS that evaluates as displayed, readable in a diff (`x = ((a + b)/(2c))`), and needs
no side tables or invisible characters. Rejected: per-cell box annotations (stale after hand edits), invisible Unicode
delimiters (hidden state in a hand-editable file), dedicated Unicode brackets (unfamiliar in source).

### 4.5 Typeset output

Output values arrive as printed MathCAS text (kernel protocol `value.printed`). The notebook parses the printed text
with mc-syntax and maps the syntax tree to boxes (the kernel needs no box format, and `.mcn` output lines, which are
printed text, re-typeset identically when a notebook is reopened):

| Printed form | Typeset as |
|---|---|
| `a/b`, `a*b/c` (a division in a product) | fraction (numerator factors over denominator factors) |
| rational literals `1/2` | fraction, smaller when inline in a product *(guess on size)* |
| `x^e` | superscript; `x^(1/2)` and `sqrt(x)` radical; `x^(1/n)` and `root(x, n)` radical with index |
| `a*b` | juxtaposition with a thin space (`2 x`, `a b`); `*` between two numbers stays `×` |
| `subscript(x, i)`, `overscript`, `underscript` | the script boxes |
| `sum(...)`, `product(...)`, `integrate(...)`, `limit(...)`, `d(f, x)` (unevaluated) | big operators, `∂f/∂x` |
| `derivative(n)(f)(x)` | `f′(x)`, `f″(x)`, `f⁽ⁿ⁾(x)` |
| `pi`, `inf`, `e`, `im` | π, ∞, ⅇ, ⅈ |
| `->`, `<=`, `>=`, `!=`, `==` | → ≤ ≥ ≠ ⩵ |
| strings | the characters without quotes *(spec: output shows strings without quote marks)* |
| `image("path")` | the graphic (section 6) |
| `failure(...)` | linear text in the message colour |
| anything unparsable (elisions, opaque objects) | linear text |

Lists stay linear, except that a list of 2 to 64 lists of equal length (up to 64 elements, none of them a list) is
drawn as a matrix: a grid between the list's own stretched brackets *(MathCAS decision NB6.3, round 11: the reference
keeps nested lists linear in its default output form; in a typeset notebook such a list is nearly always a matrix; a
preference turns it off)*. A product with a rational coefficient, printed `(2/3)*sqrt(2)`, is one fraction with the
coefficient's numerator in front (2√2 over 3), a negative coefficient a minus before it *(NB6.2)*.

A typeset output takes a read-only caret between its items (round 11): arrows, Shift, `Ctrl+.`, clicks and drags
select a part, which copies as the printed text it stands for (each item keeps a map back to the printed text, which
also lets Find highlight the matched part).

### 4.6 Linear, typeset and traditional forms

- **Linear input text** (`Ctrl+Shift+I`, "Copy as plain text"): the cell's canonical text with box markers reduced to
  minimal parentheses (`((a + b)/(2c))` -> `(a + b)/(2c)`, `((x)^(2))` -> `x^2`). The cell's `typeset` mark is removed.
- **Typeset input** (`Ctrl+Shift+N`): the linear text is parsed and every construct with a box form (4.5 table) becomes
  a box; the cell gets the `typeset` mark. On a parse error nothing changes and the error is shown.
- **Traditional** (`Ctrl+Shift+T`): as typeset, drawn in traditional math style (single-letter names in
  italics, function names upright, `sin x` without parentheses for single-symbol arguments, ⅆx in integrals). The cell
  gets the `traditional` mark; it evaluates through the same canonical text. Output cells can be shown traditionally
  with the same key.

### 4.7 Layout

Box layout follows TeX's math rules (TeXbook Appendix G) driven by the OpenType MATH table of the math font, as v1's
`texmath` does: fraction rule thickness and shifts, superscript/subscript shifts and the script and script-script
scale (70 %, 50 %), radical gap and rule, operator limits, stretchy delimiters from glyph variants and assemblies (`(`
`[` `{` `|` around tall content grow), grid gaps. Inputs use the reference's "inline" style (fractions at text style,
limits of big operators beside them); outputs and DisplayFormula cells use display style for big operators
*(spec: the reference's typeset forms differ in script level between input and display formulas)*.

### 4.8 Line breaking

- Input and output content wrap at the cell's width (window width minus margins and brackets); nothing scrolls
  horizontally except a single item wider than the cell (a wide grid or image), which scrolls inside its cell.
- Breaks are chosen by a penalty: lowest bracket depth first, then after `,` and `;`, then before binary operators by
  precedence (`+ -` before `*`), never inside a token or inside a box; boxes too wide for a line break inside their
  slots (numerator and denominator wrap separately).
- Continuation lines of inputs are indented by the bracket depth at the break (3.2); outputs' continuation lines start
  at the cell's left edge *(guess, 13)*.
- Wrapping is recomputed on window resize for the visible cells first, the rest lazily.

### 4.9 Large outputs

An output whose printed text exceeds the size limit (1 MiB by default, a preference) is not typeset in full *(spec:
the reference's default limit is about 1 MB)*. The Output cell instead shows a framed summary:

- a title line: "A very large output was generated. Here is a sample of it:";
- a sample: the printed text shortened to about 2,000 characters by eliding the middle of long lists and arguments as
  `«n»` (n = number of elided elements), keeping the structure balanced;
- buttons: **Show less** (halve the sample), **Show more** (double it), **Show full output** (typeset everything,
  with a progress indicator; cancellable), **Set size limit...**;
- the full text is kept in the model (and saved), so copying the cell copies everything.

The summary is a display state, not saved.

### 4.10 Fonts

| Role | Font | Source |
|---|---|---|
| Code (Input, Code, Output, Print) | Consolas | system (Windows); fallback Cascadia Mono, then egui's bundled Hack |
| Math (structures, big operators, stretchy delimiters, math symbols, Greek in typeset content) | STIX Two Math | bundled, SIL Open Font License 1.1 (v1's copy) |
| Prose (Title ... Text, Items) | Segoe UI (Variable on Windows 11) | system; fallback egui's bundled Ubuntu |
| UI, labels, messages | Segoe UI | system |

Missing glyphs fall back along the list (egui font families with fallbacks); the math font comes last in every family
so every math symbol has a glyph. All sizes scale with the window magnification (50 % - 300 %).

## 5. Evaluation model

### 5.1 The kernel process

- The notebook starts `mc-kernel` (found next to the notebook executable, overridable in preferences) as a child
  process with piped stdin/stdout, `--threads N` from preferences, and a working directory equal to the notebook's
  folder (so `import` finds modules next to it, as `mc-repl notebook.mcn` does).
- A reader thread parses JSON lines into responses and hands them to the UI over a channel, waking the UI (egui
  repaint request). A writer function serialises requests. stderr is captured into the kernel log window.
- The kernel is started on the first evaluation, not when a notebook opens *(spec)*. One kernel per notebook window
  (MathCAS; the reference shares one kernel by default; section 13).
- A kernel that exits (EOF on stdout) marks the session dead: the running cell gets a Message cell "the kernel
  stopped" and the queue is cleared; the next evaluation starts a new kernel.

### 5.2 Protocol additions (protocol 1, compatible)

All additions are optional request fields and new response fields or types, which protocol 1 allows; `mc-repl` and the
Jupyter kernel keep working unchanged.

| Addition | Where | Meaning |
|---|---|---|
| `values: "each"` | `eval` request | send a `value` for **every** top-level statement whose value is shown (not `null`, not a definition, and not suppressed by a trailing `;` on that statement), instead of only the last |
| `numbered: true` | `eval` request | the kernel numbers statements with its line counter (2.6) and records history |
| `n` | `value`, `failure`, `output`, `error` responses | the statement's line number |
| `first`, `next` | `done` response | the first number used by the request and the counter after it |
| `image` | new response type `display` with `kind: "image"`, `path`, `width`, `height` | a statement's value is an image term; sent instead of `value` (the printed form goes in `printed` for the file) |
| `message` | new response type with `text`, `tag` | warnings from library code (now printed with `print`), shown as Message cells |
| print per call | `output.text` | in `values: "each"` mode, one `output` per `print` call, the text may contain newlines |
| `names` | new request | defined global names and their kinds (function, value, module) for colouring; `complete` remains for completion |
| `history` | kernel state | `out(n)` gives the value of statement n, `out()` the last, `out(-k)` the k-th previous; the history is cleared by `reset` |

The notebook always sends `values: "each"` and `numbered: true`. Whether MathCAS also gets the reference's `%` syntax
for `out()` (`%`, `%%`, `%n`) is a syntax decision for `04-syntax.md` (section 13, question 1); the notebook works
either way.

### 5.3 The evaluation queue

- Shift+Enter appends the selected evaluatable cells to the **notebook's queue** in document order. The queue lives in
  the notebook, not in the kernel: the notebook sends one `eval` at a time and sends the next when `done` arrives, so
  that abort, deletion and editing of queued cells act on the queue directly.
- The text sent is the cell's canonical text **at the moment its evaluation starts** *(guess, 13)*; the `cell` field
  is the cell's stable id; `origin` is the notebook path; `line` is the cell's first line in the file, so error
  positions point into the notebook (decision 2.8 of `mc-kernel`).
- A queued cell's bracket shows the queued state; the running cell's bracket shows the evaluating state, and its label
  shows `In[*]:=` until the first response arrives *(guess)*.
- Deleting a queued cell removes it from the queue. Deleting the running cell lets the evaluation finish and discards
  its results.
- Evaluating a cell that is already queued does not queue it twice.
- When the queue empties, the selection moves to the insertion point after the last evaluated cell's output group if
  the user has not moved it meanwhile *(spec: Shift+Enter leaves the insertion point below the new output)*.

### 5.4 Responses to cells

When a cell's evaluation starts, its old generated cells are deleted (2.8). Then, as responses stream in:

| Response | Becomes |
|---|---|
| `output` | a Print cell after the input's generated cells so far |
| `message` | a Message cell |
| `value` | an Output cell `Out[n]=` with the printed text, typeset (4.5) |
| `display` (image) | an Output cell `Out[n]=` holding the graphic |
| `failure` | an Output cell `Out[n]=` showing `failure(...)` in the message colour (in MathCAS a failure is the statement's value; `out(n)` returns it) |
| `error` (read error, nothing ran) | the input's bracket turns to the error colour; a Message cell with the rendered error (source line and caret) |
| `error` (limit) | a Message cell `limit: ...` |
| `done` `interrupted` | an Output cell `Out[n]= aborted` for the running statement *(the reference shows its "aborted" symbol as the output)* |
| `done` | the input gets `In[first]:=`; the bracket returns to normal; the status bar shows `millis` |

Generated cells appear while the evaluation runs (streaming per statement), so long computations show their prints.

### 5.5 Numbers and history

Labels use the kernel's numbers (`n`, `first`). The kernel keeps history only in `numbered` mode, so scripts and
Jupyter are unaffected. A read error consumes one number (the input is labelled; a Message cell follows) *(spec)*.

### 5.6 Abort, interrupt, quit

- **Abort** (`Alt+.`): sends `interrupt` for the running request and clears the queue *(guess that the queue is
  cleared, 13)*. The running statement ends `interrupted` (5.4).
- **Remove from queue** (`Alt+Shift+.`): unqueues the selected cells.
- **Interrupt** (`Alt+,`): a dialog with **Abort** (as above) and **Continue** (close the dialog). The reference's
  interrupt also offers to enter a subsession; MathCAS has none.
- **Quit kernel** (Evaluation menu): asks for confirmation if the kernel is busy; sends `shutdown`, kills the process
  after 2 s if it has not exited; clears the queue. Labels stay; the next evaluation starts a new kernel at 1.
  **Restart kernel** = quit + start.
- If the kernel does not answer an `interrupt` within 3 s, the status bar offers "Kill kernel".

### 5.7 Initialization cells

Before the first evaluation in a fresh kernel, if the notebook has initialization cells (Code cells by default), a
dialog asks "Evaluate the initialization cells first?" (Yes / No / Always for this notebook); Yes queues them, in
order, before the requested cells *(spec for the feature)*. Evaluation ▸ Evaluate initialization cells does it on
demand.

## 6. Graphics in cells

- **Now (PNG):** a value `image("path")` (from `mc-plot`) is displayed as the image in its Output cell, at the image's
  pixel size scaled by the magnification and the display scale factor, at most the cell width (scaled down keeping the
  aspect ratio). Images are decoded with `png` (already used by mc-plot) on a worker thread and cached as GPU textures
  keyed by path and modification time; the cache is limited (256 MB of pixels, least recently drawn evicted).
- **Selection:** a single click on an image selects the graphic: a 1 px frame in the accent colour with eight square
  handles. Click outside, Esc or any arrow key deselects. Copy copies the image (PNG) and its `image(...)` text.
- **Resizing:** dragging a corner handle resizes keeping the aspect ratio; side handles change one dimension only with
  Shift held (otherwise also keep the ratio) *(guess)*. The size is stored on the output (`size=WxH`, 7.2) and kept
  across re-evaluation of the same input when the new value is also an image *(guess)*. Double-click on a resized
  image restores its natural size *(MathCAS)*.
- **Later (vector):** values `svg("path")` or a graphics term rendered by a future `mc-graphics`: rasterised with
  `resvg`/`tiny-skia` (in the local caches) at the current display scale on a worker thread, re-rasterised on zoom;
  hit-testing per element is out of scope.
- Images in saved notebooks: 7.5.

## 7. File format

### 7.1 Today

The `.mcn` format of `crates/mc-kernel/NOTES.md`: header `#! mathcas 2 notebook`; marker lines `#% cell` (code),
`#% text` (prose as `# ...` lines), `#% output` (`#|` print, `#=` value, `#!` failure/error); any other `#%` line is a
comment; every marker, text and output line is a comment, so the file is also a script.

### 7.2 Extended grammar

```
file        = header { blank } { cell }
header      = "#! mathcas 2 notebook" [ attrs ] NL
cell        = marker NL body { output-block }
marker      = "#% " kind [ attrs ]
kind        = "cell" | "input" | "code" | "text" | "title" | "subtitle" | "subsubtitle" | "chapter" | "subchapter"
            | "section" | "subsection" | "subsubsection" | "item" | "itemparagraph" | "subitem" | "subitemparagraph"
            | "subsubitem" | "itemnumbered" | "subitemnumbered" | "subsubitemnumbered" | "displayformula" | "program"
attrs       = { " " attr }
attr        = name | name "=" value          (value: no spaces; `[0-9A-Za-z_.:x-]+`)
body        = source lines (code kinds) | "# " text lines and "#" empty lines (prose kinds)
output-block= "#% output" [ attrs ] NL { out-line }
out-line    = "#| " print | "#= " value | "#! " failure/error | "#? " message
```

- `cell` and `input` are the same kind (`cell` stays the canonical spelling, so existing files are unchanged).
- Cell attributes: `label=N` (In number), `typeset`, `traditional`, `closed` (the group headed by this cell is closed,
  showing its first cell), `shown=K` (the group headed by this cell is closed showing its K-th cell, 2.4), `noeval`,
  `readonly`, `nodelete`, `init` / `noinit` (initialization, against the style default), `together` (manual group
  member), `hidden` (the cell itself closed).
- Output-block attributes: `n=N` (Out number), `size=WxH` (graphic display size in points). One output block per
  shown statement value; Print and Message lines of a statement go in the block of that statement's value (or in a
  block without `n` if the statement shows no value). Consecutive lines with the same prefix form one generated cell
  (as today); a multi-line value is consecutive `#=` lines.
- Header attributes: `grouping=manual`, `magnification=1.25`, `kernel-threads=N`. Unknown attributes are kept and
  written back unchanged (forward compatibility).
- Typeset cells hold canonical box text (4.4) in their body.

### 7.3 Example

```
#! mathcas 2 notebook

#% title
# Cooling of a sphere

#% section
# Model

#% text
# Newton's law with a temperature-dependent coefficient.

#% cell label=1 typeset
k = ((3)/(4))
T(t) = T0*exp(-k t)
#% output n=1
#= 3/4

#% cell label=3
plot(T(t) /. T0 -> 90, [t, 0, 10])
#% output n=3 size=360x225
#= image("cooling.files/plot-3f2a91.png")

#% section closed
# Appendix

#% code label=4
fn helper(x) = x^2
```

### 7.4 Compatibility and invariants

- **Reading old files:** every file valid today reads with the same meaning: `#% cell`/`#% text`/`#% output` without
  attributes are Input, Text and an output block; unlabelled; automatic grouping.
- **Writing:** a notebook that uses only Input and Text cells, no labels and at most one value per cell is written in
  exactly today's format, so old readers (`mc-repl`, the Jupyter wrapper's converter) read it. Otherwise old readers see
  new markers as comments: the file still runs as a script (decision 2.6 holds: every new line is a comment), though an
  old notebook reader may merge cells.
- **Canonical layout:** one blank line after each cell; attributes in a fixed order (`label`, style marks, state
  marks); printing a parsed notebook gives back the same text (round-trip test over all example notebooks and
  generated ones).
- **Diffability:** evaluation changes only output blocks and `label=` attributes; a preference "Save without outputs"
  writes no output blocks and no labels (clean diffs for version control) *(MathCAS)*.
- **Script view:** headings, text and outputs are comments, so `mc-repl notebook.mcn` keeps working; `--update` writes
  outputs in the new format (the runner moves to the shared format crate, P1).
- **Hand editing:** an unknown kind or a malformed attribute is reported with its line and kept as a Text cell, so a
  typo never loses content.

### 7.5 Images and sidecar files

Plot images live in the system temporary folder while a session runs. On save, every image referenced by an output is
copied to a sidecar folder `<notebook name>.files/` next to the notebook under a content-hash name, and the output
line is rewritten to the relative path (`#= image("cooling.files/plot-3f2a91.png")`). Unreferenced files in the sidecar
folder are deleted on save. Opening resolves relative image paths against the notebook's folder. A missing image shows
a placeholder frame with the path. (Embedding images as base64 lines was rejected: it destroys diffability.)

## 8. Themes

### 8.1 Palettes

The Atom One Light and One Dark syntax and UI themes; values are Atom's (`one-light-syntax`, `one-dark-syntax`,
`one-light-ui`, `one-dark-ui`). v1's `theme.rs` palette matches them and is reused with the corrections marked ✱.

| Token | One Light | One Dark |
|---|---|---|
| background (notebook) | `#fafafa` | `#282c34` |
| panel (toolbar, menus bar, status bar) | `#eaeaeb` | `#21252b` |
| popup / card | `#ffffff` | `#2f343f` |
| border | `#dbdbdc` | `#181a1f` ✱ (v1: `#3e4451`) |
| text (mono-1) | `#383a42` | `#abb2bf` |
| weak text (mono-2) | `#696c77` | `#828997` ✱ (v1: `#7f848e`) |
| faint text (mono-3) | `#a0a1a7` | `#5c6370` |
| hue-1 cyan | `#0184bc` | `#56b6c2` |
| hue-2 blue | `#4078f2` | `#61afef` |
| hue-3 purple | `#a626a4` | `#c678dd` |
| hue-4 green | `#50a14f` | `#98c379` |
| hue-5 red | `#e45649` | `#e06c75` |
| hue-5-2 dark red | `#ca1243` | `#be5046` |
| hue-6 orange | `#986801` | `#d19a66` |
| hue-6-2 yellow | `#c18401` | `#e5c07b` |
| accent | `#526fff` | `#528bff` |
| cursor | `#526fff` | `#528bff` |
| selection | `#e5e5e6` | `#3e4451` |
| current line (caret cell tint) | `#f2f2f2` (derived: text at 5 %) | `#2c313c` |
| gutter (labels, brackets) | `#9d9d9f` | `#636d83` |
| guide (insertion line) | `#c8c8ca` (MathCAS: darker than Atom's indent guide so the line is visible) | `#3b4048` |

### 8.2 Roles

| Role | Token |
|---|---|
| plain text, output text | text |
| keyword | hue-3 |
| defined global name (incl. library) | text |
| undefined global name | hue-2 |
| local variable | hue-1 |
| pattern variable | hue-1, italic |
| constant (`true` ...), number | hue-6 |
| string | hue-4 |
| comment | faint, italic |
| operator, punctuation | text |
| syntax error | hue-5 on 12 % hue-5 background |
| unmatched bracket | hue-5-2, bold |
| shadowing, excess argument, unknown option | hue-5-2 |
| matching bracket highlight | 25 % accent background |
| alias text and marker | hue-3; unknown alias hue-5 |
| placeholder | faint; selected placeholder: accent |
| In / Out labels | gutter |
| cell bracket | gutter; hover: accent; selected: accent fill at 35 %; queued: hue-6-2; evaluating: accent, 2 px; error: hue-5; closed mark: small filled triangle at the bottom hook |
| insertion line | guide (2 px); "+" button: gutter outline, accent on hover |
| caret | cursor, 2 px |
| text selection | selection (light: accent at 22 % for contrast, as v1) |
| Message cell text | hue-5-2 |
| failure value | hue-5-2 |
| Print cell text | text |
| Code cell background | panel at 50 % over background |
| Section rule, Program rules | border |
| Title / Chapter / Section / Subsection text | text; Section hue-2 ✱ (the reference colours section headings with its accent; Atom blue keeps that cue) |
| Item bullet | hue-2 |
| large-output frame | border; buttons as UI buttons |
| graphic selection frame and handles | accent |
| find matches | hue-6-2 at 30 %; current match: accent at 35 % |

Images are drawn on their own background; in dark mode a plot with a white background keeps it (no inversion).

### 8.3 Following the system

`View ▸ Theme ▸ System / Light / Dark`; the default is System, which follows the Windows app theme
(`egui::ThemePreference::System`, updated live when the OS setting changes). The choice is saved in the settings file
(`%APPDATA%/MathCAS/notebook.json`).

## 9. Technology and architecture

### 9.1 Constraints and what is available

Rust; Windows first; must build offline. The local cargo caches hold (checked November 2026): `eframe`/`egui`/
`epaint`/`egui-winit`/`egui_glow`/`egui-wgpu`/`egui_extras` 0.37.0, `winit` 0.30.13, `wgpu` 30, `glow` 0.17,
`accesskit` 0.25 with `accesskit_windows`, `arboard` 3.7, `clipboard-win` 5.4, `rfd` 0.17 (file dialogs), `png`
0.18, `image` 0.25, `resvg`/`usvg` 0.49, `tiny-skia` 0.11/0.12, `vello_cpu`, `ttf-parser` 0.25, `skrifa`, `harfrust`
(text shaping), `windows` 0.62, `serde_json`. Not available: iced, slint, GTK, any web view (wry/tao), druid, xilem,
floem.

**Spike (outside the repository):** a minimal eframe 0.37 application with `glow`, `accesskit`, `clipboard-win`,
`resvg` and `ttf-parser` passes `cargo check --offline` in 33 s from the caches, and `egui::Context::system_theme` /
`ThemePreference` exist. egui 0.37 shapes text with `harfrust` and reads fonts with `skrifa`, so OpenType (CFF) fonts
such as STIX Two Math load as egui fonts, which v1 (egui with `ab_glyph`) could not rely on.

### 9.2 Options

| Option | For | Against |
|---|---|---|
| **A. eframe/egui 0.37** (glow backend), notebook canvas as one custom-painted, virtualised widget; egui for menus, toolbar, dialogs, popups | in the caches; v1 built a working typeset notebook with it; AccessKit integration; system theme; shaping; immediate mode makes state-to-pixels simple; fast to iterate | immediate-mode text editing widgets are not enough (MathCAS writes its own editor anyway); per-frame layout must be cached by hand; menus are in-window, not native |
| B. winit + wgpu (or `vello_cpu`/`tiny-skia`) + own retained UI | full control of rendering and text | menus, dialogs, popups, focus, IME, accessibility all from scratch: months before the notebook part starts |
| C. Native Win32 (`windows` 0.62): native menus and dialogs, Direct2D/DirectWrite canvas | native look; best text rendering on Windows | Windows-only forever; a large unsafe API surface; the canvas is custom anyway; no reuse of v1 |
| D. Web view (HTML/JS front end) | rich text and math libraries | not available offline; a second language; against the single-binary dist |

### 9.3 Recommendation

**Option A: eframe/egui 0.37 with the glow backend**, wgpu as a compile-time alternative. The notebook document is not
built from egui widgets: one custom widget lays out and paints visible cells from cached layouts and routes all keys
through MathCAS's own key table, so egui's limits in text editing do not matter, and the model stays testable without
any UI. egui supplies what would otherwise cost the most: windows, menus, dialogs, popups, IME, clipboard, file
dialogs (`rfd`), AccessKit and theme following. If egui ever limits the canvas, only `mc-notebook`'s painter changes,
because layout lives in `mc-boxes`.

### 9.4 What v1 taught, and what is reused

v1 (`MathCAS/crates/notebook`, egui; `ext/src/eqedit.rs` editor; `crates/texmath`) worked: cell styles, right-hand
brackets, Shift+Enter, Alt+digit styles, an always-on structural editor for fractions, radicals and scripts, Esc
aliases, a keymap table with contexts and a generated help window, Atom themes, TeX math rendered from STIX Two Math,
and a scripted self-test with screenshots.

What did not work:

- `app.rs` grew to 2,600 lines mixing model, layout, input and kernel: behaviour could only be tested through the UI.
  v2 separates the model (pure, tested with key scripts) from painting.
- The kernel ran in-process on a thread: a runaway evaluation or a crash took the window with it, and memory could not
  be reclaimed. v2 uses the separate `mc-kernel` process.
- The editor's nodes were only `Frac`, `Sqrt`, `Sup`, `Sub`, with scripts attached to "the preceding unit"; no grids,
  under/overscripts, placeholders or templates; saved notebooks stored source text only, so structures came back as
  linear text after reloading. v2's box model (4.1) has script bases as boxes, placeholders and grids, and the
  canonical text (4.4) round-trips.
- No word wrap in the editor, no double-click word selection, copying structures yielded source text only.
- Typeset math was rasterised to textures per formula (`tex.rs`), which made editing slow and blurry at fractional
  scales; v2 lays out boxes with MATH metrics and draws glyphs through egui's text pipeline (cached glyph atlas, crisp
  at any scale).
- Painting state lived in thread-locals (`math.rs`); v2 passes a paint context.

Reuse:

| v1 piece | Decision |
|---|---|
| `texmath/src/font.rs` (OpenType MATH table: constants, glyph variants, assemblies) and the STIX Two Math font with its OFL text | **port** into `mc-mathfont` (it depends only on `ttf-parser`) |
| `texmath/src/layout.rs` (Appendix G rules) | **port the rules** into `mc-boxes` layout over a metrics trait; drop its TeX-source parser from the notebook path (keep it for "Copy as LaTeX" checks and plot labels later) |
| `notebook/src/theme.rs` | **reuse** the palette with the corrections of 8.1 |
| `notebook/src/input.rs` (key table with contexts, routing precedence, help generation, visual line motion, hit-testing) | **reuse the design**, rewrite against v2's command set |
| `ext/src/eqedit.rs` | **do not reuse the code** (model too narrow); reuse its tests' cases as starting vectors |
| `app.rs`, `notebook.rs`, `worker.rs`, `math.rs`, `tex.rs` | do not reuse |
| self-test harness (events injected through egui's raw input hook, screenshots) | **reuse the approach** (9.8) |

### 9.5 Crates

```
mc-notebook (bin, eframe)          window, chrome, painter, input routing, kernel process client, themes, clipboard,
   |                                accessibility, settings
   +-- mc-nbmodel                   document model: cells, styles, grouping, selection, commands, key table, undo,
   |      |                         evaluation queue state machine, clipboard fragments (no UI, no kernel)
   |      +-- mc-boxes              box tree, caret/selection in boxes, structure commands, canonical text <-> boxes,
   |      |      |                  printed output -> boxes, aliases table, layout and line breaking over a metrics
   |      |      |                  trait (no UI)
   |      |      +-- mc-syntax      lexer/parser (existing)
   |      |      +-- mc-mathfont    OpenType MATH data, bundled STIX Two Math
   |      +-- mc-nbfile             .mcn read/write (extracted from mc-kernel)
   +-- (spawns) mc-kernel           protocol server (existing; depends on mc-nbfile for the runner)
```

`mc-notebook` links `mc-syntax` (pure parsing) but never `mc-engine`: evaluation only happens in the kernel process.
The dependency check (`tools/depcheck.ps1`) gets a rule for this.

Model sketch (Rust):

```rust
pub struct Notebook { cells: Vec<Cell>, header: Header, groups: GroupTree /* derived */, sel: Selection,
                      undo: UndoStack, queue: EvalQueue, rev: u64 }
pub struct Cell { id: CellId, style: Style, content: Content, attrs: Attrs, label: Option<Label>,
                  outputs_of: Option<CellId> /* generated: its input */, rev: u64 }
pub enum Content { Text(String), Boxes(boxes::Row), Generated(Generated) }
pub enum Command { Insert(String), Move(Motion, Extend), Structure(BoxKind), SetStyle(Style), Evaluate, ... }
pub fn apply(nb: &mut Notebook, cmd: Command, env: &mut dyn Env) -> Effects;  // Env: clipboard, layout queries
pub trait KernelLink { fn send(&mut self, r: Request); fn poll(&mut self) -> Vec<Response>; }
```

Every key, menu item and mouse action becomes a `Command`; the UI never mutates the model directly.

### 9.6 Rendering and performance

- **Virtualisation:** each cell caches its layout (height, line boxes, glyph runs) keyed by (content revision, width,
  magnification). Cell tops are prefix sums in a Fenwick tree, so scrolling, hit-testing and "scroll to cell" are
  O(log n). Only cells intersecting the viewport (plus one screen of margin) are laid out and painted per frame.
- **Edits** invalidate one cell's layout; grouping recomputation is O(n) and only changes bracket geometry.
- **Text** is drawn as egui galleys (shaped and cached by egui); box layout positions galleys and draws rules and
  stretchy glyphs as shapes.
- **Budgets** (release build, 1,000 cells of mixed content, 4K window): keystroke to frame < 16 ms; scrolling 60 fps;
  open + first paint < 1 s; save < 100 ms; memory < 300 MB without images. A benchmark notebook generator and a
  frame-time probe in the self-test check these (M2 exit).
- **Background work:** image decoding, SVG rasterisation, typesetting of large outputs and syntax analysis of huge
  cells run on a worker pool (`std::thread`), results posted back with repaint requests.
- **Idle:** no repaint when nothing changes except the caret blink (two repaints per second).

### 9.7 Accessibility

- Every visible cell is an AccessKit node: role `heading` with level for Title ... Subsubsection, `paragraph` for Text,
  `text input` (multi-line) for Input/Code named by its style, `status`-like read-only text for outputs (name
  `Output out(3)`, value = printed linear text), `log` for Print and Message, `image` for pictures (built in round 11;
  the labels are no longer drawn, see 2.6). Brackets and the insertion point are reached from the keyboard through
  commands (no mouse needed for any action); the menus open with F10 and Alt+letter.
- Typeset content exposes its linear input text (4.6) as value, so screen readers read `(a + b)/(2c)`.
- Live region: "Evaluation finished" / "Kernel stopped" announcements.
- Themes: text and plain-code contrast are WCAG AA in both themes; comments (faint) are below AA, so a preference raises
  comment contrast. Magnification 50 % - 300 % (`Ctrl+wheel`, `Ctrl+Alt+=`/`Ctrl+Alt+-`; `Ctrl+-` is subscript).
- Caret blink can be turned off; animations: none besides blink and the flash of matched brackets.

### 9.8 Testing

- **Key scripts (main tool):** `crates/mc-nbmodel/tests/keys/*.keys` files: an initial `.mcn` with `▮` marking the
  caret (or `⟦ ⟧` a selection, `‖` an insertion point), a key sequence, and the expected `.mcn` with selection marks.
  A runner applies the keys through the same key table the UI uses, with a fake layout environment (fixed-width
  metrics) and a fake kernel (scripted responses). Every *(observed)* vector in this document becomes a script.
- **Format:** round-trip tests over generated notebooks (property tests on canonical layout) and all example files.
- **Boxes:** canonical text round-trip properties; structure-command vectors (4.2); layout snapshot tests as text
  (box positions rounded to 0.1 pt) for a fixed metrics source.
- **Kernel link:** integration tests start the real `mc-kernel` and run notebooks (as `mc-repl` tests do).
- **Golden screenshots:** the self-test mode (environment variable) drives the real window through egui's raw input
  hook and saves screenshots for a fixed set of notebooks in both themes at 100 % and 150 %; comparison with a
  per-pixel tolerance. They run locally on Windows (GPU/GL needed), not in CI; CI runs everything else.

## 10. Window chrome

The chrome is MathCAS's own; it is listed so that the menus expose every command of section 3.

### 10.1 Menus

- **File:** New (`Ctrl+N`), Open... (`Ctrl+O`), Open recent ▸, Save (`Ctrl+S`), Save as... (`Ctrl+Shift+S`), Save
  without outputs, Export ▸ (script `.mc`, later HTML/PDF), Close (`Ctrl+W`), Exit.
- **Edit:** Undo, Redo, Cut, Copy, Copy as ▸ (Plain text `Ctrl+Shift+C`, Input text, LaTeX (M7)), Paste, Clear,
  Extend selection (`Ctrl+.`), Select all, Check balance (`Ctrl+Shift+B`), Un/comment (`Alt+/`), Complete selection
  (`Ctrl+K`), Make template (`Ctrl+Shift+K`), Find... (`Ctrl+F`), Find next/previous (`F3`/`Shift+F3`), Use selection
  for find (`Ctrl+E`), Preferences.
- **Insert:** Input from above (`Ctrl+L`), Output from above (`Ctrl+Shift+L`), Cell with same style (`Alt+Enter`),
  Typesetting ▸ (the 4.2 commands with their keys), Table/Matrix ▸ (New..., Add row `Ctrl+Enter`, Add column `Ctrl+,`),
  Special character... (a searchable grid over the alias table showing each alias).
- **Format:** Style ▸ (all styles of 2.1 with `Alt+1` ... `Alt+9`, Other... `Alt+0`), Theme ▸ (System, Light, Dark),
  Magnification ▸ (50 % - 300 %).
- **Cell:** Convert to ▸ (Linear input text `Ctrl+Shift+I`, Typeset `Ctrl+Shift+N`, Traditional `Ctrl+Shift+T`),
  Cell properties ▸ (Open, Editable, Evaluatable, Deletable, Initialization `Ctrl+8`), Grouping ▸ (Group together
  `Ctrl+Shift+G`, Ungroup `Ctrl+Shift+U`, Manual / Automatic grouping, Open all subgroups `Ctrl+{`, Close all subgroups
  `Ctrl+}`, Open/close group `Ctrl+'`), Divide cell (`Ctrl+Shift+D`), Merge cells (`Ctrl+Shift+M`), Delete all output,
  Show expression (`Ctrl+Shift+E`).
- **Evaluation:** Evaluate cells (`Shift+Enter`), Evaluate in place (`Ctrl+Shift+Enter`), Evaluate notebook, Evaluate
  initialization cells, Abort (`Alt+.`), Remove from queue (`Alt+Shift+.`), Interrupt... (`Alt+,`), Find evaluating
  cell, Start kernel, Quit kernel, Restart kernel, Kernel log.
- **Window:** Show toolbar, Full screen (`F12`), list of open notebooks.
- **Help:** Find selected function (`F1`: opens `docs/functions/<name>.mcn` or the library usage), Keyboard shortcuts
  (generated from the key table), Aliases (the table), About.

### 10.2 Toolbar

One row, can be hidden: the **style** drop-down showing the style of the current cell (choosing applies it, like
Alt+digit), **Evaluate**, **Abort**, a **kernel indicator** (grey: not started, green: idle, amber: busy with the queue
length, red: stopped; click for start/quit/restart), **theme** toggle, **magnification** drop-down.

### 10.3 Status bar

Kernel state and version (`hello.engine`), queue length, time of the last evaluation (`done.millis`), the current
cell's style and label, caret line:column for code cells, file path and "modified".

### 10.4 Context menus

- **Cell bracket:** Cut, Copy, Paste, Copy as ▸, Evaluate cell, Convert to ▸, Style ▸, Divide cell, Merge cells,
  Group/Ungroup, Open/close group, Delete cells, Show expression.
- **Inside a cell:** Cut, Copy, Paste, Copy as ▸, Style ▸, Divide cell, Evaluate in place, Check balance, Un/comment,
  Get help (F1).
- **Graphic:** Copy image, Copy as text, Save image as..., Reset size.
- **Notebook background:** Paste, Evaluate notebook, Save.

### 10.5 Dialogs

Open/save (`rfd` native dialogs), Preferences (theme, fonts, magnification, kernel path and threads, output size
limit, auto-closing brackets, operator rendering, completion delay, save without outputs), Other style, Special
character, Interrupt, Initialization cells, Quit kernel confirmation, unsaved-changes prompt on close.

## 11. Milestones

Each milestone ends with tests that run in CI (key scripts, format, kernel link) plus a short manual or golden
screenshot check on Windows.

### M1: plain-text notebook over the kernel, themes

Scope: `mc-nbfile` extracted (today's format); `mc-nbmodel` with Input and Text cells (other styles read and shown as
Text-like), caret and text selection, insertion point and new cells, single-level brackets, cell selection by click
and shift-click, undo/redo, plain-text clipboard; `mc-notebook` window with virtualised painting, Consolas/Segoe UI,
One Light/One Dark/System; kernel process with the protocol additions `values: "each"`, `numbered`, `n`, `first`/`next`
(P2 part 1); queue, labels `In[n]:=`/`Out[n]=`, Print/Output/Message cells (linear text), abort, quit/restart; open,
save, save as.

Exit tests: key scripts for typing, new cell at insertion point, Shift+Enter flow with a fake kernel (labels and
generated cells replaced on re-evaluation, numbering `In[2]`/`Out[2]`/`Out[3]`/`In[4]`), undo of typing and of cell
creation; integration test: open `examples/notebook.mcn`, evaluate all, save, compare with `mc-repl --update` output
modulo labels; manual: both themes, system switch followed live.

### M2: the document

Scope: all styles and Alt+digit/Alt+0; automatic grouping and nested brackets; open/close; manual groups; cell
operations (divide, merge, input/output from above, same style below, delete all output, show expression); cell
clipboard with private flavour; drag and drop of cells; cell properties; initialization cells; file format extensions
(7.2) with round-trip; Print/Message per call (P2 part 2); 1,000-cell performance budget.

Exit tests: every grouping vector of 2.3 and open/close vector of 2.4 as key scripts; divide/merge/duplicate vectors of
3.8; format round-trip over generated notebooks; perf probe on a generated 1,000-cell notebook meets 9.6 budgets.

### M3: code editing

Scope: syntax colouring (all classes except signature-based ones), bracket matching and check balance, extend
selection, completion popup with kernel names, aliases (character aliases and the full table, not templates), Unicode
operator equivalents in the lexer (P7), operator rendering and spacing, visual indentation and line wrapping of
inputs, find, comment toggle.

Exit tests: extend-selection vectors (3.4); alias scripts for every kind of 3.9; colouring snapshot tests (class per
token) for a corpus of example cells; completion ranking tests.

### M4: graphics and large outputs

Scope: inline PNG, graphic selection and resizing, sidecar files on save, large-output summary with show more/less,
line breaking of linear outputs.

Exit tests: save/reopen with images (paths rewritten, files copied, unreferenced removed); resize script stores `size=`;
a 5 MB output shows the summary and copies in full; wrap tests at three widths.

### M5: typeset output

Scope: `mc-mathfont`, `mc-boxes` layout (fractions, scripts, radicals, under/over, grids, big operators, stretchy
delimiters), printed output -> boxes (4.5), output line breaking inside boxes, traditional display of outputs, copy as
plain text/input text from outputs; editing an output duplicates it as typeset input.

Exit tests: printed-to-boxes vectors for every row of 4.5; layout snapshot tests; golden screenshots of a typeset
output notebook in both themes.

### M6: two-dimensional input

Scope: the box editor (caret and selection in boxes, structure commands of 4.2, Ctrl+Space, Tab placeholders,
Ctrl+Enter/Ctrl+, grids, Ctrl+5), templates and template aliases, canonical text round trip (4.4) and the `typeset`
mark, conversions `Ctrl+Shift+I/N/T`, make template, evaluate in place, the new library heads `subscript`,
`overscript`, `underscript`, `underoverscript`, `root` with their `docs/functions` pages.

Exit tests: every 4.2 vector as a key script; placeholder/Tab vectors; property tests `text(boxes(t)) == t` and
`boxes(text(b)) == b`; conversion round trips; a typeset cell evaluates to the same value as its linear conversion.

### M7: polish

Scope: SVG/vector graphics via resvg, accessibility audit (Narrator walk-through), text drag and drop, image drops,
signature-based colouring (excess arguments, unknown options), Copy as LaTeX, preferences dialog complete, Linux
build check.

Exit tests: accessibility tree snapshot test; SVG golden screenshots; a Narrator checklist recorded in the milestone
notes.

## 12. Work packages and parallelism

No two packages own the same crate at the same time (`06-roadmap.md` rule).

| Package | Crates owned | Depends on | Milestone |
|---|---|---|---|
| P1 format | new `mc-nbfile`; `mc-kernel/src/notebook.rs` moves there (mc-kernel and mc-repl switch to it) | none | M1 (today's format), M2 (extensions) |
| P2 kernel protocol | `mc-kernel` (protocol, server, cell), `mc-engine` (per-statement values, line counter, history, `out`), `mc-plot` (display hint), library warnings as messages | none | M1 (each/numbered), M2 (print per call, messages, names), M4 (display) |
| P3 model | new `mc-nbmodel` | P1, P4 (M6 only: before that, content is text) | M1-M6 |
| P4 boxes | new `mc-boxes` | P5, mc-syntax | M5, M6 (can start at M1 in parallel) |
| P5 math font | new `mc-mathfont` (port of v1 `font.rs`, STIX Two Math, OFL) | none | M5 (can start at M1) |
| P6 app | new `mc-notebook` | P3, P4, P5 | M1-M7 |
| P7 syntax | `mc-syntax` (Unicode operator equivalents, scope data for colouring, `□` diagnostic), `04-syntax.md` | none | M3 |
| P8 library heads | the crate that owns inert heads (`mc-engine` registration) + `docs/functions/*.mcn` | P2 done with mc-engine | M6 |

Order: P1, P2, P5 and P7 start together; P3 and P6 start with M1 once P1's API is fixed (a day's interface agreement:
`mc-nbfile` types); P4 starts in parallel with M1-M2 (pure code, tested alone) and joins at M5. P2 and P8 both touch
`mc-engine` and therefore run one after the other. Dist: `tools/dist.ps1` adds `mc-notebook.exe`; the installer adds
a Start menu entry and the `.mcn` file association (opening a `.mcn` launches the notebook).

## 13. Behaviours not determined, with best guesses

To check by hand in the reference front end; each guess is what this spec implements until corrected.

1. **History syntax.** The reference has `%`, `%%`, `%n` for previous outputs. *Guess/decision needed:* add `%` forms
   to MathCAS syntax as sugar for `out()`; until decided, only `out(n)` exists.
2. **Label after editing.** Does typing into an evaluated input remove its `In[n]:=` label immediately? Programmatic
   edits kept it. *Guess:* kept until re-evaluation.
3. **Where an edited output's duplicate goes.** *Guess:* a new Input cell right after the output's group, caret in it.
4. **Typing with cells selected by bracket.** *Guess:* ignored (only cell-level keys act). Alternative: replaces the
   cells or creates a new cell after them.
5. **Paste with cells selected.** *Guess:* replaces the selected cells.
6. **Ctrl+click on brackets.** *Guess:* toggles one bracket in a non-contiguous cell selection.
7. **Up/Down/Left/Right across cells.** *Guess:* every gap between cells is a stop (insertion point) for Up/Down and
   for Left/Right at a cell's ends.
8. **Tab without placeholders.** The command did nothing when sent programmatically; *guess:* the key indents
   (inserts indentation) as the menu's "indent selected lines (Tab)" suggests.
9. **Auto-closing brackets and quotes.** *Guess:* the reference does not auto-insert closing brackets in input cells;
   MathCAS offers it as a preference, off by default.
10. **Operator substitution at the caret.** *Guess:* `->` drawn as → behaves as one unit for the caret, Backspace
    deletes both characters.
11. **Unknown alias.** *Guess:* stays as marked text in the error colour until edited.
12. **Bracket marks.** Which small marks the reference draws on brackets for closed groups, non-editable cells,
    generated cells. *Guess:* only the closed-group triangle (8.2).
13. **Queued and running indication.** *Guess:* queued brackets tinted, running bracket emphasised, running label
    `In[*]:=` until the first response.
14. **Text sent for a queued cell.** *Guess:* the content when its evaluation starts, not when it was queued.
15. **Abort and the queue.** *Guess:* abort also clears the queue.
16. **Divide with a selection.** *Guess:* splits before and after the selection (up to three cells).
17. **One kernel per window.** The reference shares one kernel between notebooks by default. *Guess/decision:* one per
    window in MathCAS (isolation); a preference "share one kernel" can come later.
18. **Output continuation lines.** *Guess:* wrapped output lines start at the cell's left edge (inputs are indented).
19. **Alt+digit at an insertion point.** *Guess:* sets the style of the cell typing will create.
20. **Completion delay.** The reference opens the popup immediately; *guess:* MathCAS waits 300 ms after two
    characters to avoid flicker; set to 0 if that feels different.
21. **Selection dragged across cells.** *Guess:* turns into a cell selection of the cells touched.
22. **Rational size in outputs.** *Guess:* inline rationals in products are drawn at script size (as `½`-like small
    fractions); standalone rationals at full size.
23. **Graphic side handles.** *Guess:* keep the aspect ratio unless Shift is held.
24. **Escalation of Ctrl+A.** *Guess:* first all of the cell's content, a second Ctrl+A all cells.

## 14. Decisions

- **NB.1** The notebook part follows the reference behaviour exactly; chrome and colours are MathCAS's (Atom One).
- **NB.2** egui/eframe 0.37 (glow) with a custom virtualised notebook canvas; model and box layout are UI-free crates.
- **NB.3** The kernel is always a separate `mc-kernel` process, one per notebook window, started on first evaluation.
- **NB.4** Protocol 1 grows compatibly: per-statement values, kernel-owned line numbers and history, display and
  message responses, a `names` request.
- **NB.5** The notebook owns the evaluation queue and sends one `eval` at a time.
- **NB.6** Typeset input is stored as canonical box text: valid MathCAS where every box is a double-parenthesised
  fixed shape; lossless both ways; cells marked `typeset`.
- **NB.7** Typeset output is derived from the printed text on the notebook side; the kernel sends no box format.
- **NB.8** `.mcn` grows by attributes on marker lines and new kinds; old files read unchanged; simple notebooks are
  written in today's exact format.
- **NB.9** Images are saved in a `<name>.files/` sidecar folder with relative paths, not embedded.
- **NB.10** Indentation in input cells is visual; files store exactly what was typed.
- **NB.11** Every behaviour is a `Command` on the model and tested with key scripts; golden screenshots are local.
- **NB.12** v1's MATH-table code, STIX Two Math, palette and keymap design are reused; its editor and app code are not.
- **NB.13** Implicit multiplication in typeset input (user decision): where a box would touch an operand, the box
  editor inserts a visible `*` (`x*√Q`, `√Q*y`), so canonical text never reads as a call; a number before a box
  needs none (`2√2`).
