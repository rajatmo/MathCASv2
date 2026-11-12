| case | size | store | split | threads | median ms | speedup | efficiency | vs seq | splits | redos | check |
|---|---|---|---|---|---|---|---|---|---|---|---|
| M6a deriv | 1000 | C:optimistic | never | 1 | 21.46 | | | 1.00 | 0 | 0 | ref  |
| M6a deriv | 1000 | C:optimistic | lazy | 1 | 21.22 | 1.00 | 1.00 | 0.99 | 0 | 0 | ok  |
| M6a deriv | 1000 | C:optimistic | lazy | 2 | 12.28 | 1.73 | 0.86 | 0.57 | 9 | 0 | ok  |
| M6a deriv | 1000 | C:optimistic | lazy | 4 | 7.14 | 2.97 | 0.74 | 0.33 | 18 | 0 | ok  |
| M6a deriv | 1000 | C:optimistic | static | 1 | 22.65 | 1.00 | 1.00 | 1.06 | 31 | 0 | ok  |
| M6a deriv | 1000 | C:optimistic | static | 2 | 12.34 | 1.84 | 0.92 | 0.58 | 31 | 0 | ok  |
| M6a deriv | 1000 | C:optimistic | static | 4 | 7.21 | 3.14 | 0.79 | 0.34 | 31 | 0 | ok  |
| M6a deriv | 1000 | C:optimistic | always | 1 | 21.38 | 1.00 | 1.00 | 1.00 | 19904 | 0 | ok  |
| M6a deriv | 1000 | C:optimistic | always | 2 | 11.85 | 1.80 | 0.90 | 0.55 | 19906 | 0 | ok  |
| M6a deriv | 1000 | C:optimistic | always | 4 | 7.17 | 2.98 | 0.75 | 0.33 | 19905 | 0 | ok  |
| M6a deriv | 1000 | B:sharded | lazy | 1 | 19.26 | 1.00 | 1.00 | 0.90 | 0 | 0 | ok  |
| M6a deriv | 1000 | B:sharded | lazy | 2 | 10.94 | 1.76 | 0.88 | 0.51 | 9 | 0 | ok  |
| M6a deriv | 1000 | B:sharded | lazy | 4 | 6.65 | 2.90 | 0.72 | 0.31 | 19 | 0 | ok  |
| M6a deriv | 1000 | A:locked | lazy | 1 | 18.52 | 1.00 | 1.00 | 0.86 | 0 | 0 | ok  |
| M6a deriv | 1000 | A:locked | lazy | 2 | 13.54 | 1.37 | 0.68 | 0.63 | 9 | 0 | ok  |
| M6a deriv | 1000 | A:locked | lazy | 4 | 13.00 | 1.42 | 0.36 | 0.61 | 19 | 0 | ok  |
| M6a deriv | 5000 | C:optimistic | never | 1 | 96.85 | | | 1.00 | 0 | 0 | ref  |
| M6a deriv | 5000 | C:optimistic | lazy | 1 | 106.46 | 1.00 | 1.00 | 1.10 | 0 | 0 | ok  |
| M6a deriv | 5000 | C:optimistic | lazy | 2 | 57.51 | 1.85 | 0.93 | 0.59 | 15 | 0 | ok  |
| M6a deriv | 5000 | C:optimistic | lazy | 4 | 34.15 | 3.12 | 0.78 | 0.35 | 69 | 0 | ok  |
| M6a deriv | 5000 | C:optimistic | static | 1 | 102.56 | 1.00 | 1.00 | 1.06 | 294 | 0 | ok  |
| M6a deriv | 5000 | C:optimistic | static | 2 | 61.17 | 1.68 | 0.84 | 0.63 | 287 | 0 | ok  |
| M6a deriv | 5000 | C:optimistic | static | 4 | 33.21 | 3.09 | 0.77 | 0.34 | 276 | 0 | ok  |
| M6a deriv | 5000 | C:optimistic | always | 1 | 103.17 | 1.00 | 1.00 | 1.07 | 93915 | 0 | ok  |
| M6a deriv | 5000 | C:optimistic | always | 2 | 58.80 | 1.75 | 0.88 | 0.61 | 93913 | 0 | ok  |
| M6a deriv | 5000 | C:optimistic | always | 4 | 33.31 | 3.10 | 0.77 | 0.34 | 93917 | 0 | ok  |
| M6a deriv | 5000 | B:sharded | lazy | 1 | 90.48 | 1.00 | 1.00 | 0.93 | 0 | 0 | ok  |
| M6a deriv | 5000 | B:sharded | lazy | 2 | 53.55 | 1.69 | 0.84 | 0.55 | 15 | 0 | ok  |
| M6a deriv | 5000 | B:sharded | lazy | 4 | 30.85 | 2.93 | 0.73 | 0.32 | 39 | 0 | ok  |
| M6a deriv | 5000 | A:locked | lazy | 1 | 91.25 | 1.00 | 1.00 | 0.94 | 0 | 0 | ok  |
| M6a deriv | 5000 | A:locked | lazy | 2 | 67.09 | 1.36 | 0.68 | 0.69 | 16 | 0 | ok  |
| M6a deriv | 5000 | A:locked | lazy | 4 | 74.01 | 1.23 | 0.31 | 0.76 | 43 | 0 | ok  |
| M6b loop | 1000 | C:optimistic | never | 1 | 4.05 | | | 1.00 | 0 | 0 | ref  |
| M6b loop | 1000 | C:optimistic | lazy | 1 | 3.88 | 1.00 | 1.00 | 0.96 | 0 | 0 | ok  |
| M6b loop | 1000 | C:optimistic | lazy | 2 | 2.26 | 1.72 | 0.86 | 0.56 | 7 | 0 | ok  |
| M6b loop | 1000 | C:optimistic | lazy | 4 | 1.49 | 2.60 | 0.65 | 0.37 | 11 | 0 | ok  |
| M6b loop | 1000 | C:optimistic | static | 1 | 3.37 | 1.00 | 1.00 | 0.83 | 0 | 0 | ok  |
| M6b loop | 1000 | C:optimistic | static | 2 | 3.11 | 1.08 | 0.54 | 0.77 | 0 | 0 | ok  |
| M6b loop | 1000 | C:optimistic | static | 4 | 3.14 | 1.07 | 0.27 | 0.77 | 0 | 0 | ok  |
| M6b loop | 1000 | C:optimistic | always | 1 | 3.19 | 1.00 | 1.00 | 0.79 | 63 | 0 | ok  |
| M6b loop | 1000 | C:optimistic | always | 2 | 2.13 | 1.49 | 0.75 | 0.53 | 63 | 0 | ok  |
| M6b loop | 1000 | C:optimistic | always | 4 | 1.37 | 2.32 | 0.58 | 0.34 | 63 | 0 | ok  |
| M6b loop | 1000 | A:locked | lazy | 1 | 4.45 | 1.00 | 1.00 | 1.10 | 0 | 0 | ok  |
| M6b loop | 1000 | A:locked | lazy | 2 | 4.58 | 0.97 | 0.49 | 1.13 | 7 | 0 | ok  |
| M6b loop | 1000 | A:locked | lazy | 4 | 7.07 | 0.63 | 0.16 | 1.74 | 11 | 0 | ok  |
| M6b loop | 10000 | C:optimistic | never | 1 | 56.11 | | | 1.00 | 0 | 0 | ref  |
| M6b loop | 10000 | C:optimistic | lazy | 1 | 55.96 | 1.00 | 1.00 | 1.00 | 0 | 0 | ok  |
| M6b loop | 10000 | C:optimistic | lazy | 2 | 37.01 | 1.51 | 0.76 | 0.66 | 11 | 0 | ok  |
| M6b loop | 10000 | C:optimistic | lazy | 4 | 22.37 | 2.50 | 0.63 | 0.40 | 25 | 0 | ok  |
| M6b loop | 10000 | C:optimistic | static | 1 | 51.02 | 1.00 | 1.00 | 0.91 | 0 | 0 | ok  |
| M6b loop | 10000 | C:optimistic | static | 2 | 50.82 | 1.00 | 0.50 | 0.91 | 0 | 0 | ok  |
| M6b loop | 10000 | C:optimistic | static | 4 | 51.15 | 1.00 | 0.25 | 0.91 | 0 | 0 | ok  |
| M6b loop | 10000 | C:optimistic | always | 1 | 50.74 | 1.00 | 1.00 | 0.90 | 63 | 0 | ok  |
| M6b loop | 10000 | C:optimistic | always | 2 | 37.17 | 1.36 | 0.68 | 0.66 | 63 | 0 | ok  |
| M6b loop | 10000 | C:optimistic | always | 4 | 21.81 | 2.33 | 0.58 | 0.39 | 63 | 0 | ok  |
| M6b loop | 10000 | A:locked | lazy | 1 | 52.76 | 1.00 | 1.00 | 0.94 | 0 | 0 | ok  |
| M6b loop | 10000 | A:locked | lazy | 2 | 57.09 | 0.92 | 0.46 | 1.02 | 11 | 0 | ok  |
| M6b loop | 10000 | A:locked | lazy | 4 | 86.22 | 0.61 | 0.15 | 1.54 | 27 | 0 | ok  |
| M7 map | 100000 | C:optimistic | never | 1 | 0.63 | | | 1.00 | 0 | 0 | ref  |
| M7 map | 100000 | C:optimistic | lazy | 1 | 0.61 | 1.00 | 1.00 | 0.96 | 0 | 0 | ok  |
| M7 map | 100000 | C:optimistic | lazy | 2 | 0.27 | 2.20 | 1.10 | 0.44 | 10 | 0 | ok  |
| M7 map | 100000 | C:optimistic | lazy | 4 | 0.20 | 3.02 | 0.75 | 0.32 | 19 | 0 | ok  |
| M7 map | 100000 | C:optimistic | static | 1 | 0.43 | 1.00 | 1.00 | 0.69 | 32 | 0 | ok  |
| M7 map | 100000 | C:optimistic | static | 2 | 0.26 | 1.65 | 0.82 | 0.42 | 32 | 0 | ok  |
| M7 map | 100000 | C:optimistic | static | 4 | 0.17 | 2.59 | 0.65 | 0.27 | 32 | 0 | ok  |
| M7 map | 100000 | C:optimistic | always | 1 | 3.17 | 1.00 | 1.00 | 5.03 | 100000 | 0 | ok  |
| M7 map | 100000 | C:optimistic | always | 2 | 1.57 | 2.02 | 1.01 | 2.49 | 100000 | 0 | ok  |
| M7 map | 100000 | C:optimistic | always | 4 | 0.78 | 4.08 | 1.02 | 1.23 | 100000 | 0 | ok  |
| M7 map | 1000000 | C:optimistic | never | 1 | 7.07 | | | 1.00 | 0 | 0 | ref  |
| M7 map | 1000000 | C:optimistic | lazy | 1 | 6.98 | 1.00 | 1.00 | 0.99 | 0 | 0 | ok  |
| M7 map | 1000000 | C:optimistic | lazy | 2 | 3.43 | 2.03 | 1.02 | 0.49 | 16 | 0 | ok  |
| M7 map | 1000000 | C:optimistic | lazy | 4 | 2.30 | 3.03 | 0.76 | 0.33 | 65 | 0 | ok  |
| M7 map | 1000000 | C:optimistic | static | 1 | 5.40 | 1.00 | 1.00 | 0.76 | 256 | 0 | ok  |
| M7 map | 1000000 | C:optimistic | static | 2 | 3.42 | 1.58 | 0.79 | 0.48 | 256 | 0 | ok  |
| M7 map | 1000000 | C:optimistic | static | 4 | 2.20 | 2.45 | 0.61 | 0.31 | 256 | 0 | ok  |
| M7 map | 1000000 | C:optimistic | always | 1 | 30.40 | 1.00 | 1.00 | 4.30 | 1000000 | 0 | ok  |
| M7 map | 1000000 | C:optimistic | always | 2 | 16.02 | 1.90 | 0.95 | 2.27 | 1000000 | 0 | ok  |
| M7 map | 1000000 | C:optimistic | always | 4 | 7.60 | 4.00 | 1.00 | 1.07 | 1000000 | 0 | ok  |
| M8 intern | 1000000 | A:locked | - | 1 | 47.66 | 1.00 | 1.00 | 0.97 | 0 | 0 | ok 27 MB reserved |
| M8 intern | 1000000 | A:locked | - | 2 | 67.27 | 0.71 | 0.35 | 1.37 | 0 | 0 | ok 27 MB reserved |
| M8 intern | 1000000 | A:locked | - | 4 | 108.65 | 0.44 | 0.11 | 2.22 | 0 | 0 | ok 27 MB reserved |
| M8 intern | 1000000 | B:sharded | - | 1 | 48.12 | 1.00 | 1.00 | 0.98 | 0 | 0 | ok 27 MB reserved |
| M8 intern | 1000000 | B:sharded | - | 2 | 37.16 | 1.29 | 0.65 | 0.76 | 0 | 0 | ok 27 MB reserved |
| M8 intern | 1000000 | B:sharded | - | 4 | 25.56 | 1.88 | 0.47 | 0.52 | 0 | 0 | ok 28 MB reserved |
| M8 intern | 1000000 | C:optimistic | - | 1 | 56.86 | 1.00 | 1.00 | 1.16 | 0 | 0 | ok 27 MB reserved |
| M8 intern | 1000000 | C:optimistic | - | 2 | 41.30 | 1.38 | 0.69 | 0.84 | 0 | 0 | ok 27 MB reserved |
| M8 intern | 1000000 | C:optimistic | - | 4 | 25.51 | 2.23 | 0.56 | 0.52 | 0 | 0 | ok 28 MB reserved |
| M8 intern | 10000000 | A:locked | - | 1 | 1090.71 | 1.00 | 1.00 | 1.04 | 0 | 0 | ok 259 MB reserved |
| M8 intern | 10000000 | A:locked | - | 2 | 1197.88 | 0.91 | 0.46 | 1.14 | 0 | 0 | ok 260 MB reserved |
| M8 intern | 10000000 | A:locked | - | 4 | 1560.28 | 0.70 | 0.17 | 1.49 | 0 | 0 | ok 260 MB reserved |
| M8 intern | 10000000 | B:sharded | - | 1 | 982.83 | 1.00 | 1.00 | 0.94 | 0 | 0 | ok 259 MB reserved |
| M8 intern | 10000000 | B:sharded | - | 2 | 595.91 | 1.65 | 0.82 | 0.57 | 0 | 0 | ok 259 MB reserved |
| M8 intern | 10000000 | B:sharded | - | 4 | 379.32 | 2.59 | 0.65 | 0.36 | 0 | 0 | ok 260 MB reserved |
| M8 intern | 10000000 | C:optimistic | - | 1 | 953.46 | 1.00 | 1.00 | 0.91 | 0 | 0 | ok 259 MB reserved |
| M8 intern | 10000000 | C:optimistic | - | 2 | 580.15 | 1.64 | 0.82 | 0.55 | 0 | 0 | ok 260 MB reserved |
| M8 intern | 10000000 | C:optimistic | - | 4 | 356.58 | 2.67 | 0.67 | 0.34 | 0 | 0 | ok 260 MB reserved |
| M10 ptable | 10000 | C:optimistic | never | 1 | 45.63 | | | 1.00 | 0 | 0 | ref  |
| M10 ptable | 10000 | C:optimistic | lazy | 1 | 45.62 | 1.00 | 1.00 | 1.00 | 0 | 0 | ok  |
| M10 ptable | 10000 | C:optimistic | lazy | 2 | 29.72 | 1.53 | 0.77 | 0.65 | 36 | 0 | ok  |
| M10 ptable | 10000 | C:optimistic | lazy | 4 | 15.75 | 2.90 | 0.72 | 0.35 | 69 | 0 | ok  |
| M10 ptable | 10000 | C:optimistic | always | 1 | 72.46 | 1.00 | 1.00 | 1.59 | 625004 | 400002 | ok  |
| M10 ptable | 10000 | C:optimistic | always | 2 | 40.42 | 1.79 | 0.90 | 0.89 | 625004 | 400002 | ok  |
| M10 ptable | 10000 | C:optimistic | always | 4 | 21.76 | 3.33 | 0.83 | 0.48 | 625004 | 400002 | ok  |
| M10 ptable | 10000 | A:locked | lazy | 1 | 44.96 | 1.00 | 1.00 | 0.99 | 0 | 0 | ok  |
| M10 ptable | 10000 | A:locked | lazy | 2 | 35.99 | 1.25 | 0.62 | 0.79 | 36 | 0 | ok  |
| M10 ptable | 10000 | A:locked | lazy | 4 | 43.60 | 1.03 | 0.26 | 0.96 | 66 | 0 | ok  |
| M10 ptable | 100000 | C:optimistic | never | 1 | 445.09 | | | 1.00 | 0 | 0 | ref  |
| M10 ptable | 100000 | C:optimistic | lazy | 1 | 467.20 | 1.00 | 1.00 | 1.05 | 0 | 0 | ok  |
| M10 ptable | 100000 | C:optimistic | lazy | 2 | 282.43 | 1.65 | 0.83 | 0.63 | 80 | 2 | ok  |
| M10 ptable | 100000 | C:optimistic | lazy | 4 | 148.60 | 3.14 | 0.79 | 0.33 | 237 | 2 | ok  |
| M10 ptable | 100000 | C:optimistic | always | 1 | 720.05 | 1.00 | 1.00 | 1.62 | 6250004 | 4000002 | ok  |
| M10 ptable | 100000 | C:optimistic | always | 2 | 400.83 | 1.80 | 0.90 | 0.90 | 6250004 | 4000002 | ok  |
| M10 ptable | 100000 | C:optimistic | always | 4 | 208.19 | 3.46 | 0.86 | 0.47 | 6250004 | 4000002 | ok  |
| M10 ptable | 100000 | A:locked | lazy | 1 | 464.43 | 1.00 | 1.00 | 1.04 | 0 | 0 | ok  |
| M10 ptable | 100000 | A:locked | lazy | 2 | 400.17 | 1.16 | 0.58 | 0.90 | 77 | 2 | ok  |
| M10 ptable | 100000 | A:locked | lazy | 4 | 521.41 | 0.89 | 0.22 | 1.17 | 190 | 2 | ok  |
| M11 prints | 1000 | C:optimistic | never | 1 | 3.63 | | | 1.00 | 0 | 0 | ref 5 outputs |
| M11 prints | 1000 | C:optimistic | lazy | 1 | 3.61 | 1.00 | 1.00 | 1.00 | 0 | 0 | ok 5 outputs |
| M11 prints | 1000 | C:optimistic | lazy | 2 | 2.47 | 1.46 | 0.73 | 0.68 | 15 | 4 | ok 5 outputs |
| M11 prints | 1000 | C:optimistic | lazy | 4 | 1.60 | 2.26 | 0.57 | 0.44 | 19 | 4 | ok 5 outputs |
| M11 prints | 1000 | C:optimistic | always | 1 | 3.12 | 1.00 | 1.00 | 0.86 | 147 | 13 | ok 5 outputs |
| M11 prints | 1000 | C:optimistic | always | 2 | 2.02 | 1.54 | 0.77 | 0.56 | 147 | 13 | ok 5 outputs |
| M11 prints | 1000 | C:optimistic | always | 4 | 1.27 | 2.45 | 0.61 | 0.35 | 147 | 13 | ok 5 outputs |
| M11 prints | 1000 | A:locked | always | 1 | 3.13 | 1.00 | 1.00 | 0.86 | 147 | 13 | ok 5 outputs |
| M11 prints | 1000 | A:locked | always | 2 | 4.39 | 0.71 | 0.36 | 1.21 | 147 | 13 | ok 5 outputs |
| M11 prints | 1000 | A:locked | always | 4 | 7.08 | 0.44 | 0.11 | 1.95 | 147 | 13 | ok 5 outputs |
| M11 prints | 5000 | C:optimistic | never | 1 | 21.78 | | | 1.00 | 0 | 0 | ref 5 outputs |
| M11 prints | 5000 | C:optimistic | lazy | 1 | 22.18 | 1.00 | 1.00 | 1.02 | 0 | 0 | ok 5 outputs |
| M11 prints | 5000 | C:optimistic | lazy | 2 | 14.33 | 1.55 | 0.77 | 0.66 | 32 | 4 | ok 5 outputs |
| M11 prints | 5000 | C:optimistic | lazy | 4 | 9.82 | 2.26 | 0.56 | 0.45 | 44 | 4 | ok 5 outputs |
| M11 prints | 5000 | C:optimistic | always | 1 | 20.61 | 1.00 | 1.00 | 0.95 | 147 | 13 | ok 5 outputs |
| M11 prints | 5000 | C:optimistic | always | 2 | 13.23 | 1.56 | 0.78 | 0.61 | 147 | 13 | ok 5 outputs |
| M11 prints | 5000 | C:optimistic | always | 4 | 8.16 | 2.53 | 0.63 | 0.37 | 147 | 13 | ok 5 outputs |
| M11 prints | 5000 | A:locked | always | 1 | 17.04 | 1.00 | 1.00 | 0.78 | 147 | 13 | ok 5 outputs |
| M11 prints | 5000 | A:locked | always | 2 | 23.34 | 0.73 | 0.37 | 1.07 | 147 | 13 | ok 5 outputs |
| M11 prints | 5000 | A:locked | always | 4 | 37.31 | 0.46 | 0.11 | 1.71 | 147 | 13 | ok 5 outputs |
| M11 defines | 1000 | C:optimistic | never | 1 | 3.30 | | | 1.00 | 0 | 0 | ref 1 outputs |
| M11 defines | 1000 | C:optimistic | lazy | 1 | 3.27 | 1.00 | 1.00 | 0.99 | 0 | 0 | ok 1 outputs |
| M11 defines | 1000 | C:optimistic | lazy | 2 | 2.55 | 1.28 | 0.64 | 0.77 | 16 | 5 | ok 1 outputs |
| M11 defines | 1000 | C:optimistic | lazy | 4 | 2.18 | 1.50 | 0.38 | 0.66 | 18 | 5 | ok 1 outputs |
| M11 defines | 1000 | C:optimistic | always | 1 | 3.93 | 1.00 | 1.00 | 1.19 | 17716 | 11 | ok 1 outputs |
| M11 defines | 1000 | C:optimistic | always | 2 | 2.64 | 1.49 | 0.74 | 0.80 | 17287 | 11 | ok 1 outputs |
| M11 defines | 1000 | C:optimistic | always | 4 | 2.11 | 1.86 | 0.47 | 0.64 | 17439 | 11 | ok 1 outputs |
| M11 defines | 1000 | A:locked | always | 1 | 3.87 | 1.00 | 1.00 | 1.17 | 17716 | 11 | ok 1 outputs |
| M11 defines | 1000 | A:locked | always | 2 | 4.60 | 0.84 | 0.42 | 1.40 | 17374 | 11 | ok 1 outputs |
| M11 defines | 1000 | A:locked | always | 4 | 6.66 | 0.58 | 0.15 | 2.02 | 17051 | 11 | ok 1 outputs |
| M11 defines | 5000 | C:optimistic | never | 1 | 17.79 | | | 1.00 | 0 | 0 | ref 1 outputs |
| M11 defines | 5000 | C:optimistic | lazy | 1 | 17.82 | 1.00 | 1.00 | 1.00 | 0 | 0 | ok 1 outputs |
| M11 defines | 5000 | C:optimistic | lazy | 2 | 13.87 | 1.28 | 0.64 | 0.78 | 37 | 7 | ok 1 outputs |
| M11 defines | 5000 | C:optimistic | lazy | 4 | 10.27 | 1.73 | 0.43 | 0.58 | 61 | 7 | ok 1 outputs |
| M11 defines | 5000 | C:optimistic | always | 1 | 21.94 | 1.00 | 1.00 | 1.23 | 87716 | 11 | ok 1 outputs |
| M11 defines | 5000 | C:optimistic | always | 2 | 15.09 | 1.45 | 0.73 | 0.85 | 85009 | 11 | ok 1 outputs |
| M11 defines | 5000 | C:optimistic | always | 4 | 10.98 | 2.00 | 0.50 | 0.62 | 86293 | 11 | ok 1 outputs |
| M11 defines | 5000 | A:locked | always | 1 | 20.06 | 1.00 | 1.00 | 1.13 | 87716 | 11 | ok 1 outputs |
| M11 defines | 5000 | A:locked | always | 2 | 27.34 | 0.73 | 0.37 | 1.54 | 84967 | 11 | ok 1 outputs |
| M11 defines | 5000 | A:locked | always | 4 | 39.25 | 0.51 | 0.13 | 2.21 | 84416 | 11 | ok 1 outputs |
| M11 nested | 1000 | C:optimistic | never | 1 | 1.93 | | | 1.00 | 0 | 0 | ref 34 outputs |
| M11 nested | 1000 | C:optimistic | lazy | 1 | 1.90 | 1.00 | 1.00 | 0.98 | 0 | 0 | ok 34 outputs |
| M11 nested | 1000 | C:optimistic | lazy | 2 | 2.01 | 0.94 | 0.47 | 1.04 | 15 | 9 | ok 34 outputs |
| M11 nested | 1000 | C:optimistic | lazy | 4 | 2.05 | 0.93 | 0.23 | 1.06 | 15 | 9 | ok 34 outputs |
| M11 nested | 1000 | C:optimistic | always | 1 | 2.46 | 1.00 | 1.00 | 1.27 | 9087 | 3634 | ok 34 outputs |
| M11 nested | 1000 | C:optimistic | always | 2 | 1.76 | 1.40 | 0.70 | 0.91 | 9092 | 3634 | ok 34 outputs |
| M11 nested | 1000 | C:optimistic | always | 4 | 1.36 | 1.81 | 0.45 | 0.71 | 9092 | 3634 | ok 34 outputs |
| M11 nested | 1000 | A:locked | always | 1 | 2.50 | 1.00 | 1.00 | 1.29 | 9087 | 3634 | ok 34 outputs |
| M11 nested | 1000 | A:locked | always | 2 | 2.13 | 1.17 | 0.59 | 1.11 | 9092 | 3634 | ok 34 outputs |
| M11 nested | 1000 | A:locked | always | 4 | 2.70 | 0.93 | 0.23 | 1.40 | 9092 | 3634 | ok 34 outputs |
| M11 nested | 5000 | C:optimistic | never | 1 | 7.58 | | | 1.00 | 0 | 0 | ref 34 outputs |
| M11 nested | 5000 | C:optimistic | lazy | 1 | 7.71 | 1.00 | 1.00 | 1.02 | 0 | 0 | ok 34 outputs |
| M11 nested | 5000 | C:optimistic | lazy | 2 | 7.58 | 1.02 | 0.51 | 1.00 | 28 | 13 | ok 34 outputs |
| M11 nested | 5000 | C:optimistic | lazy | 4 | 7.69 | 1.00 | 0.25 | 1.02 | 30 | 13 | ok 34 outputs |
| M11 nested | 5000 | C:optimistic | always | 1 | 8.56 | 1.00 | 1.00 | 1.13 | 18924 | 3634 | ok 34 outputs |
| M11 nested | 5000 | C:optimistic | always | 2 | 6.03 | 1.42 | 0.71 | 0.80 | 18886 | 3634 | ok 34 outputs |
| M11 nested | 5000 | C:optimistic | always | 4 | 5.43 | 1.58 | 0.39 | 0.72 | 18590 | 3634 | ok 34 outputs |
| M11 nested | 5000 | A:locked | always | 1 | 7.97 | 1.00 | 1.00 | 1.05 | 18924 | 3634 | ok 34 outputs |
| M11 nested | 5000 | A:locked | always | 2 | 8.40 | 0.95 | 0.47 | 1.11 | 18907 | 3634 | ok 34 outputs |
| M11 nested | 5000 | A:locked | always | 4 | 11.23 | 0.71 | 0.18 | 1.48 | 18147 | 3634 | ok 34 outputs |
| LIMIT | fuel 10^5, interrupt | C:optimistic | never | 1 | fuel used by the root task 102400 | ok |
| LIMIT | fuel 10^5, interrupt | C:optimistic | lazy | 1 | fuel used by the root task 102400 | ok |
| LIMIT | fuel 10^5, interrupt | C:optimistic | always | 1 | fuel used by the root task 353955 | ok |
| LIMIT | fuel 10^5, interrupt | C:optimistic | never | 2 | fuel used by the root task 102400 | ok |
| LIMIT | fuel 10^5, interrupt | C:optimistic | lazy | 2 | fuel used by the root task 126976 | ok |
| LIMIT | fuel 10^5, interrupt | C:optimistic | always | 2 | fuel used by the root task 353955 | ok |
| LIMIT | fuel 10^5, interrupt | C:optimistic | never | 4 | fuel used by the root task 102400 | ok |
| LIMIT | fuel 10^5, interrupt | C:optimistic | lazy | 4 | fuel used by the root task 143360 | ok |
| LIMIT | fuel 10^5, interrupt | C:optimistic | always | 4 | fuel used by the root task 360448 | ok |

