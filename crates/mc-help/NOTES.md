# mc-help: the help index (round 12, package nb-docs)

One source for the help the editors show: the documentation window's tooltips, the notebook's completion, usage
hint, hover and F1, and the language server's completion, signature help and hover all read `Index::embedded()`.
No dependencies, so the notebook and the language server can use it without the libraries or the docs window.

- **Catalogue table** `data/index.txt`: name, area, topic and the library's one-line description of every function
  the documentation lists (722 at the start of the round). Written by `mc-docs-pages index` from
  `mc_docs::catalogue`; the `mc-docs` test `index::the_help_index_is_current` fails when the catalogue changed and
  the table was not regenerated (the libraries' `FUNCTIONS` lists cannot be read from here without depending on
  every library).
- **Usage lines**: the build script reads every `docs/functions/<name>.mcn` (the text cells between the title and
  the first section: the call in backticks, then what it gives), so pages edited or added are always current.
- `Entry`: `calls()` (the page's usage calls, else the signature at the start of the description), `text_of(k)`,
  `line()` (first call and what it gives: the one-line usage), `is_function()`, `best_call(arg)` (the first call
  with enough parameters, or one with `...`).
- `Index::complete(prefix)` and `score(name, prefix)`: prefix, then a word of the name (after `_`), then subsequence;
  ties: a page first, shorter, alphabetical.
- `scan`: `word_at`, `word_before`, `call_at` / `calls_at` (the calls whose brackets hold a position, innermost
  first, with the argument number; strings, backquoted names and comments skipped; an unclosed call counts),
  `params` and `highlight` of a usage call, `matching_close`.

Tests: 7 unit tests (table reading and writing, calls from pages or descriptions, completion order, the embedded
table, words, calls, parameters).
