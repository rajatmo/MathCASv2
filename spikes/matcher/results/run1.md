# Spike 2 raw results, run 1 (preliminary)

Taken on a busy machine (four spikes running at once on an AMD Ryzen 7 7700X, 16 threads, 31 GB, Windows 11, Rust 1.99.1, release build): one run per variant, limit 10 s (R4, D4) or 20 s (others). These are not the final numbers; `--full` is rerun on an idle machine.

## R4 and D4 (`--runs 1 --limit 10 R4 D4`)

| case | size | variant | phase | median | peak heap | check | note |
|---|---|---|---|---|---|---|---|
| R4 AC rules | 100 | linear/naive | define | 0.092 ms | 1.9 MB | ok |  |
| R4 AC rules | 100 | linear/naive | apply | 25.72 ms | 1.9 MB | ok | 1182 of 10000 subjects rewritten |
| R4 AC rules | 100 | linear/smart | define | 0.073 ms | 1.9 MB | ok |  |
| R4 AC rules | 100 | linear/smart | apply | 9.48 ms | 1.9 MB | ok | 1182 of 10000 subjects rewritten |
| R4 AC rules | 100 | indexed/naive | define | 0.073 ms | 1.9 MB | ok |  |
| R4 AC rules | 100 | indexed/naive | apply | 4.51 ms | 1.9 MB | ok | 1182 of 10000 subjects rewritten |
| R4 AC rules | 100 | indexed/naive+rest | define | 0.072 ms | 1.9 MB | ok |  |
| R4 AC rules | 100 | indexed/naive+rest | apply | 4.12 ms | 1.9 MB | ok | 1182 of 10000 subjects rewritten |
| R4 AC rules | 100 | indexed/smart | define | 0.066 ms | 1.9 MB | ok |  |
| R4 AC rules | 100 | indexed/smart | apply | 4.03 ms | 1.9 MB | ok | 1182 of 10000 subjects rewritten |
| R4 AC rules | 1000 | linear/naive | define | 0.705 ms | 2.8 MB | ok |  |
| R4 AC rules | 1000 | linear/naive | apply | 4431 ms | 2.8 MB | ok | 6076 of 10000 subjects rewritten |
| R4 AC rules | 1000 | linear/smart | define | 0.784 ms | 2.8 MB | ok |  |
| R4 AC rules | 1000 | linear/smart | apply | 64.71 ms | 2.8 MB | ok | 6076 of 10000 subjects rewritten |
| R4 AC rules | 1000 | indexed/naive | define | 0.775 ms | 2.8 MB | ok |  |
| R4 AC rules | 1000 | indexed/naive | apply | 4303 ms | 2.8 MB | ok | 6076 of 10000 subjects rewritten |
| R4 AC rules | 1000 | indexed/naive+rest | define | 0.758 ms | 2.8 MB | ok |  |
| R4 AC rules | 1000 | indexed/naive+rest | apply | 22.50 ms | 2.8 MB | ok | 6076 of 10000 subjects rewritten |
| R4 AC rules | 1000 | indexed/smart | define | 0.720 ms | 2.8 MB | ok |  |
| R4 AC rules | 1000 | indexed/smart | apply | 21.25 ms | 2.8 MB | ok | 6076 of 10000 subjects rewritten |
| R4 AC rules | 10000 | linear/naive | define | 18.40 ms | 6.2 MB | limit |  |
| R4 AC rules | 10000 | linear/naive | apply | 9973 ms | 6.2 MB | limit | limit: 3841 of 10000 subjects in 10.0 s (2596.45 us each) |
| R4 AC rules | 10000 | linear/smart | define | 18.93 ms | 7.4 MB | ok |  |
| R4 AC rules | 10000 | linear/smart | apply | 202 ms | 7.4 MB | ok | 9609 of 10000 subjects rewritten |
| R4 AC rules | 10000 | indexed/naive | define | 19.36 ms | 6.2 MB | limit |  |
| R4 AC rules | 10000 | indexed/naive | apply | 9972 ms | 6.2 MB | limit | limit: 3857 of 10000 subjects in 10.0 s (2585.33 us each) |
| R4 AC rules | 10000 | indexed/naive+rest | define | 17.62 ms | 7.4 MB | ok |  |
| R4 AC rules | 10000 | indexed/naive+rest | apply | 125 ms | 7.4 MB | ok | 9609 of 10000 subjects rewritten |
| R4 AC rules | 10000 | indexed/smart | define | 18.16 ms | 7.4 MB | ok |  |
| R4 AC rules | 10000 | indexed/smart | apply | 123 ms | 7.4 MB | ok | 9609 of 10000 subjects rewritten |
| D4 define AC | 100 | scan | define | 0.255 ms | 0.1 MB | ok | 53 + 46 rules, 0 out of definition order |
| D4 define AC | 100 | index | define | 0.074 ms | 0.1 MB | ok | 53 + 46 rules, 0 out of definition order |
| D4 define AC | 1000 | scan | define | 18.54 ms | 0.6 MB | ok | 472 + 486 rules, 17 out of definition order |
| D4 define AC | 1000 | index | define | 0.818 ms | 0.6 MB | ok | 472 + 486 rules, 17 out of definition order |
| D4 define AC | 10000 | scan | define | 1365 ms | 4.7 MB | ok | 4049 + 4034 rules, 798 out of definition order |
| D4 define AC | 10000 | index | define | 18.30 ms | 4.7 MB | ok | 4049 + 4034 rules, 798 out of definition order |

