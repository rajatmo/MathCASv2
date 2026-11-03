# Package F: `mc-load` (files, modules, imports, the prelude)

Status: built and merged (branch `lib/f-load`, commit 27f27bb; merge 91e785a). `mc-repl examples/<script>.mc` works,
`import` finds module files next to the importing file or in the embedded standard library, and the prelude loads at
engine start. Workspace: 447 passed, 0 failed, 6 ignored; clippy and `cargo doc` clean.

## What was built

- **`crates/mc-load`** (depends on `mc-api`, `mc-syntax` only). `Loader` holds a session's reader state (module table,
  session imports). `Loader::read(store, text, origin, known)` checks the `#!` header, parses and lowers with spans
  (`mc_syntax::read_spanned`), handles `module`/`import` (loading module files through a `SourceProvider`), and
  resolves every name of every statement; it returns `Read { statements, reloaded }` or a `LoadError` and restores the
  old reader state, so nothing is evaluated on an error. Output: plain statements on qualified symbols (a `module`
  becomes its body's definitions plus `null`, an `import` becomes `null`, loaded module files' statements come first);
  evaluation never consults the table.
  - `resolve.rs`: the walk over lowered terms, with `mc-syntax`'s scope helpers on the binding forms lowering uses
    (`fn` closures, blocks with `let`/`var`, `for`, `match` arms, rules, compound `set`/`set_delayed`, `define_fn`), so
    a bound name is never renamed; bounded by the parser's `MAX_DEPTH`.
  - `loader.rs`: module/import statements, file loading, cycle detection, header check, error positions.
  - `modules.rs`: `Module` and import lists. `source.rs`: `Origin`; `SourceProvider` with `FileSystem` (strips a BOM)
    and `InMemory`. `error.rs`: `LoadError` rendered as `file:line:col: message`, source line, caret. `stdlib.rs`:
    `lib/prelude.mc` and `lib/stats.mc` via `include_str!`.
- **Engine:** `CoreEngine` sends every input through the loader and evaluates the prelude after the natives.
- **Command line (`mc-repl`):** scripts, `-e`, `-i`, `--no-prelude`, `--help`; exit codes 0 / 1 (read, resolve, load
  error) / 2 (a top-level failure or engine limit) / 130 (interrupted); REPL commands `:load path`, `:modules`.
- **Repository:** `lib/prelude.mc`, `lib/stats.mc`; `examples/hello.mc`, `examples/modules.mc`, `examples/vectors.mc`;
  `tools/dist.ps1` copies `examples/`; `tools/dist-README.txt` has a section on scripts, modules and the prelude.

## Decisions

F.1-F.9 as in the brief. Refinements:

- **F.10** Lowered terms carry no positions: an error points at the first token of the offending name inside the
  statement's span (`read_spanned` + `mc_syntax::identifiers`), else the statement start.
- **F.11** Inside a module, bare names defined at the top level of the same source count as known though not yet
  evaluated.
- **F.12** The first argument of `failure(...)` (the tag) is never renamed.
- **F.13** Not resolved: types, option names in `f(x, name: v)`, decorator names (their arguments are), pattern
  variable names.
- **F.14** Defining an imported name at the session top level is an error (it would add clauses to the module's
  function, or define a bare symbol every use skips).
