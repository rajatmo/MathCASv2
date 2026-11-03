| case | size | store | split | threads | median ms | speedup | efficiency | vs seq | splits | redos | check |
|---|---|---|---|---|---|---|---|---|---|---|---|
| M7 map | 100000 | C:optimistic | never | 1 | 0.48 | | | 1.00 | 0 | 0 | ref  |
| M7 map | 100000 | C:optimistic | lazy | 1 | 0.46 | 1.00 | 1.00 | 0.97 | 0 | 0 | ok  |
| M7 map | 100000 | C:optimistic | lazy | 2 | 0.41 | 1.13 | 0.57 | 0.86 | 15 | 0 | ok  |
| M7 map | 100000 | C:optimistic | lazy | 4 | 0.32 | 1.46 | 0.37 | 0.67 | 34 | 0 | ok  |
| M7 map | 100000 | C:optimistic | lazy | 8 | 0.26 | 1.78 | 0.22 | 0.55 | 51 | 0 | ok  |
| M7 map | 100000 | C:optimistic | lazy | 16 | 0.30 | 1.57 | 0.10 | 0.62 | 73 | 0 | ok  |
| M7 map | 100000 | C:optimistic | static | 1 | 0.51 | 1.00 | 1.00 | 1.06 | 128 | 0 | ok  |
| M7 map | 100000 | C:optimistic | static | 2 | 0.38 | 1.32 | 0.66 | 0.81 | 128 | 0 | ok  |
| M7 map | 100000 | C:optimistic | static | 4 | 0.24 | 2.14 | 0.53 | 0.50 | 128 | 0 | ok  |
| M7 map | 100000 | C:optimistic | static | 8 | 0.27 | 1.87 | 0.23 | 0.57 | 128 | 0 | ok  |
| M7 map | 100000 | C:optimistic | static | 16 | 0.23 | 2.17 | 0.14 | 0.49 | 128 | 0 | ok  |
| M7 map | 100000 | C:optimistic | always | 1 | 2.81 | 1.00 | 1.00 | 5.89 | 100000 | 0 | ok  |
| M7 map | 100000 | C:optimistic | always | 2 | 1.43 | 1.97 | 0.98 | 2.99 | 100000 | 0 | ok  |
| M7 map | 100000 | C:optimistic | always | 4 | 1.01 | 2.78 | 0.69 | 2.12 | 100000 | 0 | ok  |
| M7 map | 100000 | C:optimistic | always | 8 | 0.64 | 4.38 | 0.55 | 1.34 | 100000 | 0 | ok  |
| M7 map | 100000 | C:optimistic | always | 16 | 0.57 | 4.97 | 0.31 | 1.19 | 100000 | 0 | ok  |
| M7 map | 1000000 | C:optimistic | never | 1 | 4.82 | | | 1.00 | 0 | 0 | ref  |
| M7 map | 1000000 | C:optimistic | lazy | 1 | 4.83 | 1.00 | 1.00 | 1.00 | 0 | 0 | ok  |
| M7 map | 1000000 | C:optimistic | lazy | 2 | 3.70 | 1.30 | 0.65 | 0.77 | 34 | 0 | ok  |
| M7 map | 1000000 | C:optimistic | lazy | 4 | 2.31 | 2.09 | 0.52 | 0.48 | 95 | 0 | ok  |
| M7 map | 1000000 | C:optimistic | lazy | 8 | 1.93 | 2.51 | 0.31 | 0.40 | 142 | 0 | ok  |
| M7 map | 1000000 | C:optimistic | lazy | 16 | 1.89 | 2.55 | 0.16 | 0.39 | 392 | 0 | ok  |
| M7 map | 1000000 | C:optimistic | static | 1 | 5.55 | 1.00 | 1.00 | 1.15 | 1024 | 0 | ok  |
| M7 map | 1000000 | C:optimistic | static | 2 | 3.30 | 1.68 | 0.84 | 0.69 | 1024 | 0 | ok  |
| M7 map | 1000000 | C:optimistic | static | 4 | 2.30 | 2.41 | 0.60 | 0.48 | 1024 | 0 | ok  |
| M7 map | 1000000 | C:optimistic | static | 8 | 1.87 | 2.97 | 0.37 | 0.39 | 1024 | 0 | ok  |
| M7 map | 1000000 | C:optimistic | static | 16 | 1.84 | 3.02 | 0.19 | 0.38 | 1024 | 0 | ok  |
| M7 map | 1000000 | C:optimistic | always | 1 | 29.42 | 1.00 | 1.00 | 6.10 | 1000000 | 0 | ok  |
| M7 map | 1000000 | C:optimistic | always | 2 | 15.07 | 1.95 | 0.98 | 3.13 | 1000000 | 0 | ok  |
| M7 map | 1000000 | C:optimistic | always | 4 | 8.35 | 3.52 | 0.88 | 1.73 | 1000000 | 0 | ok  |
| M7 map | 1000000 | C:optimistic | always | 8 | 4.94 | 5.96 | 0.75 | 1.02 | 1000000 | 0 | ok  |
| M7 map | 1000000 | C:optimistic | always | 16 | 4.49 | 6.55 | 0.41 | 0.93 | 1000000 | 0 | ok  |
| M7 map | 10000000 | C:optimistic | never | 1 | 45.49 | | | 1.00 | 0 | 0 | ref  |
| M7 map | 10000000 | C:optimistic | lazy | 1 | 45.63 | 1.00 | 1.00 | 1.00 | 0 | 0 | ok  |
| M7 map | 10000000 | C:optimistic | lazy | 2 | 33.29 | 1.37 | 0.69 | 0.73 | 57 | 0 | ok  |
| M7 map | 10000000 | C:optimistic | lazy | 4 | 21.74 | 2.10 | 0.52 | 0.48 | 79 | 0 | ok  |
| M7 map | 10000000 | C:optimistic | lazy | 8 | 16.19 | 2.82 | 0.35 | 0.36 | 316 | 0 | ok  |
| M7 map | 10000000 | C:optimistic | lazy | 16 | 15.62 | 2.92 | 0.18 | 0.34 | 763 | 0 | ok  |
| M7 map | 10000000 | C:optimistic | static | 1 | 51.54 | 1.00 | 1.00 | 1.13 | 16384 | 0 | ok  |
| M7 map | 10000000 | C:optimistic | static | 2 | 31.24 | 1.65 | 0.82 | 0.69 | 16384 | 0 | ok  |
| M7 map | 10000000 | C:optimistic | static | 4 | 20.68 | 2.49 | 0.62 | 0.45 | 16384 | 0 | ok  |
| M7 map | 10000000 | C:optimistic | static | 8 | 15.82 | 3.26 | 0.41 | 0.35 | 16384 | 0 | ok  |
| M7 map | 10000000 | C:optimistic | static | 16 | 15.52 | 3.32 | 0.21 | 0.34 | 16384 | 0 | ok  |

all checks ok