## R4h (`--runs 1 --limit 20 R4h`)

| case | size | variant | phase | median | peak heap | check | note |
|---|---|---|---|---|---|---|---|
| R4h AC pigeonhole | 6 | naive+rest | match | 12.52 ms | 0.2 MB | ok | 500 of 1000 match |
| R4h AC pigeonhole | 6 | smart | match | 1.28 ms | 0.2 MB | ok | 500 of 1000 match |
| R4h AC pigeonhole | 8 | naive+rest | match | 503 ms | 0.2 MB | ok | 500 of 1000 match |
| R4h AC pigeonhole | 8 | smart | match | 1.81 ms | 0.2 MB | ok | 500 of 1000 match |
| R4h AC pigeonhole | 10 | naive+rest | match | 20000 ms | 0.2 MB | limit | limit: 552 of 1000 subjects in 20.0 s (36231.90 us each) |
| R4h AC pigeonhole | 10 | smart | match | 2.52 ms | 0.2 MB | ok | 500 of 1000 match |

## R5, R13, R14, S7, D5 (`--runs 1 --limit 20 R5 R13 R14 S7 D5`)

| case | size | variant | phase | median | peak heap | check | note |
|---|---|---|---|---|---|---|---|
| R5 literal rules | 500 | linear | define | 0.623 ms | 8.2 MB | ok |  |
| R5 literal rules | 500 | linear | calls | 1380 ms | 8.2 MB | ok | 499115 of 1000000 calls hit a literal rule |
| R5 literal rules | 500 | indexed | define | 0.508 ms | 8.2 MB | ok |  |
| R5 literal rules | 500 | indexed | calls | 141 ms | 8.2 MB | ok | 499115 of 1000000 calls hit a literal rule |
| R5 literal rules | 5000 | linear | define | 2.92 ms | 10.5 MB | ok |  |
| R5 literal rules | 5000 | linear | calls | 12277 ms | 10.5 MB | ok | 499026 of 1000000 calls hit a literal rule |
| R5 literal rules | 5000 | indexed | define | 4.62 ms | 10.5 MB | ok |  |
| R5 literal rules | 5000 | indexed | calls | 153 ms | 10.5 MB | ok | 499026 of 1000000 calls hit a literal rule |
| R5 literal rules | 50000 | linear | define | 41.85 ms | 32.1 MB | limit |  |
| R5 literal rules | 50000 | linear | calls | 19934 ms | 32.1 MB | limit | limit: 164543 of 1000000 calls in 19.9 s (121.15 us each) |
| R5 literal rules | 50000 | indexed | define | 41.87 ms | 32.1 MB | ok |  |
| R5 literal rules | 50000 | indexed | calls | 226 ms | 32.1 MB | ok | 499681 of 1000000 calls hit a literal rule |
| R13 arg-head rules | 10 | arg-head | define | 0.022 ms | 51.0 MB | ok |  |
| R13 arg-head rules | 10 | arg-head | calls | 177 ms | 51.0 MB | ok | 99866 calls fell through to the generic rule |
| R13 arg-head rules | 10 | on-area/linear | define | 0.018 ms | 51.0 MB | ok |  |
| R13 arg-head rules | 10 | on-area/linear | calls | 165 ms | 51.0 MB | ok | 99866 calls fell through to the generic rule |
| R13 arg-head rules | 10 | on-area/indexed | define | 0.016 ms | 51.0 MB | ok |  |
| R13 arg-head rules | 10 | on-area/indexed | calls | 203 ms | 51.0 MB | ok | 99866 calls fell through to the generic rule |
| R13 arg-head rules | 100 | arg-head | define | 0.150 ms | 94.2 MB | ok |  |
| R13 arg-head rules | 100 | arg-head | calls | 195 ms | 94.2 MB | ok | 99866 calls fell through to the generic rule |
| R13 arg-head rules | 100 | on-area/linear | define | 0.068 ms | 94.0 MB | ok |  |
| R13 arg-head rules | 100 | on-area/linear | calls | 689 ms | 94.0 MB | ok | 99866 calls fell through to the generic rule |
| R13 arg-head rules | 100 | on-area/indexed | define | 0.066 ms | 94.0 MB | ok |  |
| R13 arg-head rules | 100 | on-area/indexed | calls | 192 ms | 94.0 MB | ok | 99866 calls fell through to the generic rule |
| R13 arg-head rules | 1000 | arg-head | define | 1.28 ms | 117.0 MB | ok |  |
| R13 arg-head rules | 1000 | arg-head | calls | 235 ms | 117.0 MB | ok | 99866 calls fell through to the generic rule |
| R13 arg-head rules | 1000 | on-area/linear | define | 0.569 ms | 115.4 MB | ok |  |
| R13 arg-head rules | 1000 | on-area/linear | calls | 6067 ms | 115.4 MB | ok | 99866 calls fell through to the generic rule |
| R13 arg-head rules | 1000 | on-area/indexed | define | 0.586 ms | 115.4 MB | ok |  |
| R13 arg-head rules | 1000 | on-area/indexed | calls | 730 ms | 115.4 MB | ok | 99866 calls fell through to the generic rule |
| R14 read sets | 10000 | no-memo | eval | 1.63 ms | 3.7 MB | ok |  |
| R14 read sets | 10000 | no-memo | re-eval, no change | 1.43 ms | 3.7 MB | ok |  |
| R14 read sets | 10000 | no-memo | re-eval, unrelated change | 1.33 ms | 3.7 MB | ok |  |
| R14 read sets | 10000 | no-memo | re-eval, g3 changed | 1.31 ms | 3.7 MB | ok | rule steps per phase [10000, 10000, 10000, 10000]; correct after the g3 change (result changed) |
| R14 read sets | 10000 | memo/untracked | eval | 1.82 ms | 4.2 MB | excluded |  |
| R14 read sets | 10000 | memo/untracked | re-eval, no change | 0.000 ms | 4.2 MB | excluded |  |
| R14 read sets | 10000 | memo/untracked | re-eval, unrelated change | 0.000 ms | 4.2 MB | excluded |  |
| R14 read sets | 10000 | memo/untracked | re-eval, g3 changed | 0.000 ms | 4.2 MB | excluded | rule steps per phase [10000, 0, 0, 0]; STALE after the g3 change, as expected without read sets |
| R14 read sets | 10000 | memo/mask | eval | 1.85 ms | 4.2 MB | ok |  |
| R14 read sets | 10000 | memo/mask | re-eval, no change | 0.000 ms | 4.2 MB | ok |  |
| R14 read sets | 10000 | memo/mask | re-eval, unrelated change | 0.088 ms | 4.2 MB | ok |  |
| R14 read sets | 10000 | memo/mask | re-eval, g3 changed | 0.122 ms | 4.2 MB | ok | rule steps per phase [10000, 0, 100, 200]; correct after the g3 change (result changed) |
| R14 read sets | 10000 | memo/exact | eval | 2.15 ms | 4.6 MB | ok |  |
| R14 read sets | 10000 | memo/exact | re-eval, no change | 0.000 ms | 4.6 MB | ok |  |
| R14 read sets | 10000 | memo/exact | re-eval, unrelated change | 0.000 ms | 4.6 MB | ok |  |
| R14 read sets | 10000 | memo/exact | re-eval, g3 changed | 0.219 ms | 4.6 MB | ok | rule steps per phase [10000, 0, 0, 100]; correct after the g3 change (result changed) |
| R14 read sets | 100000 | no-memo | eval | 18.37 ms | 28.9 MB | ok |  |
| R14 read sets | 100000 | no-memo | re-eval, no change | 14.02 ms | 28.9 MB | ok |  |
| R14 read sets | 100000 | no-memo | re-eval, unrelated change | 13.90 ms | 28.9 MB | ok |  |
| R14 read sets | 100000 | no-memo | re-eval, g3 changed | 13.94 ms | 28.9 MB | ok | rule steps per phase [100000, 100000, 100000, 100000]; correct after the g3 change (result changed) |
| R14 read sets | 100000 | memo/untracked | eval | 18.05 ms | 33.8 MB | excluded |  |
| R14 read sets | 100000 | memo/untracked | re-eval, no change | 0.000 ms | 33.8 MB | excluded |  |
| R14 read sets | 100000 | memo/untracked | re-eval, unrelated change | 0.000 ms | 33.8 MB | excluded |  |
| R14 read sets | 100000 | memo/untracked | re-eval, g3 changed | 0.000 ms | 33.8 MB | excluded | rule steps per phase [100000, 0, 0, 0]; STALE after the g3 change, as expected without read sets |
| R14 read sets | 100000 | memo/mask | eval | 18.04 ms | 38.6 MB | ok |  |
| R14 read sets | 100000 | memo/mask | re-eval, no change | 0.000 ms | 38.6 MB | ok |  |
| R14 read sets | 100000 | memo/mask | re-eval, unrelated change | 0.892 ms | 38.6 MB | ok |  |
| R14 read sets | 100000 | memo/mask | re-eval, g3 changed | 1.94 ms | 38.6 MB | ok | rule steps per phase [100000, 0, 1000, 2000]; correct after the g3 change (result changed) |
| R14 read sets | 100000 | memo/exact | eval | 29.38 ms | 42.8 MB | ok |  |
| R14 read sets | 100000 | memo/exact | re-eval, no change | 0.000 ms | 42.8 MB | ok |  |
| R14 read sets | 100000 | memo/exact | re-eval, unrelated change | 0.000 ms | 42.8 MB | ok |  |
| R14 read sets | 100000 | memo/exact | re-eval, g3 changed | 4.76 ms | 42.8 MB | ok | rule steps per phase [100000, 0, 0, 1000]; correct after the g3 change (result changed) |
| R14 read sets | 1000000 | no-memo | eval | 238 ms | 268.5 MB | ok |  |
| R14 read sets | 1000000 | no-memo | re-eval, no change | 204 ms | 268.5 MB | ok |  |
| R14 read sets | 1000000 | no-memo | re-eval, unrelated change | 260 ms | 268.5 MB | ok |  |
| R14 read sets | 1000000 | no-memo | re-eval, g3 changed | 238 ms | 268.5 MB | ok | rule steps per phase [1000000, 1000000, 1000000, 1000000]; correct after the g3 change (result changed) |
| R14 read sets | 1000000 | memo/untracked | eval | 281 ms | 299.8 MB | excluded |  |
| R14 read sets | 1000000 | memo/untracked | re-eval, no change | 0.000 ms | 299.8 MB | excluded |  |
| R14 read sets | 1000000 | memo/untracked | re-eval, unrelated change | 0.000 ms | 299.8 MB | excluded |  |
| R14 read sets | 1000000 | memo/untracked | re-eval, g3 changed | 0.000 ms | 299.8 MB | excluded | rule steps per phase [1000000, 0, 0, 0]; STALE after the g3 change, as expected without read sets |
| R14 read sets | 1000000 | memo/mask | eval | 277 ms | 364.6 MB | ok |  |
| R14 read sets | 1000000 | memo/mask | re-eval, no change | 0.000 ms | 364.6 MB | ok |  |
| R14 read sets | 1000000 | memo/mask | re-eval, unrelated change | 10.87 ms | 364.6 MB | ok |  |
| R14 read sets | 1000000 | memo/mask | re-eval, g3 changed | 27.96 ms | 364.6 MB | ok | rule steps per phase [1000000, 0, 10000, 20000]; correct after the g3 change (result changed) |
| R14 read sets | 1000000 | memo/exact | eval | 334 ms | 398.1 MB | ok |  |
| R14 read sets | 1000000 | memo/exact | re-eval, no change | 0.000 ms | 398.1 MB | ok |  |
| R14 read sets | 1000000 | memo/exact | re-eval, unrelated change | 0.001 ms | 398.1 MB | ok |  |
| R14 read sets | 1000000 | memo/exact | re-eval, g3 changed | 34.87 ms | 398.1 MB | ok | rule steps per phase [1000000, 0, 0, 10000]; correct after the g3 change (result changed) |
| S7 cases | 10000 | naive | cases x5 | 0.796 ms | 0.6 MB | ok | selected [1512, 2541, 550, 249, 1351] |
| S7 cases | 10000 | smart | cases x5 | 0.810 ms | 0.6 MB | ok | selected [1512, 2541, 550, 249, 1351] |
| S7 cases | 100000 | naive | cases x5 | 8.36 ms | 4.0 MB | ok | selected [14545, 24930, 5590, 2649, 14292] |
| S7 cases | 100000 | smart | cases x5 | 8.12 ms | 4.0 MB | ok | selected [14545, 24930, 5590, 2649, 14292] |
| S7 cases | 1000000 | naive | cases x5 | 84.09 ms | 29.2 MB | ok | selected [147176, 250028, 56090, 26255, 140523] |
| S7 cases | 1000000 | smart | cases x5 | 82.18 ms | 29.2 MB | ok | selected [147176, 250028, 56090, 26255, 140523] |
| D5 define literal | 500 | scan | define | 1.48 ms | 0.1 MB | ok | 476 rules after replacements |
| D5 define literal | 500 | index | define | 0.267 ms | 0.1 MB | ok | 476 rules after replacements |
| D5 define literal | 5000 | scan | define | 129 ms | 1.9 MB | ok | 4751 rules after replacements |
| D5 define literal | 5000 | index | define | 2.82 ms | 1.9 MB | ok | 4751 rules after replacements |
| D5 define literal | 50000 | scan | define | 12856 ms | 15.9 MB | ok | 47501 rules after replacements |
| D5 define literal | 50000 | index | define | 42.66 ms | 15.9 MB | ok | 47501 rules after replacements |

