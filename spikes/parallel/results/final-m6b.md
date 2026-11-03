| case | size | store | split | threads | median ms | speedup | efficiency | vs seq | splits | redos | check |
|---|---|---|---|---|---|---|---|---|---|---|---|
| M6b loop | 1000 | C:optimistic | never | 1 | 3.24 | | | 1.00 | 0 | 0 | ref  |
| M6b loop | 1000 | C:optimistic | lazy | 1 | 3.15 | 1.00 | 1.00 | 0.97 | 0 | 0 | ok  |
| M6b loop | 1000 | C:optimistic | lazy | 2 | 2.32 | 1.36 | 0.68 | 0.72 | 11 | 0 | ok  |
| M6b loop | 1000 | C:optimistic | lazy | 4 | 1.50 | 2.10 | 0.53 | 0.46 | 27 | 0 | ok  |
| M6b loop | 1000 | C:optimistic | lazy | 8 | 1.15 | 2.74 | 0.34 | 0.35 | 38 | 0 | ok  |
| M6b loop | 1000 | C:optimistic | lazy | 16 | 1.03 | 3.04 | 0.19 | 0.32 | 40 | 0 | ok  |
| M6b loop | 1000 | C:optimistic | static | 1 | 3.14 | 1.00 | 1.00 | 0.97 | 0 | 0 | ok  |
| M6b loop | 1000 | C:optimistic | static | 2 | 3.26 | 0.97 | 0.48 | 1.01 | 0 | 0 | ok  |
| M6b loop | 1000 | C:optimistic | static | 4 | 3.14 | 1.00 | 0.25 | 0.97 | 0 | 0 | ok  |
| M6b loop | 1000 | C:optimistic | static | 8 | 3.13 | 1.01 | 0.13 | 0.97 | 0 | 0 | ok  |
| M6b loop | 1000 | C:optimistic | static | 16 | 3.14 | 1.00 | 0.06 | 0.97 | 0 | 0 | ok  |
| M6b loop | 1000 | C:optimistic | always | 1 | 3.15 | 1.00 | 1.00 | 0.97 | 63 | 0 | ok  |
| M6b loop | 1000 | C:optimistic | always | 2 | 2.21 | 1.42 | 0.71 | 0.68 | 63 | 0 | ok  |
| M6b loop | 1000 | C:optimistic | always | 4 | 1.44 | 2.19 | 0.55 | 0.44 | 63 | 0 | ok  |
| M6b loop | 1000 | C:optimistic | always | 8 | 1.11 | 2.84 | 0.35 | 0.34 | 63 | 0 | ok  |
| M6b loop | 1000 | C:optimistic | always | 16 | 1.02 | 3.09 | 0.19 | 0.31 | 63 | 0 | ok  |
| M6b loop | 1000 | A:locked | lazy | 1 | 3.44 | 1.00 | 1.00 | 1.06 | 0 | 0 | ok  |
| M6b loop | 1000 | A:locked | lazy | 2 | 5.09 | 0.68 | 0.34 | 1.57 | 14 | 0 | ok  |
| M6b loop | 1000 | A:locked | lazy | 4 | 7.53 | 0.46 | 0.11 | 2.32 | 23 | 0 | ok  |
| M6b loop | 1000 | A:locked | lazy | 8 | 16.71 | 0.21 | 0.03 | 5.16 | 31 | 0 | ok  |
| M6b loop | 1000 | A:locked | lazy | 16 | 25.81 | 0.13 | 0.01 | 7.97 | 40 | 0 | ok  |
| M6b loop | 10000 | C:optimistic | never | 1 | 50.48 | | | 1.00 | 0 | 0 | ref  |
| M6b loop | 10000 | C:optimistic | lazy | 1 | 50.04 | 1.00 | 1.00 | 0.99 | 0 | 0 | ok  |
| M6b loop | 10000 | C:optimistic | lazy | 2 | 41.01 | 1.22 | 0.61 | 0.81 | 11 | 0 | ok  |
| M6b loop | 10000 | C:optimistic | lazy | 4 | 24.60 | 2.03 | 0.51 | 0.49 | 26 | 0 | ok  |
| M6b loop | 10000 | C:optimistic | lazy | 8 | 17.19 | 2.91 | 0.36 | 0.34 | 34 | 0 | ok  |
| M6b loop | 10000 | C:optimistic | lazy | 16 | 14.25 | 3.51 | 0.22 | 0.28 | 42 | 0 | ok  |
| M6b loop | 10000 | C:optimistic | static | 1 | 50.44 | 1.00 | 1.00 | 1.00 | 0 | 0 | ok  |
| M6b loop | 10000 | C:optimistic | static | 2 | 49.08 | 1.03 | 0.51 | 0.97 | 0 | 0 | ok  |
| M6b loop | 10000 | C:optimistic | static | 4 | 48.07 | 1.05 | 0.26 | 0.95 | 0 | 0 | ok  |
| M6b loop | 10000 | C:optimistic | static | 8 | 53.45 | 0.94 | 0.12 | 1.06 | 0 | 0 | ok  |
| M6b loop | 10000 | C:optimistic | static | 16 | 48.43 | 1.04 | 0.07 | 0.96 | 0 | 0 | ok  |
| M6b loop | 10000 | C:optimistic | always | 1 | 46.71 | 1.00 | 1.00 | 0.93 | 63 | 0 | ok  |
| M6b loop | 10000 | C:optimistic | always | 2 | 39.12 | 1.19 | 0.60 | 0.77 | 63 | 0 | ok  |
| M6b loop | 10000 | C:optimistic | always | 4 | 22.51 | 2.07 | 0.52 | 0.45 | 63 | 0 | ok  |
| M6b loop | 10000 | C:optimistic | always | 8 | 15.35 | 3.04 | 0.38 | 0.30 | 63 | 0 | ok  |
| M6b loop | 10000 | C:optimistic | always | 16 | 10.91 | 4.28 | 0.27 | 0.22 | 63 | 0 | ok  |
| M6b loop | 10000 | A:locked | lazy | 1 | 43.61 | 1.00 | 1.00 | 0.86 | 0 | 0 | ok  |
| M6b loop | 10000 | A:locked | lazy | 2 | 61.27 | 0.71 | 0.36 | 1.21 | 11 | 0 | ok  |
| M6b loop | 10000 | A:locked | lazy | 4 | 91.13 | 0.48 | 0.12 | 1.81 | 24 | 0 | ok  |
| M6b loop | 10000 | A:locked | lazy | 8 | 184.72 | 0.24 | 0.03 | 3.66 | 35 | 0 | ok  |
| M6b loop | 10000 | A:locked | lazy | 16 | 279.46 | 0.16 | 0.01 | 5.54 | 42 | 0 | ok  |
| M6b loop | 50000 | C:optimistic | never | 1 | 339.50 | | | 1.00 | 0 | 0 | ref  |
| M6b loop | 50000 | C:optimistic | lazy | 1 | 327.54 | 1.00 | 1.00 | 0.96 | 0 | 0 | ok  |
| M6b loop | 50000 | C:optimistic | lazy | 2 | 217.63 | 1.51 | 0.75 | 0.64 | 11 | 0 | ok  |
| M6b loop | 50000 | C:optimistic | lazy | 4 | 134.12 | 2.44 | 0.61 | 0.40 | 24 | 0 | ok  |
| M6b loop | 50000 | C:optimistic | lazy | 8 | 88.47 | 3.70 | 0.46 | 0.26 | 35 | 0 | ok  |
| M6b loop | 50000 | C:optimistic | lazy | 16 | 67.66 | 4.84 | 0.30 | 0.20 | 42 | 0 | ok  |
| M6b loop | 50000 | C:optimistic | static | 1 | 328.83 | 1.00 | 1.00 | 0.97 | 0 | 0 | ok  |
| M6b loop | 50000 | C:optimistic | static | 2 | 318.11 | 1.03 | 0.52 | 0.94 | 0 | 0 | ok  |
| M6b loop | 50000 | C:optimistic | static | 4 | 318.78 | 1.03 | 0.26 | 0.94 | 0 | 0 | ok  |
| M6b loop | 50000 | C:optimistic | static | 8 | 313.88 | 1.05 | 0.13 | 0.92 | 0 | 0 | ok  |
| M6b loop | 50000 | C:optimistic | static | 16 | 325.78 | 1.01 | 0.06 | 0.96 | 0 | 0 | ok  |
| M6b loop | 50000 | C:optimistic | always | 1 | 315.22 | 1.00 | 1.00 | 0.93 | 63 | 0 | ok  |
| M6b loop | 50000 | C:optimistic | always | 2 | 213.32 | 1.48 | 0.74 | 0.63 | 63 | 0 | ok  |
| M6b loop | 50000 | C:optimistic | always | 4 | 137.27 | 2.30 | 0.57 | 0.40 | 63 | 0 | ok  |
| M6b loop | 50000 | C:optimistic | always | 8 | 83.96 | 3.75 | 0.47 | 0.25 | 63 | 0 | ok  |
| M6b loop | 50000 | C:optimistic | always | 16 | 57.64 | 5.47 | 0.34 | 0.17 | 63 | 0 | ok  |
| M6b loop | 50000 | A:locked | lazy | 1 | 275.79 | 1.00 | 1.00 | 0.81 | 0 | 0 | ok  |
| M6b loop | 50000 | A:locked | lazy | 2 | 374.86 | 0.74 | 0.37 | 1.10 | 11 | 0 | ok  |
| M6b loop | 50000 | A:locked | lazy | 4 | 532.00 | 0.52 | 0.13 | 1.57 | 24 | 0 | ok  |
| M6b loop | 50000 | A:locked | lazy | 8 | 987.21 | 0.28 | 0.03 | 2.91 | 37 | 0 | ok  |
| M6b loop | 50000 | A:locked | lazy | 16 | 1517.19 | 0.18 | 0.01 | 4.47 | 42 | 0 | ok  |

all checks ok