| case | size | store | phase | median ms | vs hc+imm | check |
|---|---|---|---|---|---|---|
| R10 replace | 10000 | hc+imm (spike 1) | build | 0.41 | 1.00 | ok |
| R10 replace | 10000 | hc+imm (spike 1) | replace | 0.62 | 1.00 | ok |
| R10 replace | 10000 | A:locked | build | 0.50 | 1.22 | ok |
| R10 replace | 10000 | A:locked | replace | 0.64 | 1.03 | ok |
| R10 replace | 10000 | B:sharded | build | 0.54 | 1.30 | ok |
| R10 replace | 10000 | B:sharded | replace | 0.65 | 1.06 | ok |
| R10 replace | 10000 | C:optimistic | build | 0.49 | 1.18 | ok |
| R10 replace | 10000 | C:optimistic | replace | 0.62 | 1.00 | ok |
| R10 replace | 100000 | hc+imm (spike 1) | build | 4.73 | 1.00 | ok |
| R10 replace | 100000 | hc+imm (spike 1) | replace | 7.84 | 1.00 | ok |
| R10 replace | 100000 | A:locked | build | 4.64 | 0.98 | ok |
| R10 replace | 100000 | A:locked | replace | 6.42 | 0.82 | ok |
| R10 replace | 100000 | B:sharded | build | 4.82 | 1.02 | ok |
| R10 replace | 100000 | B:sharded | replace | 6.59 | 0.84 | ok |
| R10 replace | 100000 | C:optimistic | build | 4.68 | 0.99 | ok |
| R10 replace | 100000 | C:optimistic | replace | 6.97 | 0.89 | ok |
| R11 eval | 10000 | hc+imm (spike 1) | build | 0.41 | 1.00 | ok |
| R11 eval | 10000 | hc+imm (spike 1) | eval | 0.76 | 1.00 | ok |
| R11 eval | 10000 | hc+imm (spike 1) | re-eval (unrelated change) | 0.73 | 1.00 | ok |
| R11 eval | 10000 | hc+imm (spike 1) | re-eval (s3 changed) | 0.72 | 1.00 | ok |
| R11 eval | 10000 | A:locked | build | 0.49 | 1.19 | ok |
| R11 eval | 10000 | A:locked | eval | 0.76 | 1.00 | ok |
| R11 eval | 10000 | A:locked | re-eval (unrelated change) | 0.72 | 0.99 | ok |
| R11 eval | 10000 | A:locked | re-eval (s3 changed) | 0.73 | 1.00 | ok |
| R11 eval | 10000 | B:sharded | build | 0.54 | 1.30 | ok |
| R11 eval | 10000 | B:sharded | eval | 0.80 | 1.05 | ok |
| R11 eval | 10000 | B:sharded | re-eval (unrelated change) | 0.75 | 1.03 | ok |
| R11 eval | 10000 | B:sharded | re-eval (s3 changed) | 0.74 | 1.03 | ok |
| R11 eval | 10000 | C:optimistic | build | 0.48 | 1.15 | ok |
| R11 eval | 10000 | C:optimistic | eval | 0.75 | 0.99 | ok |
| R11 eval | 10000 | C:optimistic | re-eval (unrelated change) | 0.68 | 0.93 | ok |
| R11 eval | 10000 | C:optimistic | re-eval (s3 changed) | 0.67 | 0.92 | ok |
| R11 eval | 100000 | hc+imm (spike 1) | build | 5.05 | 1.00 | ok |
| R11 eval | 100000 | hc+imm (spike 1) | eval | 7.95 | 1.00 | ok |
| R11 eval | 100000 | hc+imm (spike 1) | re-eval (unrelated change) | 7.20 | 1.00 | ok |
| R11 eval | 100000 | hc+imm (spike 1) | re-eval (s3 changed) | 7.15 | 1.00 | ok |
| R11 eval | 100000 | A:locked | build | 4.91 | 0.97 | ok |
| R11 eval | 100000 | A:locked | eval | 7.67 | 0.97 | ok |
| R11 eval | 100000 | A:locked | re-eval (unrelated change) | 7.33 | 1.02 | ok |
| R11 eval | 100000 | A:locked | re-eval (s3 changed) | 7.27 | 1.02 | ok |
| R11 eval | 100000 | B:sharded | build | 5.11 | 1.01 | ok |
| R11 eval | 100000 | B:sharded | eval | 7.64 | 0.96 | ok |
| R11 eval | 100000 | B:sharded | re-eval (unrelated change) | 7.30 | 1.01 | ok |
| R11 eval | 100000 | B:sharded | re-eval (s3 changed) | 7.33 | 1.02 | ok |
| R11 eval | 100000 | C:optimistic | build | 4.66 | 0.92 | ok |
| R11 eval | 100000 | C:optimistic | eval | 7.78 | 0.98 | ok |
| R11 eval | 100000 | C:optimistic | re-eval (unrelated change) | 7.39 | 1.03 | ok |
| R11 eval | 100000 | C:optimistic | re-eval (s3 changed) | 7.35 | 1.03 | ok |

all checks ok