## Quick mode (`--quick`)

| case | size | variant | phase | median | peak heap | check | note |
|---|---|---|---|---|---|---|---|
| R4 AC rules | 100 | linear/naive | define | 0.104 ms | 0.3 MB | ok |  |
| R4 AC rules | 100 | linear/naive | apply | 2.56 ms | 0.3 MB | ok | 112 of 1000 subjects rewritten |
| R4 AC rules | 100 | linear/smart | define | 0.087 ms | 0.3 MB | ok |  |
| R4 AC rules | 100 | linear/smart | apply | 1.00 ms | 0.3 MB | ok | 112 of 1000 subjects rewritten |
| R4 AC rules | 100 | indexed/naive | define | 0.070 ms | 0.3 MB | ok |  |
| R4 AC rules | 100 | indexed/naive | apply | 0.455 ms | 0.3 MB | ok | 112 of 1000 subjects rewritten |
| R4 AC rules | 100 | indexed/naive+rest | define | 0.066 ms | 0.3 MB | ok |  |
| R4 AC rules | 100 | indexed/naive+rest | apply | 0.403 ms | 0.3 MB | ok | 112 of 1000 subjects rewritten |
| R4 AC rules | 100 | indexed/smart | define | 0.069 ms | 0.3 MB | ok |  |
| R4 AC rules | 100 | indexed/smart | apply | 0.398 ms | 0.3 MB | ok | 112 of 1000 subjects rewritten |
| R4h AC pigeonhole | 6 | naive+rest | match | 1.33 ms | 0.0 MB | ok | 50 of 100 match |
| R4h AC pigeonhole | 6 | smart | match | 0.134 ms | 0.0 MB | ok | 50 of 100 match |
| R5 literal rules | 500 | linear | define | 0.274 ms | 0.3 MB | ok |  |
| R5 literal rules | 500 | linear | calls | 12.44 ms | 0.3 MB | ok | 5051 of 10000 calls hit a literal rule |
| R5 literal rules | 500 | indexed | define | 0.279 ms | 0.3 MB | ok |  |
| R5 literal rules | 500 | indexed | calls | 1.26 ms | 0.3 MB | ok | 5051 of 10000 calls hit a literal rule |
| R13 arg-head rules | 10 | arg-head | define | 0.012 ms | 1.4 MB | ok |  |
| R13 arg-head rules | 10 | arg-head | calls | 1.24 ms | 1.4 MB | ok | 1049 calls fell through to the generic rule |
| R13 arg-head rules | 10 | on-area/linear | define | 0.008 ms | 1.4 MB | ok |  |
| R13 arg-head rules | 10 | on-area/linear | calls | 1.22 ms | 1.4 MB | ok | 1049 calls fell through to the generic rule |
| R13 arg-head rules | 10 | on-area/indexed | define | 0.009 ms | 1.4 MB | ok |  |
| R13 arg-head rules | 10 | on-area/indexed | calls | 1.55 ms | 1.4 MB | ok | 1049 calls fell through to the generic rule |
| R14 read sets | 10000 | no-memo | eval | 1.56 ms | 3.7 MB | ok |  |
| R14 read sets | 10000 | no-memo | re-eval, no change | 1.30 ms | 3.7 MB | ok |  |
| R14 read sets | 10000 | no-memo | re-eval, unrelated change | 1.30 ms | 3.7 MB | ok |  |
| R14 read sets | 10000 | no-memo | re-eval, g3 changed | 1.30 ms | 3.7 MB | ok | rule steps per phase [10000, 10000, 10000, 10000]; correct after the g3 change (result changed) |
| R14 read sets | 10000 | memo/untracked | eval | 1.70 ms | 4.2 MB | excluded |  |
| R14 read sets | 10000 | memo/untracked | re-eval, no change | 0.000 ms | 4.2 MB | excluded |  |
| R14 read sets | 10000 | memo/untracked | re-eval, unrelated change | 0.000 ms | 4.2 MB | excluded |  |
| R14 read sets | 10000 | memo/untracked | re-eval, g3 changed | 0.000 ms | 4.2 MB | excluded | rule steps per phase [10000, 0, 0, 0]; STALE after the g3 change, as expected without read sets |
| R14 read sets | 10000 | memo/mask | eval | 1.83 ms | 4.2 MB | ok |  |
| R14 read sets | 10000 | memo/mask | re-eval, no change | 0.000 ms | 4.2 MB | ok |  |
| R14 read sets | 10000 | memo/mask | re-eval, unrelated change | 0.089 ms | 4.2 MB | ok |  |
| R14 read sets | 10000 | memo/mask | re-eval, g3 changed | 0.136 ms | 4.2 MB | ok | rule steps per phase [10000, 0, 100, 200]; correct after the g3 change (result changed) |
| R14 read sets | 10000 | memo/exact | eval | 2.08 ms | 4.6 MB | ok |  |
| R14 read sets | 10000 | memo/exact | re-eval, no change | 0.000 ms | 4.6 MB | ok |  |
| R14 read sets | 10000 | memo/exact | re-eval, unrelated change | 0.000 ms | 4.6 MB | ok |  |
| R14 read sets | 10000 | memo/exact | re-eval, g3 changed | 0.228 ms | 4.6 MB | ok | rule steps per phase [10000, 0, 0, 100]; correct after the g3 change (result changed) |
| S7 cases | 10000 | naive | cases x5 | 0.825 ms | 0.6 MB | ok | selected [1512, 2541, 550, 249, 1351] |
| S7 cases | 10000 | smart | cases x5 | 0.809 ms | 0.6 MB | ok | selected [1512, 2541, 550, 249, 1351] |
| D5 define literal | 500 | scan | define | 1.51 ms | 0.1 MB | ok | 476 rules after replacements |
| D5 define literal | 500 | index | define | 0.256 ms | 0.1 MB | ok | 476 rules after replacements |
| D4 define AC | 100 | scan | define | 0.233 ms | 0.1 MB | ok | 53 + 46 rules, 0 out of definition order |
| D4 define AC | 100 | index | define | 0.065 ms | 0.1 MB | ok | 53 + 46 rules, 0 out of definition order |
