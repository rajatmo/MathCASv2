# `mc-strings` (roadmap item 1C)

String functions, Unicode-correct. Depends on `mc-api`, `mc-core`, `mc-eval`, `mc-syntax` (dev: `mc-engine`,
`mc-bench`). API: `install(k, h)`, `FUNCTIONS` (15 entries).

## What was built

- `ctx`: symbols, argument checks, failures, options, fuel.
- `search`: `string_split`, `string_replace`, `string_contains`, `string_starts`, `string_ends`.
- `shape`: `string_take`, `string_drop`, `characters`, `from_characters`, `string_reverse`, `to_upper`, `to_lower`,
  `string_pad`, `string_trim`.
- `read`: `to_expression`.

Forms: `string_split(s)` at white space, `(s, sep | [seps])` keeping empty pieces (k separators give k + 1), `(s, sep,
n)` at most n pieces; `string_replace(s, a -> b | a :> b | [rules])`, `(s, rules, n)`, `(s, a, b)`; `string_take` /
`string_drop` with `k` (negative from the end) or `[i, j]` / `[i]` (1-based, negative from the end; `[i, i-1]` empty);
`string_contains`/`starts`/`ends` with one string or a list; `string_pad(s, n[, pad], side: right|left|center)`;
`string_trim(s[, sub])`.

## Decisions

- **S.1** Characters are Unicode scalar values, never bytes (no grapheme segmentation); case uses the full mapping
  (`straße` to `STRASSE`; final sigma lowers to `ς`).
- **S.2** A symbol or compound where a string is expected leaves the call unevaluated; another atom is
  `failure(argument, f, k)`; out-of-range positions `failure(index, i, n)`; a list in the string position maps (except
  `from_characters`); results over 2^30 bytes `failure(limit, memory, call)`; unknown options
  `failure(unknown_option, name)`.
- **S.3** Search strings are literal; several are tried in order, leftmost occurrence wins, no overlaps; separators and
  search strings must be nonempty.
- **S.4** `to_expression` reads with `mc_syntax::read` and hands the term to the caller to evaluate; several statements
  become one `block`, empty text `null`; a syntax error is `failure(syntax, "line:col: message")`; names are not
  resolved through modules or imports (no read-time context at run time).
- **S.5** Fuel charged by bytes; scans poll the interrupt every 64 KB.

## Tests

`tests/strings.rs` (9, about 150 cases): splitting, replacing, take/drop (Unicode, index failures), searching, case and
characters with reversal, padding and trimming, `to_expression` (including a definition read at run time), long
strings (300 KB, 10^5 pieces, a size-limit failure), identical at 1 and 4 threads under collection stress.

## Known gaps

- `mc-lists`' `split` duplicates `string_split`'s basic forms (one name per concept would retire or alias `split`).
- `string_join`, `string_length`, `to_string` stay in `mc-lists`.
- ~~No patterns in search strings~~: done in round 30 (below).

## Round 29

- `string_expression(p...)` joins literal strings (`"a" ~~ "b"` is `"ab"`) and stays symbolic around patterns;
  `longest` and `shortest` are inert markers. The search functions still take literal search strings (decision S.3):
  a string pattern language (blanks, repetition, `digit_character`, alternatives) is the missing part that these
  three names need to matter, and would be a new matcher module.

## Round 30: string patterns

Modules: `strpat` (pattern terms read as string patterns, the matcher), `regex` (`regular_expression`), `scan`
(the searching functions). New functions: `string_cases`, `string_position`, `string_count`, `string_match_q`,
`string_free_q`, `string_delete`, `regular_expression`, and the pattern symbols `digit_character`,
`letter_character`, `whitespace_character`, `word_character`, `punctuation_character`, `hexadecimal_character`,
`whitespace`, `number_string`, `start_of_string`, `end_of_string`, `start_of_line`, `end_of_line`,
`word_boundary`. `string_replace`, `string_split`, `string_contains`, `string_starts`, `string_ends` take
patterns too; their literal forms keep the old fast paths.

- **S.6 Greedy by default.** Runs (`__`, `___`) and repetitions take the longest text first, as in regular
  expressions (term patterns take the shortest run first, semantics 6.2: the two differ on purpose). `shortest(p)`
  flips that inside `p`; `longest(p)` makes it explicit (the parts before it keep their order: `x__ ~~ longest(__)`
  on `"abcdefg"` gives `x = "abcdef"`).
- **S.7 First match by backtracking.** Alternatives in order, runs by length in the direction of S.6; the first
  complete match wins (the backtracking NFA semantics, Friedl, *Mastering Regular Expressions*, ch. 4). Variables
  bind strings; a variable bound twice must bind equal text (`x_ ~~ x_`). Conditions (`p when c`) are evaluated
  with the bindings made so far (through `replace([texts], [any(x), ...] :> c)`, so substitution is hygienic).
- **S.8 Limits.** Fuel per step, interrupt polled; a repetition of one character (or a class) is a run and never
  nests; repetitions of compound patterns nest at most 4000 deep, past that `failure(limit, string_pattern)`.
- **S.9 Regular expressions** are read into the same patterns: `.` `[...]` `[^...]` `\d \w \s` (and negations)
  `^ $  \A \z`, groups (bound to `$1`, `$2`, ...; in a string right side `$k` is replaced, `$0` is the match),
  `(?:...)`, `|`, `* + ? {m,n}` greedy or lazy. Look-around, back references and flags are not read (the call stays
  unevaluated). No regex crate: none is vendored, and one matcher keeps the two forms consistent.
- **S.10 Searching.** A spec is a pattern, a rule, or a list (a list of patterns only is one alternative pattern).
  Matches do not overlap unless `overlaps: true | all`; `string_position` overlaps by default (`[start, end]`,
  1-based, inclusive). `ignore_case: true` compares case-insensitively. A rule's right side is evaluated with the
  variables bound; `string_replace` joins the pieces with `string_expression`, so a right side that is not a string
  leaves `string_expression(...)` (this replaces the old `failure(argument, string_replace, 2)` for `"a" -> 1`).
  `string_match_q` reads `*` and `@` in a literal string as wildcards. `string_split` with a pattern keeps empty
  pieces like the literal form (S.3); `p -> v` puts `v` between the pieces.
- Lists in a pattern are alternatives (`characters("aeiou")`, `character_range("a", "f")`); alternatives of single
  characters become one character class.
- Exceptions: `x_ when not match_q(x, c)` and `x @ p when not match_q(x, c)` (how `except(c)`, `except(c, p)` are
  written) match what `p` matches when that text does not match the string pattern `c`. A compound term that is no
  pattern form is evaluated once and read as a pattern (`characters("aeiou")` held in a condition).
- Gaps: `string_replace_list`, `string_replace_part`, `any_order` not done;
  typed blanks (`x_: T`) are not string patterns.
- Tests: `tests/patterns.rs` (5 tests, 60 cases: greedy/lazy, classes, bindings, anchors, regular expressions, the
  searching functions, long texts).
