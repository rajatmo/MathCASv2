# `mc-repl`

The REPL, scripts (`mc-repl file.mc`, `-e`, `-i`; item F, see `crates/mc-load/NOTES.md`) and the headless notebook
runner (`mc-repl notebook.mcn`, `--update`; item 2, see `crates/mc-kernel/NOTES.md`).

## Item 9 (library and kernel gaps)

- **`print` shown twice (9.5):** `host::shown_value` leaves out the last value when it equals the output line printed
  just before it (for a string, its literal); covers scripts and `:load`. `print` still returns its argument
  (`f(print(x))` and `mc-eval` tests rely on it), so `print(3); 3` shows 3 once. New dependency `mc-syntax`; 7 new
  `cli.rs` cases.