- **F.15** Lowering heads and the contract's names (`Head::ALL`, `mc_api::names`, failure tags) always resolve bare;
  defining one in a module is an error (otherwise `+` would resolve to a module's `add`).
- **F.16** Library names without definitions (`pi`, `e`, `euler_gamma`, `catalan`, `inf`) are known: the engine passes
  the libraries' `FUNCTIONS` names plus `inf`.
- **F.17** A `module M` for a name already in the table replaces it: the old members are cleared with `Kernel::clear`
  before the input runs. No reopening or extending.
- **F.18** Top-level imports of REPL input, `-e`, scripts and `:load` files persist for the session; those of a
  module file loaded by `import` stay local to it; imports inside a module body apply to that body only, and session
  imports do not apply inside modules.
- **F.19** "Loaded once" is enforced by the module table.
- **F.20** `M.f` with `M` not loaded is an error ("unknown module ... import it first"); qualified names never load.
  Fresh module symbols (`M.y`) are not members: naming one from outside is an error.
- **F.21** Scripts: `print` output and the last non-null value to stdout; each failure to stderr as
  `origin: failure(...)`; a read error stops later items; `-i` enters the REPL even after an error.
- **F.22** `Engine::eval` reports errors in its own text as `EngineError::Syntax { message, offset }`; an error in a
  loaded module file is `EngineError::Other` with the full rendering.
- **F.23** The prelude runs in `CoreEngine::with_config` and panics on failure (a build defect, tested); its value does
  not count for `bind_last`.
- **F.24** `pub import`, nested `module`, and `pub` on a non-definition are errors.
- **F.25** `mc-eval` fix: a `fn` whose clause is `f(xs...)` was replaced by its own catch-all (same pattern); the
  catch-all is now left out when a clause already catches every call.

## Public API additions in other crates

- `mc-syntax`: `pub mod scope` (`Scopes`, `Bind`, `pattern_vars`, `lambda_param_names`, `statement_binding`), `Heads`
  and `as_sym` re-exported, `read_spanned` (`Spanned { term, span, body }`), `identifiers(src)`.
- `mc-eval`: `Kernel::clear(s)` (own value, attributes, identity, own and argument-head rules; keeps a native; bumps
  version and epoch), `Kernel::is_defined`, `Kernel::has_native`; internal `SymTable::get_mut`.
- `mc-engine`: depends on `mc-load`; `EngineConfig::prelude`; `CoreEngine::eval_in`, `eval_file`, `modules`,
  `set_sources`, `has_native`; `Script { events, last, last_is_null }` with `failed()`, `Event::{Output, Failure}`,
  `RunError::{Read, Engine}`; re-exports `InMemory`, `LoadError`, `Module`, `Origin`, `SourceProvider`; example
  `examples/startup.rs`.
- `mc-repl`: `Host` trait (`engine`, `load`, `modules`) implemented for `CoreEngine`; `Repl::with_host` (`Repl::new`
  still takes a plain `Engine`); modules `cli` and `host`.

## Tests

| Where | What | Count |
|---|---|---|
| `mc-load/tests/resolve.rs` | shadowing order, forward references, locals/patterns/types/options/failure tags not renamed, `pub`/private/missing/unknown-module errors with `line:col` and caret, import forms, persistence and rollback, ambiguity only on use, cycles, missing file, file without its module, dotted import, nested module, lookup next to the importer, read once, embedded library, `#!` headers, `read_file`, redefinition | 14 + 1 doctest |
| `mc-engine/tests/modules.rs` | modules end to end, lookup next to the file, reload without duplicated clauses, nothing evaluated on a read error, script failures in order, prelude native-free with 8-20 names none used by `bench/cases`, every prelude function, `stats`, identical at 1 and 4 threads under `Config::stress()` | 9 |
| `mc-repl/tests/sessions/modules.txt` | 29-input transcript: prelude, `:modules`, `:load` and reload, imports, a module at the prompt, `stats`, errors | 1 |
| `mc-repl/tests/cli.rs` | example scripts, files and `-e` together, failures to stderr with exit 2, read errors exit 1, `--help`, `--no-prelude`, `-i` | 6 |
| `mc-repl` `cli` unit | argument parsing | 1 |

## Startup (release, busy machine, `examples/startup.rs`, median of 20)

| | run 1 | run 2 |
|---|---|---|
| engine without prelude | 0.42 ms | 0.39 ms |
| engine with prelude | 1.04 ms | 1.04 ms |
| prelude cost (limit 10 ms) | 0.62 ms | 0.65 ms |
| one `eval("1 + 1")` with resolution (median of 200) | 104.8 us | 101.6 us |

## Prelude (`lib/prelude.mc`, 16 names, bare namespace)

`is_even`, `is_odd`; `clamp(x, lo, hi)`; `log_base(b, x)` (exact for integer powers); `count_if`, `every`, `some`,
`member`, `unique`, `zip`, `flat_map`, `accumulate`; `dot`, `norm`; `fix_point(f, x)`; `say(x, ...)`.

Standard module `lib/stats.mc` (`import stats`): `mean`, `variance` (n - 1), `std_dev`, `median`, `geometric_mean`;
private `squared_deviations`.

## Known gaps

- Reload does not clear argument-head rules a module attached to other symbols with `@on(s)`.
- Clearing happens at the start of the whole input; earlier statements of the same input see the cleared module.
- `:load` re-runs a file's statements outside modules (their effects repeat).
- No nested modules, re-exports or reopening.
- Type names are never resolved (a type defined in a module would not be qualified).
- "Known to the kernel" is a read-time snapshot: a module using a bare name the session defines later gets `M.name`.
- Fresh module symbols print as `M.y` and cannot be read back from outside.
- Error positions point at the first occurrence of the name in the statement.
- REPL input goes through `Engine::eval`, so its errors print without an origin name.
- `print` returns its argument, so a script ending in `print(...)` shows the value twice.
- `unique` is O(n^2).
- The engine's known library names come from the `FUNCTIONS` lists; a constant missing from them becomes `M.name`.

## Package 9 (library and kernel gaps, branch `lib/p9-gaps`, commit 536c775)

- **Reload clears `@on` attachments (9.6):** `Module::attached: BTreeSet<(holder, outer)>` records the module body's
  `@on(holder)` attachments (from `define_rule`/`define_fn` decorators, resolved names); `Read::detached` gives the old
  module's attachments when an input reloads it, and the engine clears all of `holder`'s rules for that `outer` (a rule
  with the same pair defined outside the module goes too).
