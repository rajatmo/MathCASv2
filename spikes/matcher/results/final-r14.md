| case | size | variant | phase | median | peak heap | check | note |
|---|---|---|---|---|---|---|---|
| R14 read sets | 10000 | no-memo | eval | 1.54 ms | 3.7 MB | ok |  |
| R14 read sets | 10000 | no-memo | re-eval, no change | 1.27 ms | 3.7 MB | ok |  |
| R14 read sets | 10000 | no-memo | re-eval, g3 changed | 1.28 ms | 3.7 MB | ok | rule steps per phase [10000, 10000, 10000, 10000]; correct after the g3 change (result changed) |
| R14 read sets | 10000 | no-memo | re-eval, unrelated change | 1.27 ms | 3.7 MB | ok |  |
| R14 read sets | 10000 | memo/untracked | eval | 1.77 ms | 4.2 MB | excluded |  |
| R14 read sets | 10000 | memo/untracked | re-eval, no change | 0.000 ms | 4.2 MB | excluded |  |
| R14 read sets | 10000 | memo/untracked | re-eval, unrelated change | 0.000 ms | 4.2 MB | excluded |  |
| R14 read sets | 10000 | memo/mask | eval | 1.75 ms | 4.2 MB | ok |  |
| R14 read sets | 10000 | memo/mask | re-eval, no change | 0.000 ms | 4.2 MB | ok |  |
| R14 read sets | 10000 | memo/untracked | re-eval, g3 changed | 0.000 ms | 4.2 MB | excluded | rule steps per phase [10000, 0, 0, 0]; STALE after the g3 change, as expected without read sets |
| R14 read sets | 10000 | memo/mask | re-eval, unrelated change | 0.088 ms | 4.2 MB | ok |  |
| R14 read sets | 10000 | memo/exact | eval | 2.00 ms | 4.6 MB | ok |  |
| R14 read sets | 10000 | memo/mask | re-eval, g3 changed | 0.116 ms | 4.2 MB | ok | rule steps per phase [10000, 0, 100, 200]; correct after the g3 change (result changed) |
| R14 read sets | 10000 | memo/exact | re-eval, no change | 0.000 ms | 4.6 MB | ok |  |
| R14 read sets | 10000 | memo/exact | re-eval, g3 changed | 0.217 ms | 4.6 MB | ok | rule steps per phase [10000, 0, 0, 100]; correct after the g3 change (result changed) |
| R14 read sets | 10000 | memo/exact | re-eval, unrelated change | 0.000 ms | 4.6 MB | ok |  |
| R14 read sets | 100000 | no-memo | eval | 15.99 ms | 28.9 MB | ok |  |
| R14 read sets | 100000 | no-memo | re-eval, no change | 13.99 ms | 28.9 MB | ok |  |
| R14 read sets | 100000 | no-memo | re-eval, g3 changed | 13.79 ms | 28.9 MB | ok | rule steps per phase [100000, 100000, 100000, 100000]; correct after the g3 change (result changed) |
| R14 read sets | 100000 | no-memo | re-eval, unrelated change | 13.78 ms | 28.9 MB | ok |  |
| R14 read sets | 100000 | memo/untracked | eval | 17.60 ms | 33.8 MB | excluded |  |
| R14 read sets | 100000 | memo/untracked | re-eval, g3 changed | 0.000 ms | 33.8 MB | excluded | rule steps per phase [100000, 0, 0, 0]; STALE after the g3 change, as expected without read sets |
| R14 read sets | 100000 | memo/mask | eval | 18.45 ms | 38.6 MB | ok |  |
| R14 read sets | 100000 | memo/untracked | re-eval, unrelated change | 0.000 ms | 33.8 MB | excluded |  |
| R14 read sets | 100000 | memo/untracked | re-eval, no change | 0.000 ms | 33.8 MB | excluded |  |
| R14 read sets | 100000 | memo/mask | re-eval, g3 changed | 2.07 ms | 38.6 MB | ok | rule steps per phase [100000, 0, 1000, 2000]; correct after the g3 change (result changed) |
| R14 read sets | 100000 | memo/exact | eval | 21.66 ms | 42.8 MB | ok |  |
| R14 read sets | 100000 | memo/exact | re-eval, no change | 0.000 ms | 42.8 MB | ok |  |
| R14 read sets | 100000 | memo/exact | re-eval, unrelated change | 0.000 ms | 42.8 MB | ok |  |
| R14 read sets | 100000 | memo/mask | re-eval, unrelated change | 0.947 ms | 38.6 MB | ok |  |
| R14 read sets | 100000 | memo/exact | re-eval, g3 changed | 2.95 ms | 42.8 MB | ok | rule steps per phase [100000, 0, 0, 1000]; correct after the g3 change (result changed) |
| R14 read sets | 100000 | memo/mask | re-eval, no change | 0.000 ms | 38.6 MB | ok |  |
| R14 read sets | 1000000 | no-memo | eval | 264 ms | 268.5 MB | ok |  |
| R14 read sets | 1000000 | no-memo | re-eval, unrelated change | 276 ms | 268.5 MB | ok |  |
| R14 read sets | 1000000 | no-memo | re-eval, no change | 207 ms | 268.5 MB | ok |  |
| R14 read sets | 1000000 | memo/untracked | eval | 280 ms | 299.8 MB | excluded |  |
| R14 read sets | 1000000 | memo/untracked | re-eval, unrelated change | 0.000 ms | 299.8 MB | excluded |  |
| R14 read sets | 1000000 | memo/untracked | re-eval, no change | 0.000 ms | 299.8 MB | excluded |  |
| R14 read sets | 1000000 | memo/untracked | re-eval, g3 changed | 0.000 ms | 299.8 MB | excluded | rule steps per phase [1000000, 0, 0, 0]; STALE after the g3 change, as expected without read sets |
| R14 read sets | 1000000 | no-memo | re-eval, g3 changed | 282 ms | 268.5 MB | ok | rule steps per phase [1000000, 1000000, 1000000, 1000000]; correct after the g3 change (result changed) |
| R14 read sets | 1000000 | memo/mask | eval | 267 ms | 364.6 MB | ok |  |
| R14 read sets | 1000000 | memo/mask | re-eval, unrelated change | 11.18 ms | 364.6 MB | ok |  |
| R14 read sets | 1000000 | memo/exact | eval | 308 ms | 398.1 MB | ok |  |
| R14 read sets | 1000000 | memo/exact | re-eval, no change | 0.000 ms | 398.1 MB | ok |  |
| R14 read sets | 1000000 | memo/mask | re-eval, no change | 0.000 ms | 364.6 MB | ok |  |
| R14 read sets | 1000000 | memo/exact | re-eval, unrelated change | 0.001 ms | 398.1 MB | ok |  |
| R14 read sets | 1000000 | memo/exact | re-eval, g3 changed | 35.26 ms | 398.1 MB | ok | rule steps per phase [1000000, 0, 0, 10000]; correct after the g3 change (result changed) |
| R14 read sets | 1000000 | memo/mask | re-eval, g3 changed | 27.93 ms | 364.6 MB | ok | rule steps per phase [1000000, 0, 10000, 20000]; correct after the g3 change (result changed) |
