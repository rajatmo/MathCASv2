| case | size | store | total ms | phases (ms) | GCs | pause total | pause max | pause mean | live nodes after last GC | reserved after last GC MB | max reserved after GC MB | peak heap MB | overhead | check | notes |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| LS long session | 50000 | none | 14124 | 1000 evaluations 14124 | 0 | 0.000 | 0.000 | 0.000 | - | - | - | 1868.9 | +0% | ok | memo misses 19682589 |
| LS long session | 50000 | A | 11259 | 1000 evaluations 11259 | 43 | 1258 | 110 | 29.25 | 2489152 | 406.7 | 406.7 | 480.4 | -20% | ok | memo misses 21775620 |
| LS long session | 50000 | B | 11412 | 1000 evaluations 11412 | 43 | 1138 | 91.43 | 26.47 | 2489152 | 318.1 | 318.7 | 326.0 | -19% | ok | memo misses 21775620 |
| LS long session | 50000 | C | 8318 | 1000 evaluations 8318 | 147 (142 minor) | 336 | 52.25 | 2.29 | 2493983 | 180.5 | 180.5 | 183.9 | -41% | ok | memo misses 22626412 |
| LS long session | 50000 | C1 | 6351 | 1000 evaluations 6351 | 1005 (1000 minor) | 292 | 47.21 | 0.291 | 2501948 | 208.8 | 208.8 | 211.6 | -55% | ok | memo misses 24045799 |
| LS long session | 50000 | C0 | 6155 | 1000 evaluations 6155 | 1005 (1000 minor) | 289 | 47.47 | 0.287 | 2501948 | 208.8 | 208.8 | 211.6 | -56% | ok | memo misses 24045799 |

trace (MB reserved after collection): LS long session 50000 A: 15.9 15.9 16.6 19.7 19.9 29.3 32.5 35.6 36.1 54.9 60.8 67.7 104.5 105.3 117.2 130.7 132.6 206.8 230.3 256.6 261.1 406.7

trace (MB reserved after collection): LS long session 50000 B: 10.0 9.8 15.1 17.9 17.9 19.9 33.0 35.7 35.7 39.5 65.8 71.4 79.6 79.1 131.8 143.0 159.5 158.6 264.1 286.0 285.6 318.1

trace (MB reserved after collection): LS long session 50000 C: 11.7 25.2 39.5 43.7 51.6 77.9 77.9 90.8 90.8 90.8 102.1 102.1 154.6 154.6 154.6 180.5 180.5 180.5 180.5 180.5 180.5

trace (MB reserved after collection): LS long session 50000 C1: 3.3 14.7 29.2 31.5 52.4 58.0 58.0 62.6 104.6 104.6 104.6 115.8 115.8 115.8 125.0 125.0 208.8 208.8 208.8 208.8 208.8

trace (MB reserved after collection): LS long session 50000 C0: 3.3 14.7 29.2 31.5 52.4 58.0 58.0 62.6 104.6 104.6 104.6 115.8 115.8 115.8 125.0 125.0 208.8 208.8 208.8 208.8 208.8
