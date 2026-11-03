| case | size | store | split | threads | median ms | speedup | efficiency | vs seq | splits | redos | check |
|---|---|---|---|---|---|---|---|---|---|---|---|
| M10 ptable | 10000 | C:optimistic | never | 1 | 48.83 | | | 1.00 | 0 | 0 | ref  |
| M10 ptable | 10000 | C:optimistic | lazy | 1 | 48.85 | 1.00 | 1.00 | 1.00 | 0 | 0 | ok  |
| M10 ptable | 10000 | C:optimistic | lazy | 2 | 27.49 | 1.78 | 0.89 | 0.56 | 55 | 0 | ok  |
| M10 ptable | 10000 | C:optimistic | lazy | 4 | 16.03 | 3.05 | 0.76 | 0.33 | 136 | 0 | ok  |
| M10 ptable | 10000 | C:optimistic | lazy | 8 | 9.81 | 4.98 | 0.62 | 0.20 | 238 | 0 | ok  |
| M10 ptable | 10000 | C:optimistic | lazy | 16 | 7.88 | 6.20 | 0.39 | 0.16 | 323 | 0 | ok  |
| M10 ptable | 10000 | C:optimistic | always | 1 | 77.59 | 1.00 | 1.00 | 1.59 | 625004 | 400002 | ok  |
| M10 ptable | 10000 | C:optimistic | always | 2 | 43.20 | 1.80 | 0.90 | 0.88 | 625004 | 400002 | ok  |
| M10 ptable | 10000 | C:optimistic | always | 4 | 22.12 | 3.51 | 0.88 | 0.45 | 625004 | 400002 | ok  |
| M10 ptable | 10000 | C:optimistic | always | 8 | 13.27 | 5.85 | 0.73 | 0.27 | 625004 | 400002 | ok  |
| M10 ptable | 10000 | C:optimistic | always | 16 | 10.24 | 7.57 | 0.47 | 0.21 | 625004 | 400002 | ok  |
| M10 ptable | 10000 | A:locked | lazy | 1 | 47.12 | 1.00 | 1.00 | 0.96 | 0 | 0 | ok  |
| M10 ptable | 10000 | A:locked | lazy | 2 | 33.58 | 1.40 | 0.70 | 0.69 | 54 | 0 | ok  |
| M10 ptable | 10000 | A:locked | lazy | 4 | 43.63 | 1.08 | 0.27 | 0.89 | 154 | 0 | ok  |
| M10 ptable | 10000 | A:locked | lazy | 8 | 133.07 | 0.35 | 0.04 | 2.73 | 214 | 0 | ok  |
| M10 ptable | 10000 | A:locked | lazy | 16 | 230.31 | 0.20 | 0.01 | 4.72 | 308 | 0 | ok  |
| M10 ptable | 100000 | C:optimistic | never | 1 | 470.66 | | | 1.00 | 0 | 0 | ref  |
| M10 ptable | 100000 | C:optimistic | lazy | 1 | 471.83 | 1.00 | 1.00 | 1.00 | 0 | 0 | ok  |
| M10 ptable | 100000 | C:optimistic | lazy | 2 | 280.49 | 1.68 | 0.84 | 0.60 | 84 | 0 | ok  |
| M10 ptable | 100000 | C:optimistic | lazy | 4 | 152.41 | 3.10 | 0.77 | 0.32 | 248 | 0 | ok  |
| M10 ptable | 100000 | C:optimistic | lazy | 8 | 83.30 | 5.66 | 0.71 | 0.18 | 662 | 0 | ok  |
| M10 ptable | 100000 | C:optimistic | lazy | 16 | 63.69 | 7.41 | 0.46 | 0.14 | 1091 | 0 | ok  |
| M10 ptable | 100000 | C:optimistic | always | 1 | 753.60 | 1.00 | 1.00 | 1.60 | 6250004 | 4000002 | ok  |
| M10 ptable | 100000 | C:optimistic | always | 2 | 398.86 | 1.89 | 0.94 | 0.85 | 6250004 | 4000002 | ok  |
| M10 ptable | 100000 | C:optimistic | always | 4 | 207.42 | 3.63 | 0.91 | 0.44 | 6250004 | 4000002 | ok  |
| M10 ptable | 100000 | C:optimistic | always | 8 | 113.65 | 6.63 | 0.83 | 0.24 | 6250004 | 4000002 | ok  |
| M10 ptable | 100000 | C:optimistic | always | 16 | 93.40 | 8.07 | 0.50 | 0.20 | 6250004 | 4000002 | ok  |
| M10 ptable | 100000 | A:locked | lazy | 1 | 425.94 | 1.00 | 1.00 | 0.90 | 0 | 0 | ok  |
| M10 ptable | 100000 | A:locked | lazy | 2 | 332.72 | 1.28 | 0.64 | 0.71 | 83 | 0 | ok  |
| M10 ptable | 100000 | A:locked | lazy | 4 | 466.93 | 0.91 | 0.23 | 0.99 | 276 | 0 | ok  |
| M10 ptable | 100000 | A:locked | lazy | 8 | 1451.61 | 0.29 | 0.04 | 3.08 | 640 | 0 | ok  |
| M10 ptable | 100000 | A:locked | lazy | 16 | 2573.83 | 0.17 | 0.01 | 5.47 | 1128 | 0 | ok  |
| M10 ptable | 1000000 | C:optimistic | never | 1 | 5998.07 | | | 1.00 | 0 | 0 | ref  |
| M10 ptable | 1000000 | C:optimistic | lazy | 1 | 6041.02 | 1.00 | 1.00 | 1.01 | 0 | 0 | ok  |
| M10 ptable | 1000000 | C:optimistic | lazy | 2 | 3315.52 | 1.82 | 0.91 | 0.55 | 128 | 0 | ok  |
| M10 ptable | 1000000 | C:optimistic | lazy | 4 | 1857.03 | 3.25 | 0.81 | 0.31 | 455 | 0 | ok  |
| M10 ptable | 1000000 | C:optimistic | lazy | 8 | 1077.90 | 5.60 | 0.70 | 0.18 | 1636 | 0 | ok  |
| M10 ptable | 1000000 | C:optimistic | lazy | 16 | 779.88 | 7.75 | 0.48 | 0.13 | 2842 | 0 | ok  |

all checks ok
