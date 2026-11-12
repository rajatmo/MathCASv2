| case | size | store | split | threads | median ms | speedup | efficiency | vs seq | splits | redos | check |
|---|---|---|---|---|---|---|---|---|---|---|---|
| M6a deriv | 1000 | C:optimistic | never | 1 | 20.68 | | | 1.00 | 0 | 0 | ref  |
| M6a deriv | 1000 | C:optimistic | lazy | 1 | 20.53 | 1.00 | 1.00 | 0.99 | 0 | 0 | ok  |
| M6a deriv | 1000 | C:optimistic | lazy | 2 | 11.51 | 1.78 | 0.89 | 0.56 | 13 | 0 | ok  |
| M6a deriv | 1000 | C:optimistic | lazy | 4 | 7.07 | 2.90 | 0.73 | 0.34 | 36 | 0 | ok  |
| M6a deriv | 1000 | C:optimistic | static | 1 | 21.05 | 1.00 | 1.00 | 1.02 | 211 | 0 | ok  |
| M6a deriv | 1000 | C:optimistic | static | 2 | 11.65 | 1.81 | 0.90 | 0.56 | 208 | 0 | ok  |
| M6a deriv | 1000 | C:optimistic | static | 4 | 7.17 | 2.94 | 0.73 | 0.35 | 205 | 0 | ok  |
| M6a deriv | 1000 | C:optimistic | always | 1 | 21.42 | 1.00 | 1.00 | 1.04 | 19904 | 0 | ok  |
| M6a deriv | 1000 | C:optimistic | always | 2 | 11.99 | 1.79 | 0.89 | 0.58 | 19906 | 0 | ok  |
| M6a deriv | 1000 | C:optimistic | always | 4 | 7.22 | 2.97 | 0.74 | 0.35 | 19906 | 0 | ok  |
| M6a deriv | 1000 | B:sharded | lazy | 1 | 18.59 | 1.00 | 1.00 | 0.90 | 0 | 0 | ok  |
| M6a deriv | 1000 | B:sharded | lazy | 2 | 10.91 | 1.70 | 0.85 | 0.53 | 14 | 0 | ok  |
| M6a deriv | 1000 | B:sharded | lazy | 4 | 6.47 | 2.87 | 0.72 | 0.31 | 39 | 0 | ok  |
| M6a deriv | 1000 | A:locked | lazy | 1 | 18.86 | 1.00 | 1.00 | 0.91 | 0 | 0 | ok  |
| M6a deriv | 1000 | A:locked | lazy | 2 | 13.37 | 1.41 | 0.71 | 0.65 | 14 | 0 | ok  |
| M6a deriv | 1000 | A:locked | lazy | 4 | 12.92 | 1.46 | 0.37 | 0.62 | 41 | 0 | ok  |
| M6a deriv | 5000 | C:optimistic | never | 1 | 96.87 | | | 1.00 | 0 | 0 | ref  |
| M6a deriv | 5000 | C:optimistic | lazy | 1 | 96.04 | 1.00 | 1.00 | 0.99 | 0 | 0 | ok  |
| M6a deriv | 5000 | C:optimistic | lazy | 2 | 51.80 | 1.85 | 0.93 | 0.53 | 19 | 0 | ok  |
| M6a deriv | 5000 | C:optimistic | lazy | 4 | 29.65 | 3.24 | 0.81 | 0.31 | 119 | 0 | ok  |
| M6a deriv | 5000 | C:optimistic | static | 1 | 96.76 | 1.00 | 1.00 | 1.00 | 2440 | 0 | ok  |
| M6a deriv | 5000 | C:optimistic | static | 2 | 55.47 | 1.74 | 0.87 | 0.57 | 2202 | 0 | ok  |
| M6a deriv | 5000 | C:optimistic | static | 4 | 30.34 | 3.19 | 0.80 | 0.31 | 1959 | 0 | ok  |
| M6a deriv | 5000 | C:optimistic | always | 1 | 100.98 | 1.00 | 1.00 | 1.04 | 93915 | 0 | ok  |
| M6a deriv | 5000 | C:optimistic | always | 2 | 54.39 | 1.86 | 0.93 | 0.56 | 93914 | 0 | ok  |
| M6a deriv | 5000 | C:optimistic | always | 4 | 30.62 | 3.30 | 0.82 | 0.32 | 93912 | 0 | ok  |
| M6a deriv | 5000 | B:sharded | lazy | 1 | 85.34 | 1.00 | 1.00 | 0.88 | 0 | 0 | ok  |
| M6a deriv | 5000 | B:sharded | lazy | 2 | 48.86 | 1.75 | 0.87 | 0.50 | 23 | 0 | ok  |
| M6a deriv | 5000 | B:sharded | lazy | 4 | 28.81 | 2.96 | 0.74 | 0.30 | 95 | 0 | ok  |
| M6a deriv | 5000 | A:locked | lazy | 1 | 86.68 | 1.00 | 1.00 | 0.89 | 0 | 0 | ok  |
| M6a deriv | 5000 | A:locked | lazy | 2 | 65.60 | 1.32 | 0.66 | 0.68 | 19 | 0 | ok  |
| M6a deriv | 5000 | A:locked | lazy | 4 | 71.97 | 1.20 | 0.30 | 0.74 | 120 | 0 | ok  |
| M6b loop | 1000 | C:optimistic | never | 1 | 3.89 | | | 1.00 | 0 | 0 | ref  |
| M6b loop | 1000 | C:optimistic | lazy | 1 | 3.68 | 1.00 | 1.00 | 0.95 | 0 | 0 | ok  |
| M6b loop | 1000 | C:optimistic | lazy | 2 | 2.14 | 1.72 | 0.86 | 0.55 | 11 | 0 | ok  |
| M6b loop | 1000 | C:optimistic | lazy | 4 | 1.55 | 2.37 | 0.59 | 0.40 | 24 | 0 | ok  |
| M6b loop | 1000 | C:optimistic | static | 1 | 3.15 | 1.00 | 1.00 | 0.81 | 0 | 0 | ok  |
| M6b loop | 1000 | C:optimistic | static | 2 | 3.09 | 1.02 | 0.51 | 0.79 | 0 | 0 | ok  |
| M6b loop | 1000 | C:optimistic | static | 4 | 3.54 | 0.89 | 0.22 | 0.91 | 0 | 0 | ok  |
| M6b loop | 1000 | C:optimistic | always | 1 | 3.10 | 1.00 | 1.00 | 0.80 | 63 | 0 | ok  |
| M6b loop | 1000 | C:optimistic | always | 2 | 2.06 | 1.50 | 0.75 | 0.53 | 63 | 0 | ok  |
| M6b loop | 1000 | C:optimistic | always | 4 | 1.31 | 2.36 | 0.59 | 0.34 | 63 | 0 | ok  |
| M6b loop | 1000 | A:locked | lazy | 1 | 3.73 | 1.00 | 1.00 | 0.96 | 0 | 0 | ok  |
| M6b loop | 1000 | A:locked | lazy | 2 | 4.79 | 0.78 | 0.39 | 1.23 | 11 | 0 | ok  |
| M6b loop | 1000 | A:locked | lazy | 4 | 7.64 | 0.49 | 0.12 | 1.97 | 24 | 0 | ok  |
| M6b loop | 10000 | C:optimistic | never | 1 | 60.37 | | | 1.00 | 0 | 0 | ref  |
| M6b loop | 10000 | C:optimistic | lazy | 1 | 51.90 | 1.00 | 1.00 | 0.86 | 0 | 0 | ok  |
| M6b loop | 10000 | C:optimistic | lazy | 2 | 35.53 | 1.46 | 0.73 | 0.59 | 11 | 0 | ok  |
| M6b loop | 10000 | C:optimistic | lazy | 4 | 23.16 | 2.24 | 0.56 | 0.38 | 24 | 0 | ok  |
| M6b loop | 10000 | C:optimistic | static | 1 | 48.55 | 1.00 | 1.00 | 0.80 | 0 | 0 | ok  |
| M6b loop | 10000 | C:optimistic | static | 2 | 46.63 | 1.04 | 0.52 | 0.77 | 0 | 0 | ok  |
| M6b loop | 10000 | C:optimistic | static | 4 | 49.08 | 0.99 | 0.25 | 0.81 | 0 | 0 | ok  |
| M6b loop | 10000 | C:optimistic | always | 1 | 45.72 | 1.00 | 1.00 | 0.76 | 63 | 0 | ok  |
| M6b loop | 10000 | C:optimistic | always | 2 | 33.82 | 1.35 | 0.68 | 0.56 | 63 | 0 | ok  |
| M6b loop | 10000 | C:optimistic | always | 4 | 20.18 | 2.27 | 0.57 | 0.33 | 63 | 0 | ok  |
| M6b loop | 10000 | A:locked | lazy | 1 | 44.08 | 1.00 | 1.00 | 0.73 | 0 | 0 | ok  |
| M6b loop | 10000 | A:locked | lazy | 2 | 53.67 | 0.82 | 0.41 | 0.89 | 11 | 0 | ok  |
| M6b loop | 10000 | A:locked | lazy | 4 | 84.65 | 0.52 | 0.13 | 1.40 | 23 | 0 | ok  |
| M7 map | 100000 | C:optimistic | never | 1 | 0.60 | | | 1.00 | 0 | 0 | ref  |
| M7 map | 100000 | C:optimistic | lazy | 1 | 0.61 | 1.00 | 1.00 | 1.00 | 0 | 0 | ok  |
| M7 map | 100000 | C:optimistic | lazy | 2 | 0.25 | 2.42 | 1.21 | 0.41 | 14 | 0 | ok  |
| M7 map | 100000 | C:optimistic | lazy | 4 | 0.21 | 2.94 | 0.73 | 0.34 | 39 | 0 | ok  |
| M7 map | 100000 | C:optimistic | static | 1 | 0.45 | 1.00 | 1.00 | 0.74 | 128 | 0 | ok  |
| M7 map | 100000 | C:optimistic | static | 2 | 0.26 | 1.71 | 0.86 | 0.44 | 128 | 0 | ok  |
| M7 map | 100000 | C:optimistic | static | 4 | 0.15 | 2.91 | 0.73 | 0.26 | 128 | 0 | ok  |
| M7 map | 100000 | C:optimistic | always | 1 | 3.25 | 1.00 | 1.00 | 5.38 | 100000 | 0 | ok  |
| M7 map | 100000 | C:optimistic | always | 2 | 1.40 | 2.32 | 1.16 | 2.32 | 100000 | 0 | ok  |
| M7 map | 100000 | C:optimistic | always | 4 | 0.83 | 3.91 | 0.98 | 1.38 | 100000 | 0 | ok  |
| M7 map | 1000000 | C:optimistic | never | 1 | 6.58 | | | 1.00 | 0 | 0 | ref  |
| M7 map | 1000000 | C:optimistic | lazy | 1 | 6.62 | 1.00 | 1.00 | 1.01 | 0 | 0 | ok  |
| M7 map | 1000000 | C:optimistic | lazy | 2 | 3.25 | 2.04 | 1.02 | 0.49 | 20 | 0 | ok  |
| M7 map | 1000000 | C:optimistic | lazy | 4 | 2.53 | 2.62 | 0.66 | 0.38 | 67 | 0 | ok  |
| M7 map | 1000000 | C:optimistic | static | 1 | 5.17 | 1.00 | 1.00 | 0.79 | 1024 | 0 | ok  |
| M7 map | 1000000 | C:optimistic | static | 2 | 3.10 | 1.67 | 0.83 | 0.47 | 1024 | 0 | ok  |
| M7 map | 1000000 | C:optimistic | static | 4 | 2.20 | 2.35 | 0.59 | 0.34 | 1024 | 0 | ok  |
| M7 map | 1000000 | C:optimistic | always | 1 | 30.96 | 1.00 | 1.00 | 4.71 | 1000000 | 0 | ok  |
| M7 map | 1000000 | C:optimistic | always | 2 | 14.68 | 2.11 | 1.05 | 2.23 | 1000000 | 0 | ok  |
| M7 map | 1000000 | C:optimistic | always | 4 | 8.21 | 3.77 | 0.94 | 1.25 | 1000000 | 0 | ok  |
| M8 intern | 1000000 | A:locked | - | 1 | 42.26 | 1.00 | 1.00 | 0.92 | 0 | 0 | ok 27 MB reserved |
| M8 intern | 1000000 | A:locked | - | 2 | 62.25 | 0.68 | 0.34 | 1.36 | 0 | 0 | ok 27 MB reserved |
| M8 intern | 1000000 | A:locked | - | 4 | 100.37 | 0.42 | 0.11 | 2.19 | 0 | 0 | ok 27 MB reserved |
| M8 intern | 1000000 | B:sharded | - | 1 | 40.03 | 1.00 | 1.00 | 0.87 | 0 | 0 | ok 27 MB reserved |
| M8 intern | 1000000 | B:sharded | - | 2 | 30.39 | 1.32 | 0.66 | 0.66 | 0 | 0 | ok 27 MB reserved |
| M8 intern | 1000000 | B:sharded | - | 4 | 21.67 | 1.85 | 0.46 | 0.47 | 0 | 0 | ok 28 MB reserved |
| M8 intern | 1000000 | C:optimistic | - | 1 | 47.45 | 1.00 | 1.00 | 1.04 | 0 | 0 | ok 27 MB reserved |
| M8 intern | 1000000 | C:optimistic | - | 2 | 34.40 | 1.38 | 0.69 | 0.75 | 0 | 0 | ok 27 MB reserved |
| M8 intern | 1000000 | C:optimistic | - | 4 | 23.50 | 2.02 | 0.50 | 0.51 | 0 | 0 | ok 28 MB reserved |
| M8 intern | 10000000 | A:locked | - | 1 | 834.70 | 1.00 | 1.00 | 0.88 | 0 | 0 | ok 259 MB reserved |
| M8 intern | 10000000 | A:locked | - | 2 | 1079.68 | 0.77 | 0.39 | 1.14 | 0 | 0 | ok 260 MB reserved |
| M8 intern | 10000000 | A:locked | - | 4 | 1453.60 | 0.57 | 0.14 | 1.54 | 0 | 0 | ok 260 MB reserved |
| M8 intern | 10000000 | B:sharded | - | 1 | 928.48 | 1.00 | 1.00 | 0.98 | 0 | 0 | ok 259 MB reserved |
| M8 intern | 10000000 | B:sharded | - | 2 | 604.26 | 1.54 | 0.77 | 0.64 | 0 | 0 | ok 260 MB reserved |
| M8 intern | 10000000 | B:sharded | - | 4 | 381.95 | 2.43 | 0.61 | 0.40 | 0 | 0 | ok 260 MB reserved |
| M8 intern | 10000000 | C:optimistic | - | 1 | 957.09 | 1.00 | 1.00 | 1.01 | 0 | 0 | ok 259 MB reserved |
| M8 intern | 10000000 | C:optimistic | - | 2 | 567.30 | 1.69 | 0.84 | 0.60 | 0 | 0 | ok 260 MB reserved |
| M8 intern | 10000000 | C:optimistic | - | 4 | 353.95 | 2.70 | 0.68 | 0.37 | 0 | 0 | ok 260 MB reserved |
| M10 ptable | 10000 | C:optimistic | never | 1 | 45.58 | | | 1.00 | 0 | 0 | ref  |
| M10 ptable | 10000 | C:optimistic | lazy | 1 | 45.12 | 1.00 | 1.00 | 0.99 | 0 | 0 | ok  |
| M10 ptable | 10000 | C:optimistic | lazy | 2 | 26.76 | 1.69 | 0.84 | 0.59 | 55 | 0 | ok  |
| M10 ptable | 10000 | C:optimistic | lazy | 4 | 15.41 | 2.93 | 0.73 | 0.34 | 145 | 0 | ok  |
| M10 ptable | 10000 | C:optimistic | always | 1 | 76.21 | 1.00 | 1.00 | 1.67 | 625004 | 400002 | ok  |
| M10 ptable | 10000 | C:optimistic | always | 2 | 40.19 | 1.90 | 0.95 | 0.88 | 625004 | 400002 | ok  |
| M10 ptable | 10000 | C:optimistic | always | 4 | 22.56 | 3.38 | 0.84 | 0.49 | 625004 | 400002 | ok  |
| M10 ptable | 10000 | A:locked | lazy | 1 | 44.66 | 1.00 | 1.00 | 0.98 | 0 | 0 | ok  |
| M10 ptable | 10000 | A:locked | lazy | 2 | 33.47 | 1.33 | 0.67 | 0.73 | 55 | 0 | ok  |
| M10 ptable | 10000 | A:locked | lazy | 4 | 45.56 | 0.98 | 0.25 | 1.00 | 154 | 0 | ok  |
| M10 ptable | 100000 | C:optimistic | never | 1 | 465.00 | | | 1.00 | 0 | 0 | ref  |
| M10 ptable | 100000 | C:optimistic | lazy | 1 | 436.11 | 1.00 | 1.00 | 0.94 | 0 | 0 | ok  |
| M10 ptable | 100000 | C:optimistic | lazy | 2 | 254.96 | 1.71 | 0.86 | 0.55 | 94 | 0 | ok  |
| M10 ptable | 100000 | C:optimistic | lazy | 4 | 141.98 | 3.07 | 0.77 | 0.31 | 309 | 0 | ok  |
| M10 ptable | 100000 | C:optimistic | always | 1 | 760.98 | 1.00 | 1.00 | 1.64 | 6250004 | 4000002 | ok  |
| M10 ptable | 100000 | C:optimistic | always | 2 | 391.26 | 1.94 | 0.97 | 0.84 | 6250004 | 4000002 | ok  |
| M10 ptable | 100000 | C:optimistic | always | 4 | 205.58 | 3.70 | 0.93 | 0.44 | 6250004 | 4000002 | ok  |
| M10 ptable | 100000 | A:locked | lazy | 1 | 413.39 | 1.00 | 1.00 | 0.89 | 0 | 0 | ok  |
| M10 ptable | 100000 | A:locked | lazy | 2 | 338.14 | 1.22 | 0.61 | 0.73 | 91 | 0 | ok  |
| M10 ptable | 100000 | A:locked | lazy | 4 | 507.13 | 0.82 | 0.20 | 1.09 | 280 | 0 | ok  |
| M11 fuzz | 100 | C:optimistic | never | 1 | 12.65 | | | 1.00 | 0 | 0 | ref  |
| M11 fuzz | 100 | C:optimistic | lazy | 1 | 12.63 | 1.00 | 1.00 | 1.00 | 0 | 0 | ok  |
| M11 fuzz | 100 | C:optimistic | lazy | 2 | 13.69 | 0.92 | 0.46 | 1.08 | 2 | 2 | ok  |
| M11 fuzz | 100 | C:optimistic | lazy | 4 | 13.62 | 0.93 | 0.23 | 1.08 | 2 | 2 | ok  |
| M11 fuzz | 100 | C:optimistic | always | 1 | 24.54 | 1.00 | 1.00 | 1.94 | 134314 | 66463 | ok  |
| M11 fuzz | 100 | C:optimistic | always | 2 | 16.26 | 1.51 | 0.75 | 1.29 | 134430 | 66463 | ok  |
| M11 fuzz | 100 | C:optimistic | always | 4 | 13.98 | 1.75 | 0.44 | 1.11 | 134323 | 66463 | ok  |
| M11 fuzz | 100 | B:sharded | always | 1 | 23.27 | 1.00 | 1.00 | 1.84 | 134314 | 66463 | ok  |
| M11 fuzz | 100 | B:sharded | always | 2 | 16.65 | 1.40 | 0.70 | 1.32 | 134437 | 66463 | ok  |
| M11 fuzz | 100 | B:sharded | always | 4 | 14.79 | 1.57 | 0.39 | 1.17 | 134296 | 66463 | ok  |
| M11 fuzz | 300 | C:optimistic | never | 1 | 22.73 | | | 1.00 | 0 | 0 | ref  |
| M11 fuzz | 300 | C:optimistic | lazy | 1 | 22.28 | 1.00 | 1.00 | 0.98 | 0 | 0 | ok  |
| M11 fuzz | 300 | C:optimistic | lazy | 2 | 22.69 | 0.98 | 0.49 | 1.00 | 3 | 2 | ok  |
| M11 fuzz | 300 | C:optimistic | lazy | 4 | 22.86 | 0.97 | 0.24 | 1.01 | 3 | 2 | ok  |
| M11 fuzz | 300 | C:optimistic | always | 1 | 40.61 | 1.00 | 1.00 | 1.79 | 176404 | 66463 | ok  |
| M11 fuzz | 300 | C:optimistic | always | 2 | 29.15 | 1.39 | 0.70 | 1.28 | 176092 | 66463 | ok  |
| M11 fuzz | 300 | C:optimistic | always | 4 | 26.20 | 1.55 | 0.39 | 1.15 | 176298 | 66463 | ok  |
| M11 fuzz | 300 | B:sharded | always | 1 | 41.50 | 1.00 | 1.00 | 1.83 | 176404 | 66463 | ok  |
| M11 fuzz | 300 | B:sharded | always | 2 | 30.09 | 1.38 | 0.69 | 1.32 | 176045 | 66463 | ok  |
| M11 fuzz | 300 | B:sharded | always | 4 | 27.26 | 1.52 | 0.38 | 1.20 | 176767 | 66463 | ok  |
| M11 prints | 1000 | C:optimistic | never | 1 | 3.72 | | | 1.00 | 0 | 0 | ref 5 outputs |
| M11 prints | 1000 | C:optimistic | lazy | 1 | 3.69 | 1.00 | 1.00 | 0.99 | 0 | 0 | ok 5 outputs |
| M11 prints | 1000 | C:optimistic | lazy | 2 | 2.17 | 1.70 | 0.85 | 0.58 | 11 | 4 | ok 5 outputs |
| M11 prints | 1000 | C:optimistic | lazy | 4 | 1.40 | 2.63 | 0.66 | 0.38 | 24 | 4 | ok 5 outputs |
| M11 prints | 1000 | C:optimistic | lazy-stop | 1 | 3.62 | 1.00 | 1.00 | 0.97 | 0 | 0 | ok 5 outputs |
| M11 prints | 1000 | C:optimistic | lazy-stop | 2 | 2.33 | 1.55 | 0.78 | 0.63 | 12 | 11 | ok 5 outputs |
| M11 prints | 1000 | C:optimistic | lazy-stop | 4 | 1.68 | 2.16 | 0.54 | 0.45 | 23 | 7 | ok 5 outputs |
| M11 prints | 1000 | C:optimistic | always | 1 | 3.09 | 1.00 | 1.00 | 0.83 | 63 | 5 | ok 5 outputs |
| M11 prints | 1000 | C:optimistic | always | 2 | 2.05 | 1.51 | 0.75 | 0.55 | 63 | 5 | ok 5 outputs |
| M11 prints | 1000 | C:optimistic | always | 4 | 1.42 | 2.17 | 0.54 | 0.38 | 63 | 5 | ok 5 outputs |
| M11 prints | 1000 | A:locked | always | 1 | 3.05 | 1.00 | 1.00 | 0.82 | 63 | 5 | ok 5 outputs |
| M11 prints | 1000 | A:locked | always | 2 | 4.42 | 0.69 | 0.34 | 1.19 | 63 | 5 | ok 5 outputs |
| M11 prints | 1000 | A:locked | always | 4 | 6.94 | 0.44 | 0.11 | 1.87 | 63 | 5 | ok 5 outputs |
| M11 prints | 5000 | C:optimistic | never | 1 | 22.06 | | | 1.00 | 0 | 0 | ref 5 outputs |
| M11 prints | 5000 | C:optimistic | lazy | 1 | 24.56 | 1.00 | 1.00 | 1.11 | 0 | 0 | ok 5 outputs |
| M11 prints | 5000 | C:optimistic | lazy | 2 | 14.11 | 1.74 | 0.87 | 0.64 | 11 | 4 | ok 5 outputs |
| M11 prints | 5000 | C:optimistic | lazy | 4 | 8.79 | 2.79 | 0.70 | 0.40 | 27 | 4 | ok 5 outputs |
| M11 prints | 5000 | C:optimistic | lazy-stop | 1 | 23.54 | 1.00 | 1.00 | 1.07 | 0 | 0 | ok 5 outputs |
| M11 prints | 5000 | C:optimistic | lazy-stop | 2 | 14.47 | 1.63 | 0.81 | 0.66 | 13 | 11 | ok 5 outputs |
| M11 prints | 5000 | C:optimistic | lazy-stop | 4 | 9.94 | 2.37 | 0.59 | 0.45 | 24 | 7 | ok 5 outputs |
| M11 prints | 5000 | C:optimistic | always | 1 | 21.50 | 1.00 | 1.00 | 0.97 | 63 | 5 | ok 5 outputs |
| M11 prints | 5000 | C:optimistic | always | 2 | 12.78 | 1.68 | 0.84 | 0.58 | 63 | 5 | ok 5 outputs |
| M11 prints | 5000 | C:optimistic | always | 4 | 8.05 | 2.67 | 0.67 | 0.37 | 63 | 5 | ok 5 outputs |
| M11 prints | 5000 | A:locked | always | 1 | 19.56 | 1.00 | 1.00 | 0.89 | 63 | 5 | ok 5 outputs |
| M11 prints | 5000 | A:locked | always | 2 | 23.82 | 0.82 | 0.41 | 1.08 | 63 | 5 | ok 5 outputs |
| M11 prints | 5000 | A:locked | always | 4 | 35.96 | 0.54 | 0.14 | 1.63 | 63 | 5 | ok 5 outputs |
| M11 defines | 1000 | C:optimistic | never | 1 | 3.15 | | | 1.00 | 0 | 0 | ref 1 outputs |
| M11 defines | 1000 | C:optimistic | lazy | 1 | 3.10 | 1.00 | 1.00 | 0.98 | 0 | 0 | ok 1 outputs |
| M11 defines | 1000 | C:optimistic | lazy | 2 | 2.68 | 1.15 | 0.58 | 0.85 | 11 | 44 | ok 1 outputs |
| M11 defines | 1000 | C:optimistic | lazy | 4 | 2.41 | 1.29 | 0.32 | 0.76 | 27 | 44 | ok 1 outputs |
| M11 defines | 1000 | C:optimistic | lazy-stop | 1 | 3.12 | 1.00 | 1.00 | 0.99 | 0 | 0 | ok 1 outputs |
| M11 defines | 1000 | C:optimistic | lazy-stop | 2 | 2.58 | 1.21 | 0.60 | 0.82 | 11 | 44 | ok 1 outputs |
| M11 defines | 1000 | C:optimistic | lazy-stop | 4 | 2.29 | 1.36 | 0.34 | 0.73 | 27 | 44 | ok 1 outputs |
| M11 defines | 1000 | C:optimistic | always | 1 | 3.65 | 1.00 | 1.00 | 1.16 | 14880 | 44 | ok 1 outputs |
| M11 defines | 1000 | C:optimistic | always | 2 | 3.05 | 1.20 | 0.60 | 0.97 | 14567 | 44 | ok 1 outputs |
| M11 defines | 1000 | C:optimistic | always | 4 | 2.89 | 1.26 | 0.32 | 0.92 | 14842 | 44 | ok 1 outputs |
| M11 defines | 1000 | A:locked | always | 1 | 3.68 | 1.00 | 1.00 | 1.17 | 14880 | 44 | ok 1 outputs |
| M11 defines | 1000 | A:locked | always | 2 | 4.60 | 0.80 | 0.40 | 1.46 | 14579 | 44 | ok 1 outputs |
| M11 defines | 1000 | A:locked | always | 4 | 6.55 | 0.56 | 0.14 | 2.08 | 14602 | 44 | ok 1 outputs |
| M11 defines | 5000 | C:optimistic | never | 1 | 18.16 | | | 1.00 | 0 | 0 | ref 1 outputs |
| M11 defines | 5000 | C:optimistic | lazy | 1 | 17.13 | 1.00 | 1.00 | 0.94 | 0 | 0 | ok 1 outputs |
| M11 defines | 5000 | C:optimistic | lazy | 2 | 14.20 | 1.21 | 0.60 | 0.78 | 11 | 44 | ok 1 outputs |
| M11 defines | 5000 | C:optimistic | lazy | 4 | 11.66 | 1.47 | 0.37 | 0.64 | 26 | 44 | ok 1 outputs |
| M11 defines | 5000 | C:optimistic | lazy-stop | 1 | 17.63 | 1.00 | 1.00 | 0.97 | 0 | 0 | ok 1 outputs |
| M11 defines | 5000 | C:optimistic | lazy-stop | 2 | 14.54 | 1.21 | 0.61 | 0.80 | 11 | 44 | ok 1 outputs |
| M11 defines | 5000 | C:optimistic | lazy-stop | 4 | 12.02 | 1.47 | 0.37 | 0.66 | 24 | 44 | ok 1 outputs |
| M11 defines | 5000 | C:optimistic | always | 1 | 20.24 | 1.00 | 1.00 | 1.12 | 73880 | 44 | ok 1 outputs |
| M11 defines | 5000 | C:optimistic | always | 2 | 16.72 | 1.21 | 0.61 | 0.92 | 72562 | 44 | ok 1 outputs |
| M11 defines | 5000 | C:optimistic | always | 4 | 14.98 | 1.35 | 0.34 | 0.82 | 73858 | 44 | ok 1 outputs |
| M11 defines | 5000 | A:locked | always | 1 | 18.92 | 1.00 | 1.00 | 1.04 | 73880 | 44 | ok 1 outputs |
| M11 defines | 5000 | A:locked | always | 2 | 23.01 | 0.82 | 0.41 | 1.27 | 72529 | 44 | ok 1 outputs |
| M11 defines | 5000 | A:locked | always | 4 | 33.52 | 0.56 | 0.14 | 1.85 | 72574 | 44 | ok 1 outputs |
| M11 nested | 1000 | C:optimistic | never | 1 | 1.87 | | | 1.00 | 0 | 0 | ref 34 outputs |
| M11 nested | 1000 | C:optimistic | lazy | 1 | 1.85 | 1.00 | 1.00 | 0.99 | 0 | 0 | ok 34 outputs |
| M11 nested | 1000 | C:optimistic | lazy | 2 | 1.87 | 0.99 | 0.50 | 0.99 | 1 | 1 | ok 34 outputs |
| M11 nested | 1000 | C:optimistic | lazy | 4 | 1.91 | 0.97 | 0.24 | 1.02 | 1 | 1 | ok 34 outputs |
| M11 nested | 1000 | C:optimistic | lazy-stop | 1 | 1.87 | 1.00 | 1.00 | 1.00 | 0 | 0 | ok 34 outputs |
| M11 nested | 1000 | C:optimistic | lazy-stop | 2 | 1.90 | 0.99 | 0.49 | 1.01 | 1 | 2 | ok 34 outputs |
| M11 nested | 1000 | C:optimistic | lazy-stop | 4 | 1.89 | 0.99 | 0.25 | 1.01 | 1 | 2 | ok 34 outputs |
| M11 nested | 1000 | C:optimistic | always | 1 | 2.43 | 1.00 | 1.00 | 1.30 | 8848 | 3630 | ok 34 outputs |
| M11 nested | 1000 | C:optimistic | always | 2 | 1.66 | 1.46 | 0.73 | 0.89 | 8853 | 3630 | ok 34 outputs |
| M11 nested | 1000 | C:optimistic | always | 4 | 1.33 | 1.83 | 0.46 | 0.71 | 8853 | 3630 | ok 34 outputs |
| M11 nested | 1000 | A:locked | always | 1 | 2.65 | 1.00 | 1.00 | 1.41 | 8848 | 3630 | ok 34 outputs |
| M11 nested | 1000 | A:locked | always | 2 | 2.16 | 1.23 | 0.61 | 1.15 | 8853 | 3630 | ok 34 outputs |
| M11 nested | 1000 | A:locked | always | 4 | 2.52 | 1.05 | 0.26 | 1.34 | 8853 | 3630 | ok 34 outputs |
| M11 nested | 5000 | C:optimistic | never | 1 | 7.63 | | | 1.00 | 0 | 0 | ref 34 outputs |
| M11 nested | 5000 | C:optimistic | lazy | 1 | 7.56 | 1.00 | 1.00 | 0.99 | 0 | 0 | ok 34 outputs |
| M11 nested | 5000 | C:optimistic | lazy | 2 | 7.84 | 0.96 | 0.48 | 1.03 | 10 | 7 | ok 34 outputs |
| M11 nested | 5000 | C:optimistic | lazy | 4 | 7.88 | 0.96 | 0.24 | 1.03 | 10 | 7 | ok 34 outputs |
| M11 nested | 5000 | C:optimistic | lazy-stop | 1 | 7.44 | 1.00 | 1.00 | 0.97 | 0 | 0 | ok 34 outputs |
| M11 nested | 5000 | C:optimistic | lazy-stop | 2 | 7.73 | 0.96 | 0.48 | 1.01 | 10 | 7 | ok 34 outputs |
| M11 nested | 5000 | C:optimistic | lazy-stop | 4 | 7.62 | 0.98 | 0.24 | 1.00 | 10 | 7 | ok 34 outputs |
| M11 nested | 5000 | C:optimistic | always | 1 | 8.52 | 1.00 | 1.00 | 1.12 | 18685 | 3630 | ok 34 outputs |
| M11 nested | 5000 | C:optimistic | always | 2 | 5.91 | 1.44 | 0.72 | 0.77 | 18661 | 3630 | ok 34 outputs |
| M11 nested | 5000 | C:optimistic | always | 4 | 5.53 | 1.54 | 0.38 | 0.72 | 19385 | 3630 | ok 34 outputs |
| M11 nested | 5000 | A:locked | always | 1 | 9.06 | 1.00 | 1.00 | 1.19 | 18685 | 3630 | ok 34 outputs |
| M11 nested | 5000 | A:locked | always | 2 | 8.38 | 1.08 | 0.54 | 1.10 | 18681 | 3630 | ok 34 outputs |
| M11 nested | 5000 | A:locked | always | 4 | 10.93 | 0.83 | 0.21 | 1.43 | 18138 | 3630 | ok 34 outputs |
| LIMIT | fuel 10^5, interrupt | C:optimistic | never | 1 | fuel used by the root task 102400 | ok |
| LIMIT | fuel 10^5, interrupt | C:optimistic | lazy | 1 | fuel used by the root task 102400 | ok |
| LIMIT | fuel 10^5, interrupt | C:optimistic | always | 1 | fuel used by the root task 353955 | ok |
| LIMIT | fuel 10^5, interrupt | C:optimistic | never | 2 | fuel used by the root task 102400 | ok |
| LIMIT | fuel 10^5, interrupt | C:optimistic | lazy | 2 | fuel used by the root task 126976 | ok |
| LIMIT | fuel 10^5, interrupt | C:optimistic | always | 2 | fuel used by the root task 353955 | ok |
| LIMIT | fuel 10^5, interrupt | C:optimistic | never | 4 | fuel used by the root task 102400 | ok |
| LIMIT | fuel 10^5, interrupt | C:optimistic | lazy | 4 | fuel used by the root task 126976 | ok |
| LIMIT | fuel 10^5, interrupt | C:optimistic | always | 4 | fuel used by the root task 360448 | ok |

| case | size | store | phase | median ms | vs hc+imm | check |
|---|---|---|---|---|---|---|
| R10 replace | 10000 | hc+imm (spike 1) | build | 0.41 | 1.00 | ok |
| R10 replace | 10000 | hc+imm (spike 1) | replace | 0.60 | 1.00 | ok |
| R10 replace | 10000 | A:locked | build | 0.50 | 1.23 | ok |
| R10 replace | 10000 | A:locked | replace | 0.63 | 1.07 | ok |
| R10 replace | 10000 | B:sharded | build | 0.52 | 1.27 | ok |
| R10 replace | 10000 | B:sharded | replace | 0.65 | 1.09 | ok |
| R10 replace | 10000 | C:optimistic | build | 0.48 | 1.16 | ok |
| R10 replace | 10000 | C:optimistic | replace | 0.59 | 1.00 | ok |
| R10 replace | 100000 | hc+imm (spike 1) | build | 4.71 | 1.00 | ok |
| R10 replace | 100000 | hc+imm (spike 1) | replace | 7.95 | 1.00 | ok |
| R10 replace | 100000 | A:locked | build | 4.61 | 0.98 | ok |
| R10 replace | 100000 | A:locked | replace | 6.45 | 0.81 | ok |
| R10 replace | 100000 | B:sharded | build | 4.83 | 1.02 | ok |
| R10 replace | 100000 | B:sharded | replace | 6.61 | 0.83 | ok |
| R10 replace | 100000 | C:optimistic | build | 4.69 | 0.99 | ok |
| R10 replace | 100000 | C:optimistic | replace | 7.01 | 0.88 | ok |
| R11 eval | 10000 | hc+imm (spike 1) | build | 0.41 | 1.00 | ok |
| R11 eval | 10000 | hc+imm (spike 1) | eval | 0.76 | 1.00 | ok |
| R11 eval | 10000 | hc+imm (spike 1) | re-eval (unrelated change) | 0.73 | 1.00 | ok |
| R11 eval | 10000 | hc+imm (spike 1) | re-eval (s3 changed) | 0.72 | 1.00 | ok |
| R11 eval | 10000 | A:locked | build | 0.49 | 1.19 | ok |
| R11 eval | 10000 | A:locked | eval | 0.76 | 0.99 | ok |
| R11 eval | 10000 | A:locked | re-eval (unrelated change) | 0.71 | 0.97 | ok |
| R11 eval | 10000 | A:locked | re-eval (s3 changed) | 0.72 | 1.00 | ok |
| R11 eval | 10000 | B:sharded | build | 0.51 | 1.24 | ok |
| R11 eval | 10000 | B:sharded | eval | 0.78 | 1.03 | ok |
| R11 eval | 10000 | B:sharded | re-eval (unrelated change) | 0.74 | 1.01 | ok |
| R11 eval | 10000 | B:sharded | re-eval (s3 changed) | 0.74 | 1.02 | ok |
| R11 eval | 10000 | C:optimistic | build | 0.47 | 1.14 | ok |
| R11 eval | 10000 | C:optimistic | eval | 0.71 | 0.94 | ok |
| R11 eval | 10000 | C:optimistic | re-eval (unrelated change) | 0.68 | 0.93 | ok |
| R11 eval | 10000 | C:optimistic | re-eval (s3 changed) | 0.67 | 0.92 | ok |
| R11 eval | 100000 | hc+imm (spike 1) | build | 5.06 | 1.00 | ok |
| R11 eval | 100000 | hc+imm (spike 1) | eval | 7.90 | 1.00 | ok |
| R11 eval | 100000 | hc+imm (spike 1) | re-eval (unrelated change) | 7.15 | 1.00 | ok |
| R11 eval | 100000 | hc+imm (spike 1) | re-eval (s3 changed) | 7.15 | 1.00 | ok |
| R11 eval | 100000 | A:locked | build | 4.94 | 0.98 | ok |
| R11 eval | 100000 | A:locked | eval | 7.60 | 0.96 | ok |
| R11 eval | 100000 | A:locked | re-eval (unrelated change) | 7.27 | 1.02 | ok |
| R11 eval | 100000 | A:locked | re-eval (s3 changed) | 7.30 | 1.02 | ok |
| R11 eval | 100000 | B:sharded | build | 5.02 | 0.99 | ok |
| R11 eval | 100000 | B:sharded | eval | 7.75 | 0.98 | ok |
| R11 eval | 100000 | B:sharded | re-eval (unrelated change) | 7.38 | 1.03 | ok |
| R11 eval | 100000 | B:sharded | re-eval (s3 changed) | 7.41 | 1.04 | ok |
| R11 eval | 100000 | C:optimistic | build | 4.75 | 0.94 | ok |
| R11 eval | 100000 | C:optimistic | eval | 8.41 | 1.06 | ok |
| R11 eval | 100000 | C:optimistic | re-eval (unrelated change) | 8.04 | 1.12 | ok |
| R11 eval | 100000 | C:optimistic | re-eval (s3 changed) | 8.02 | 1.12 | ok |

all checks ok
