| case | size | store | split | threads | median ms | speedup | efficiency | vs seq | splits | redos | check |
|---|---|---|---|---|---|---|---|---|---|---|---|
| M11 fuzz | 100 | C:optimistic | never | 1 | 13.72 | | | 1.00 | 0 | 0 | ref  |
| M11 fuzz | 100 | C:optimistic | lazy | 1 | 13.13 | 1.00 | 1.00 | 0.96 | 0 | 0 | ok  |
| M11 fuzz | 100 | C:optimistic | lazy | 2 | 13.70 | 0.96 | 0.48 | 1.00 | 2 | 2 | ok  |
| M11 fuzz | 100 | C:optimistic | lazy | 4 | 13.84 | 0.95 | 0.24 | 1.01 | 2 | 2 | ok  |
| M11 fuzz | 100 | C:optimistic | lazy | 8 | 14.98 | 0.88 | 0.11 | 1.09 | 2 | 2 | ok  |
| M11 fuzz | 100 | C:optimistic | lazy | 16 | 13.47 | 0.97 | 0.06 | 0.98 | 2 | 2 | ok  |
| M11 fuzz | 100 | C:optimistic | always | 1 | 23.24 | 1.00 | 1.00 | 1.69 | 134314 | 66463 | ok  |
| M11 fuzz | 100 | C:optimistic | always | 2 | 16.11 | 1.44 | 0.72 | 1.17 | 134432 | 66463 | ok  |
| M11 fuzz | 100 | C:optimistic | always | 4 | 13.76 | 1.69 | 0.42 | 1.00 | 134290 | 66463 | ok  |
| M11 fuzz | 100 | C:optimistic | always | 8 | 17.36 | 1.34 | 0.17 | 1.27 | 134302 | 66463 | ok  |
| M11 fuzz | 100 | C:optimistic | always | 16 | 25.91 | 0.90 | 0.06 | 1.89 | 134337 | 66463 | ok  |
| M11 fuzz | 100 | B:sharded | always | 1 | 24.41 | 1.00 | 1.00 | 1.78 | 134314 | 66463 | ok  |
| M11 fuzz | 100 | B:sharded | always | 2 | 16.88 | 1.45 | 0.72 | 1.23 | 134433 | 66463 | ok  |
| M11 fuzz | 100 | B:sharded | always | 4 | 15.08 | 1.62 | 0.40 | 1.10 | 134354 | 66463 | ok  |
| M11 fuzz | 100 | B:sharded | always | 8 | 17.48 | 1.40 | 0.17 | 1.27 | 134344 | 66463 | ok  |
| M11 fuzz | 100 | B:sharded | always | 16 | 28.70 | 0.85 | 0.05 | 2.09 | 134334 | 66463 | ok  |
| M11 fuzz | 300 | C:optimistic | never | 1 | 23.25 | | | 1.00 | 0 | 0 | ref  |
| M11 fuzz | 300 | C:optimistic | lazy | 1 | 24.24 | 1.00 | 1.00 | 1.04 | 0 | 0 | ok  |
| M11 fuzz | 300 | C:optimistic | lazy | 2 | 23.24 | 1.04 | 0.52 | 1.00 | 3 | 2 | ok  |
| M11 fuzz | 300 | C:optimistic | lazy | 4 | 24.42 | 0.99 | 0.25 | 1.05 | 3 | 2 | ok  |
| M11 fuzz | 300 | C:optimistic | lazy | 8 | 24.50 | 0.99 | 0.12 | 1.05 | 3 | 2 | ok  |
| M11 fuzz | 300 | C:optimistic | lazy | 16 | 24.51 | 0.99 | 0.06 | 1.05 | 3 | 2 | ok  |
| M11 fuzz | 300 | C:optimistic | always | 1 | 42.38 | 1.00 | 1.00 | 1.82 | 176404 | 66463 | ok  |
| M11 fuzz | 300 | C:optimistic | always | 2 | 29.50 | 1.44 | 0.72 | 1.27 | 176110 | 66463 | ok  |
| M11 fuzz | 300 | C:optimistic | always | 4 | 27.16 | 1.56 | 0.39 | 1.17 | 176492 | 66463 | ok  |
| M11 fuzz | 300 | C:optimistic | always | 8 | 32.15 | 1.32 | 0.16 | 1.38 | 176437 | 66463 | ok  |
| M11 fuzz | 300 | C:optimistic | always | 16 | 42.80 | 0.99 | 0.06 | 1.84 | 176476 | 66463 | ok  |
| M11 fuzz | 300 | B:sharded | always | 1 | 43.89 | 1.00 | 1.00 | 1.89 | 176404 | 66463 | ok  |
| M11 fuzz | 300 | B:sharded | always | 2 | 30.95 | 1.42 | 0.71 | 1.33 | 176033 | 66463 | ok  |
| M11 fuzz | 300 | B:sharded | always | 4 | 27.74 | 1.58 | 0.40 | 1.19 | 176317 | 66463 | ok  |
| M11 fuzz | 300 | B:sharded | always | 8 | 32.23 | 1.36 | 0.17 | 1.39 | 176111 | 66463 | ok  |
| M11 fuzz | 300 | B:sharded | always | 16 | 46.06 | 0.95 | 0.06 | 1.98 | 175990 | 66463 | ok  |
| M11 fuzz | 1000 | C:optimistic | never | 1 | 58.01 | | | 1.00 | 0 | 0 | ref  |
| M11 fuzz | 1000 | C:optimistic | lazy | 1 | 57.94 | 1.00 | 1.00 | 1.00 | 0 | 0 | ok  |
| M11 fuzz | 1000 | C:optimistic | lazy | 2 | 57.38 | 1.01 | 0.50 | 0.99 | 24 | 26 | ok  |
| M11 fuzz | 1000 | C:optimistic | lazy | 4 | 57.50 | 1.01 | 0.25 | 0.99 | 24 | 26 | ok  |
| M11 fuzz | 1000 | C:optimistic | lazy | 8 | 59.81 | 0.97 | 0.12 | 1.03 | 24 | 26 | ok  |
| M11 fuzz | 1000 | C:optimistic | lazy | 16 | 57.73 | 1.00 | 0.06 | 1.00 | 24 | 26 | ok  |
| M11 fuzz | 1000 | C:optimistic | always | 1 | 92.92 | 1.00 | 1.00 | 1.60 | 319891 | 66463 | ok  |
| M11 fuzz | 1000 | C:optimistic | always | 2 | 67.55 | 1.38 | 0.69 | 1.16 | 313440 | 66463 | ok  |
| M11 fuzz | 1000 | C:optimistic | always | 4 | 59.88 | 1.55 | 0.39 | 1.03 | 315680 | 66463 | ok  |
| M11 fuzz | 1000 | C:optimistic | always | 8 | 74.92 | 1.24 | 0.16 | 1.29 | 312257 | 66463 | ok  |
| M11 fuzz | 1000 | C:optimistic | always | 16 | 97.83 | 0.95 | 0.06 | 1.69 | 313184 | 66463 | ok  |
| M11 fuzz | 1000 | B:sharded | always | 1 | 94.57 | 1.00 | 1.00 | 1.63 | 319891 | 66463 | ok  |
| M11 fuzz | 1000 | B:sharded | always | 2 | 70.98 | 1.33 | 0.67 | 1.22 | 313430 | 66463 | ok  |
| M11 fuzz | 1000 | B:sharded | always | 4 | 65.00 | 1.45 | 0.36 | 1.12 | 315721 | 66463 | ok  |
| M11 fuzz | 1000 | B:sharded | always | 8 | 79.40 | 1.19 | 0.15 | 1.37 | 315084 | 66463 | ok  |
| M11 fuzz | 1000 | B:sharded | always | 16 | 95.01 | 1.00 | 0.06 | 1.64 | 312323 | 66463 | ok  |
| M11 prints | 1000 | C:optimistic | never | 1 | 4.29 | | | 1.00 | 0 | 0 | ref 5 outputs |
| M11 prints | 1000 | C:optimistic | lazy | 1 | 4.24 | 1.00 | 1.00 | 0.99 | 0 | 0 | ok 5 outputs |
| M11 prints | 1000 | C:optimistic | lazy | 2 | 2.26 | 1.88 | 0.94 | 0.53 | 11 | 4 | ok 5 outputs |
| M11 prints | 1000 | C:optimistic | lazy | 4 | 1.46 | 2.91 | 0.73 | 0.34 | 23 | 4 | ok 5 outputs |
| M11 prints | 1000 | C:optimistic | lazy | 8 | 0.98 | 4.32 | 0.54 | 0.23 | 32 | 4 | ok 5 outputs |
| M11 prints | 1000 | C:optimistic | lazy | 16 | 1.07 | 3.97 | 0.25 | 0.25 | 39 | 5 | ok 5 outputs |
| M11 prints | 1000 | C:optimistic | lazy-stop | 1 | 4.20 | 1.00 | 1.00 | 0.98 | 0 | 0 | ok 5 outputs |
| M11 prints | 1000 | C:optimistic | lazy-stop | 2 | 2.43 | 1.73 | 0.86 | 0.57 | 12 | 11 | ok 5 outputs |
| M11 prints | 1000 | C:optimistic | lazy-stop | 4 | 1.64 | 2.56 | 0.64 | 0.38 | 25 | 7 | ok 5 outputs |
| M11 prints | 1000 | C:optimistic | lazy-stop | 8 | 1.28 | 3.28 | 0.41 | 0.30 | 37 | 7 | ok 5 outputs |
| M11 prints | 1000 | C:optimistic | lazy-stop | 16 | 1.32 | 3.18 | 0.20 | 0.31 | 36 | 8 | ok 5 outputs |
| M11 prints | 1000 | C:optimistic | always | 1 | 3.23 | 1.00 | 1.00 | 0.75 | 63 | 5 | ok 5 outputs |
| M11 prints | 1000 | C:optimistic | always | 2 | 2.12 | 1.52 | 0.76 | 0.49 | 63 | 5 | ok 5 outputs |
| M11 prints | 1000 | C:optimistic | always | 4 | 1.34 | 2.40 | 0.60 | 0.31 | 63 | 5 | ok 5 outputs |
| M11 prints | 1000 | C:optimistic | always | 8 | 0.97 | 3.33 | 0.42 | 0.23 | 63 | 5 | ok 5 outputs |
| M11 prints | 1000 | C:optimistic | always | 16 | 0.87 | 3.71 | 0.23 | 0.20 | 63 | 5 | ok 5 outputs |
| M11 prints | 1000 | A:locked | always | 1 | 3.18 | 1.00 | 1.00 | 0.74 | 63 | 5 | ok 5 outputs |
| M11 prints | 1000 | A:locked | always | 2 | 4.48 | 0.71 | 0.35 | 1.05 | 63 | 5 | ok 5 outputs |
| M11 prints | 1000 | A:locked | always | 4 | 7.24 | 0.44 | 0.11 | 1.69 | 63 | 5 | ok 5 outputs |
| M11 prints | 1000 | A:locked | always | 8 | 16.31 | 0.20 | 0.02 | 3.81 | 63 | 5 | ok 5 outputs |
| M11 prints | 1000 | A:locked | always | 16 | 28.02 | 0.11 | 0.01 | 6.54 | 63 | 5 | ok 5 outputs |
| M11 prints | 5000 | C:optimistic | never | 1 | 28.79 | | | 1.00 | 0 | 0 | ref 5 outputs |
| M11 prints | 5000 | C:optimistic | lazy | 1 | 27.68 | 1.00 | 1.00 | 0.96 | 0 | 0 | ok 5 outputs |
| M11 prints | 5000 | C:optimistic | lazy | 2 | 15.34 | 1.80 | 0.90 | 0.53 | 11 | 4 | ok 5 outputs |
| M11 prints | 5000 | C:optimistic | lazy | 4 | 10.19 | 2.72 | 0.68 | 0.35 | 26 | 4 | ok 5 outputs |
| M11 prints | 5000 | C:optimistic | lazy | 8 | 7.20 | 3.84 | 0.48 | 0.25 | 35 | 5 | ok 5 outputs |
| M11 prints | 5000 | C:optimistic | lazy | 16 | 6.12 | 4.52 | 0.28 | 0.21 | 42 | 5 | ok 5 outputs |
| M11 prints | 5000 | C:optimistic | lazy-stop | 1 | 29.37 | 1.00 | 1.00 | 1.02 | 0 | 0 | ok 5 outputs |
| M11 prints | 5000 | C:optimistic | lazy-stop | 2 | 17.55 | 1.67 | 0.84 | 0.61 | 12 | 11 | ok 5 outputs |
| M11 prints | 5000 | C:optimistic | lazy-stop | 4 | 10.93 | 2.69 | 0.67 | 0.38 | 26 | 7 | ok 5 outputs |
| M11 prints | 5000 | C:optimistic | lazy-stop | 8 | 7.86 | 3.74 | 0.47 | 0.27 | 37 | 6 | ok 5 outputs |
| M11 prints | 5000 | C:optimistic | lazy-stop | 16 | 7.05 | 4.17 | 0.26 | 0.24 | 40 | 7 | ok 5 outputs |
| M11 prints | 5000 | C:optimistic | always | 1 | 23.06 | 1.00 | 1.00 | 0.80 | 63 | 5 | ok 5 outputs |
| M11 prints | 5000 | C:optimistic | always | 2 | 14.39 | 1.60 | 0.80 | 0.50 | 63 | 5 | ok 5 outputs |
| M11 prints | 5000 | C:optimistic | always | 4 | 9.06 | 2.54 | 0.64 | 0.31 | 63 | 5 | ok 5 outputs |
| M11 prints | 5000 | C:optimistic | always | 8 | 6.07 | 3.80 | 0.48 | 0.21 | 63 | 5 | ok 5 outputs |
| M11 prints | 5000 | C:optimistic | always | 16 | 4.82 | 4.78 | 0.30 | 0.17 | 63 | 5 | ok 5 outputs |
| M11 prints | 5000 | A:locked | always | 1 | 16.70 | 1.00 | 1.00 | 0.58 | 63 | 5 | ok 5 outputs |
| M11 prints | 5000 | A:locked | always | 2 | 24.11 | 0.69 | 0.35 | 0.84 | 63 | 5 | ok 5 outputs |
| M11 prints | 5000 | A:locked | always | 4 | 38.84 | 0.43 | 0.11 | 1.35 | 63 | 5 | ok 5 outputs |
| M11 prints | 5000 | A:locked | always | 8 | 84.24 | 0.20 | 0.02 | 2.93 | 63 | 5 | ok 5 outputs |
| M11 prints | 5000 | A:locked | always | 16 | 142.93 | 0.12 | 0.01 | 4.96 | 63 | 5 | ok 5 outputs |
| M11 prints | 20000 | C:optimistic | never | 1 | 125.57 | | | 1.00 | 0 | 0 | ref 5 outputs |
| M11 prints | 20000 | C:optimistic | lazy | 1 | 123.53 | 1.00 | 1.00 | 0.98 | 0 | 0 | ok 5 outputs |
| M11 prints | 20000 | C:optimistic | lazy | 2 | 81.77 | 1.51 | 0.76 | 0.65 | 11 | 4 | ok 5 outputs |
| M11 prints | 20000 | C:optimistic | lazy | 4 | 49.72 | 2.48 | 0.62 | 0.40 | 24 | 4 | ok 5 outputs |
| M11 prints | 20000 | C:optimistic | lazy | 8 | 33.53 | 3.68 | 0.46 | 0.27 | 36 | 4 | ok 5 outputs |
| M11 prints | 20000 | C:optimistic | lazy | 16 | 26.18 | 4.72 | 0.29 | 0.21 | 42 | 5 | ok 5 outputs |
| M11 prints | 20000 | C:optimistic | lazy-stop | 1 | 125.75 | 1.00 | 1.00 | 1.00 | 0 | 0 | ok 5 outputs |
| M11 prints | 20000 | C:optimistic | lazy-stop | 2 | 74.51 | 1.69 | 0.84 | 0.59 | 12 | 11 | ok 5 outputs |
| M11 prints | 20000 | C:optimistic | lazy-stop | 4 | 49.20 | 2.56 | 0.64 | 0.39 | 26 | 7 | ok 5 outputs |
| M11 prints | 20000 | C:optimistic | lazy-stop | 8 | 34.19 | 3.68 | 0.46 | 0.27 | 37 | 6 | ok 5 outputs |
| M11 prints | 20000 | C:optimistic | lazy-stop | 16 | 30.80 | 4.08 | 0.26 | 0.25 | 40 | 7 | ok 5 outputs |
| M11 prints | 20000 | C:optimistic | always | 1 | 103.94 | 1.00 | 1.00 | 0.83 | 63 | 5 | ok 5 outputs |
| M11 prints | 20000 | C:optimistic | always | 2 | 66.33 | 1.57 | 0.78 | 0.53 | 63 | 5 | ok 5 outputs |
| M11 prints | 20000 | C:optimistic | always | 4 | 41.09 | 2.53 | 0.63 | 0.33 | 63 | 5 | ok 5 outputs |
| M11 prints | 20000 | C:optimistic | always | 8 | 27.85 | 3.73 | 0.47 | 0.22 | 63 | 5 | ok 5 outputs |
| M11 prints | 20000 | C:optimistic | always | 16 | 21.79 | 4.77 | 0.30 | 0.17 | 63 | 5 | ok 5 outputs |
| M11 prints | 20000 | A:locked | always | 1 | 99.68 | 1.00 | 1.00 | 0.79 | 63 | 5 | ok 5 outputs |
| M11 prints | 20000 | A:locked | always | 2 | 120.13 | 0.83 | 0.41 | 0.96 | 63 | 5 | ok 5 outputs |
| M11 prints | 20000 | A:locked | always | 4 | 191.64 | 0.52 | 0.13 | 1.53 | 63 | 5 | ok 5 outputs |
| M11 prints | 20000 | A:locked | always | 8 | 360.16 | 0.28 | 0.03 | 2.87 | 63 | 5 | ok 5 outputs |
| M11 prints | 20000 | A:locked | always | 16 | 614.86 | 0.16 | 0.01 | 4.90 | 63 | 5 | ok 5 outputs |
| M11 defines | 1000 | C:optimistic | never | 1 | 3.64 | | | 1.00 | 0 | 0 | ref 1 outputs |
| M11 defines | 1000 | C:optimistic | lazy | 1 | 3.54 | 1.00 | 1.00 | 0.97 | 0 | 0 | ok 1 outputs |
| M11 defines | 1000 | C:optimistic | lazy | 2 | 2.62 | 1.35 | 0.68 | 0.72 | 11 | 44 | ok 1 outputs |
| M11 defines | 1000 | C:optimistic | lazy | 4 | 2.55 | 1.39 | 0.35 | 0.70 | 24 | 44 | ok 1 outputs |
| M11 defines | 1000 | C:optimistic | lazy | 8 | 2.03 | 1.74 | 0.22 | 0.56 | 33 | 44 | ok 1 outputs |
| M11 defines | 1000 | C:optimistic | lazy | 16 | 2.11 | 1.68 | 0.10 | 0.58 | 33 | 44 | ok 1 outputs |
| M11 defines | 1000 | C:optimistic | lazy-stop | 1 | 3.54 | 1.00 | 1.00 | 0.97 | 0 | 0 | ok 1 outputs |
| M11 defines | 1000 | C:optimistic | lazy-stop | 2 | 2.60 | 1.36 | 0.68 | 0.71 | 11 | 44 | ok 1 outputs |
| M11 defines | 1000 | C:optimistic | lazy-stop | 4 | 2.40 | 1.48 | 0.37 | 0.66 | 23 | 44 | ok 1 outputs |
| M11 defines | 1000 | C:optimistic | lazy-stop | 8 | 2.02 | 1.75 | 0.22 | 0.55 | 32 | 44 | ok 1 outputs |
| M11 defines | 1000 | C:optimistic | lazy-stop | 16 | 1.90 | 1.87 | 0.12 | 0.52 | 32 | 44 | ok 1 outputs |
| M11 defines | 1000 | C:optimistic | always | 1 | 3.83 | 1.00 | 1.00 | 1.05 | 14880 | 44 | ok 1 outputs |
| M11 defines | 1000 | C:optimistic | always | 2 | 3.12 | 1.23 | 0.61 | 0.86 | 14575 | 44 | ok 1 outputs |
| M11 defines | 1000 | C:optimistic | always | 4 | 2.95 | 1.30 | 0.32 | 0.81 | 14890 | 44 | ok 1 outputs |
| M11 defines | 1000 | C:optimistic | always | 8 | 2.99 | 1.28 | 0.16 | 0.82 | 14739 | 44 | ok 1 outputs |
| M11 defines | 1000 | C:optimistic | always | 16 | 4.64 | 0.82 | 0.05 | 1.27 | 15031 | 44 | ok 1 outputs |
| M11 defines | 1000 | A:locked | always | 1 | 3.74 | 1.00 | 1.00 | 1.03 | 14880 | 44 | ok 1 outputs |
| M11 defines | 1000 | A:locked | always | 2 | 4.75 | 0.79 | 0.39 | 1.30 | 14580 | 44 | ok 1 outputs |
| M11 defines | 1000 | A:locked | always | 4 | 6.77 | 0.55 | 0.14 | 1.86 | 14637 | 44 | ok 1 outputs |
| M11 defines | 1000 | A:locked | always | 8 | 13.54 | 0.28 | 0.03 | 3.72 | 14398 | 44 | ok 1 outputs |
| M11 defines | 1000 | A:locked | always | 16 | 22.49 | 0.17 | 0.01 | 6.17 | 14404 | 44 | ok 1 outputs |
| M11 defines | 5000 | C:optimistic | never | 1 | 19.34 | | | 1.00 | 0 | 0 | ref 1 outputs |
| M11 defines | 5000 | C:optimistic | lazy | 1 | 19.38 | 1.00 | 1.00 | 1.00 | 0 | 0 | ok 1 outputs |
| M11 defines | 5000 | C:optimistic | lazy | 2 | 14.57 | 1.33 | 0.66 | 0.75 | 11 | 44 | ok 1 outputs |
| M11 defines | 5000 | C:optimistic | lazy | 4 | 12.41 | 1.56 | 0.39 | 0.64 | 25 | 44 | ok 1 outputs |
| M11 defines | 5000 | C:optimistic | lazy | 8 | 10.15 | 1.91 | 0.24 | 0.52 | 39 | 44 | ok 1 outputs |
| M11 defines | 5000 | C:optimistic | lazy | 16 | 9.67 | 2.00 | 0.13 | 0.50 | 41 | 44 | ok 1 outputs |
| M11 defines | 5000 | C:optimistic | lazy-stop | 1 | 19.78 | 1.00 | 1.00 | 1.02 | 0 | 0 | ok 1 outputs |
| M11 defines | 5000 | C:optimistic | lazy-stop | 2 | 14.62 | 1.35 | 0.68 | 0.76 | 11 | 44 | ok 1 outputs |
| M11 defines | 5000 | C:optimistic | lazy-stop | 4 | 12.41 | 1.59 | 0.40 | 0.64 | 26 | 44 | ok 1 outputs |
| M11 defines | 5000 | C:optimistic | lazy-stop | 8 | 10.23 | 1.93 | 0.24 | 0.53 | 36 | 44 | ok 1 outputs |
| M11 defines | 5000 | C:optimistic | lazy-stop | 16 | 9.86 | 2.01 | 0.13 | 0.51 | 41 | 44 | ok 1 outputs |
| M11 defines | 5000 | C:optimistic | always | 1 | 21.46 | 1.00 | 1.00 | 1.11 | 73880 | 44 | ok 1 outputs |
| M11 defines | 5000 | C:optimistic | always | 2 | 17.02 | 1.26 | 0.63 | 0.88 | 72557 | 44 | ok 1 outputs |
| M11 defines | 5000 | C:optimistic | always | 4 | 15.42 | 1.39 | 0.35 | 0.80 | 74133 | 44 | ok 1 outputs |
| M11 defines | 5000 | C:optimistic | always | 8 | 15.83 | 1.35 | 0.17 | 0.82 | 73081 | 44 | ok 1 outputs |
| M11 defines | 5000 | C:optimistic | always | 16 | 24.85 | 0.86 | 0.05 | 1.28 | 74065 | 44 | ok 1 outputs |
| M11 defines | 5000 | A:locked | always | 1 | 20.28 | 1.00 | 1.00 | 1.05 | 73880 | 44 | ok 1 outputs |
| M11 defines | 5000 | A:locked | always | 2 | 24.00 | 0.85 | 0.42 | 1.24 | 72549 | 44 | ok 1 outputs |
| M11 defines | 5000 | A:locked | always | 4 | 34.53 | 0.59 | 0.15 | 1.79 | 72916 | 44 | ok 1 outputs |
| M11 defines | 5000 | A:locked | always | 8 | 67.73 | 0.30 | 0.04 | 3.50 | 71735 | 44 | ok 1 outputs |
| M11 defines | 5000 | A:locked | always | 16 | 113.05 | 0.18 | 0.01 | 5.85 | 71415 | 44 | ok 1 outputs |
| M11 defines | 20000 | C:optimistic | never | 1 | 86.25 | | | 1.00 | 0 | 0 | ref 1 outputs |
| M11 defines | 20000 | C:optimistic | lazy | 1 | 83.49 | 1.00 | 1.00 | 0.97 | 0 | 0 | ok 1 outputs |
| M11 defines | 20000 | C:optimistic | lazy | 2 | 64.79 | 1.29 | 0.64 | 0.75 | 11 | 44 | ok 1 outputs |
| M11 defines | 20000 | C:optimistic | lazy | 4 | 53.85 | 1.55 | 0.39 | 0.62 | 26 | 44 | ok 1 outputs |
| M11 defines | 20000 | C:optimistic | lazy | 8 | 40.84 | 2.04 | 0.26 | 0.47 | 38 | 44 | ok 1 outputs |
| M11 defines | 20000 | C:optimistic | lazy | 16 | 38.61 | 2.16 | 0.14 | 0.45 | 41 | 44 | ok 1 outputs |
| M11 defines | 20000 | C:optimistic | lazy-stop | 1 | 85.58 | 1.00 | 1.00 | 0.99 | 0 | 0 | ok 1 outputs |
| M11 defines | 20000 | C:optimistic | lazy-stop | 2 | 62.00 | 1.38 | 0.69 | 0.72 | 11 | 44 | ok 1 outputs |
| M11 defines | 20000 | C:optimistic | lazy-stop | 4 | 53.00 | 1.61 | 0.40 | 0.61 | 25 | 44 | ok 1 outputs |
| M11 defines | 20000 | C:optimistic | lazy-stop | 8 | 41.52 | 2.06 | 0.26 | 0.48 | 37 | 44 | ok 1 outputs |
| M11 defines | 20000 | C:optimistic | lazy-stop | 16 | 42.60 | 2.01 | 0.13 | 0.49 | 41 | 44 | ok 1 outputs |
| M11 defines | 20000 | C:optimistic | always | 1 | 95.26 | 1.00 | 1.00 | 1.10 | 295130 | 44 | ok 1 outputs |
| M11 defines | 20000 | C:optimistic | always | 2 | 76.56 | 1.24 | 0.62 | 0.89 | 289816 | 44 | ok 1 outputs |
| M11 defines | 20000 | C:optimistic | always | 4 | 62.98 | 1.51 | 0.38 | 0.73 | 295159 | 44 | ok 1 outputs |
| M11 defines | 20000 | C:optimistic | always | 8 | 62.73 | 1.52 | 0.19 | 0.73 | 291320 | 44 | ok 1 outputs |
| M11 defines | 20000 | C:optimistic | always | 16 | 105.20 | 0.91 | 0.06 | 1.22 | 297227 | 44 | ok 1 outputs |
| M11 defines | 20000 | A:locked | always | 1 | 86.54 | 1.00 | 1.00 | 1.00 | 295130 | 44 | ok 1 outputs |
| M11 defines | 20000 | A:locked | always | 2 | 101.59 | 0.85 | 0.43 | 1.18 | 289554 | 44 | ok 1 outputs |
| M11 defines | 20000 | A:locked | always | 4 | 148.18 | 0.58 | 0.15 | 1.72 | 289547 | 44 | ok 1 outputs |
| M11 defines | 20000 | A:locked | always | 8 | 279.06 | 0.31 | 0.04 | 3.24 | 285724 | 44 | ok 1 outputs |
| M11 defines | 20000 | A:locked | always | 16 | 482.76 | 0.18 | 0.01 | 5.60 | 285358 | 44 | ok 1 outputs |
| M11 nested | 1000 | C:optimistic | never | 1 | 2.11 | | | 1.00 | 0 | 0 | ref 34 outputs |
| M11 nested | 1000 | C:optimistic | lazy | 1 | 2.16 | 1.00 | 1.00 | 1.02 | 0 | 0 | ok 34 outputs |
| M11 nested | 1000 | C:optimistic | lazy | 2 | 2.12 | 1.02 | 0.51 | 1.01 | 1 | 1 | ok 34 outputs |
| M11 nested | 1000 | C:optimistic | lazy | 4 | 2.25 | 0.96 | 0.24 | 1.07 | 1 | 1 | ok 34 outputs |
| M11 nested | 1000 | C:optimistic | lazy | 8 | 2.05 | 1.05 | 0.13 | 0.97 | 1 | 1 | ok 34 outputs |
| M11 nested | 1000 | C:optimistic | lazy | 16 | 1.91 | 1.13 | 0.07 | 0.91 | 1 | 1 | ok 34 outputs |
| M11 nested | 1000 | C:optimistic | lazy-stop | 1 | 1.99 | 1.00 | 1.00 | 0.94 | 0 | 0 | ok 34 outputs |
| M11 nested | 1000 | C:optimistic | lazy-stop | 2 | 1.95 | 1.02 | 0.51 | 0.92 | 1 | 2 | ok 34 outputs |
| M11 nested | 1000 | C:optimistic | lazy-stop | 4 | 1.89 | 1.06 | 0.26 | 0.89 | 1 | 2 | ok 34 outputs |
| M11 nested | 1000 | C:optimistic | lazy-stop | 8 | 2.10 | 0.95 | 0.12 | 1.00 | 1 | 2 | ok 34 outputs |
| M11 nested | 1000 | C:optimistic | lazy-stop | 16 | 1.90 | 1.05 | 0.07 | 0.90 | 1 | 2 | ok 34 outputs |
| M11 nested | 1000 | C:optimistic | always | 1 | 2.52 | 1.00 | 1.00 | 1.19 | 8848 | 3630 | ok 34 outputs |
| M11 nested | 1000 | C:optimistic | always | 2 | 1.70 | 1.48 | 0.74 | 0.80 | 8853 | 3630 | ok 34 outputs |
| M11 nested | 1000 | C:optimistic | always | 4 | 1.37 | 1.83 | 0.46 | 0.65 | 8853 | 3630 | ok 34 outputs |
| M11 nested | 1000 | C:optimistic | always | 8 | 1.43 | 1.76 | 0.22 | 0.68 | 8909 | 3630 | ok 34 outputs |
| M11 nested | 1000 | C:optimistic | always | 16 | 1.85 | 1.36 | 0.09 | 0.88 | 8887 | 3630 | ok 34 outputs |
| M11 nested | 1000 | A:locked | always | 1 | 2.59 | 1.00 | 1.00 | 1.23 | 8848 | 3630 | ok 34 outputs |
| M11 nested | 1000 | A:locked | always | 2 | 2.18 | 1.19 | 0.59 | 1.03 | 8922 | 3630 | ok 34 outputs |
| M11 nested | 1000 | A:locked | always | 4 | 2.55 | 1.02 | 0.25 | 1.21 | 8853 | 3630 | ok 34 outputs |
| M11 nested | 1000 | A:locked | always | 8 | 5.44 | 0.48 | 0.06 | 2.58 | 8853 | 3630 | ok 34 outputs |
| M11 nested | 1000 | A:locked | always | 16 | 8.72 | 0.30 | 0.02 | 4.13 | 8853 | 3630 | ok 34 outputs |
| M11 nested | 5000 | C:optimistic | never | 1 | 8.50 | | | 1.00 | 0 | 0 | ref 34 outputs |
| M11 nested | 5000 | C:optimistic | lazy | 1 | 8.73 | 1.00 | 1.00 | 1.03 | 0 | 0 | ok 34 outputs |
| M11 nested | 5000 | C:optimistic | lazy | 2 | 7.61 | 1.15 | 0.57 | 0.89 | 10 | 7 | ok 34 outputs |
| M11 nested | 5000 | C:optimistic | lazy | 4 | 7.54 | 1.16 | 0.29 | 0.89 | 10 | 7 | ok 34 outputs |
| M11 nested | 5000 | C:optimistic | lazy | 8 | 8.36 | 1.04 | 0.13 | 0.98 | 10 | 7 | ok 34 outputs |
| M11 nested | 5000 | C:optimistic | lazy | 16 | 8.24 | 1.06 | 0.07 | 0.97 | 10 | 7 | ok 34 outputs |
| M11 nested | 5000 | C:optimistic | lazy-stop | 1 | 8.53 | 1.00 | 1.00 | 1.00 | 0 | 0 | ok 34 outputs |
| M11 nested | 5000 | C:optimistic | lazy-stop | 2 | 7.79 | 1.10 | 0.55 | 0.92 | 10 | 7 | ok 34 outputs |
| M11 nested | 5000 | C:optimistic | lazy-stop | 4 | 8.07 | 1.06 | 0.26 | 0.95 | 10 | 7 | ok 34 outputs |
| M11 nested | 5000 | C:optimistic | lazy-stop | 8 | 8.59 | 0.99 | 0.12 | 1.01 | 10 | 7 | ok 34 outputs |
| M11 nested | 5000 | C:optimistic | lazy-stop | 16 | 8.50 | 1.00 | 0.06 | 1.00 | 10 | 7 | ok 34 outputs |
| M11 nested | 5000 | C:optimistic | always | 1 | 8.88 | 1.00 | 1.00 | 1.04 | 18685 | 3630 | ok 34 outputs |
| M11 nested | 5000 | C:optimistic | always | 2 | 6.08 | 1.46 | 0.73 | 0.72 | 18649 | 3630 | ok 34 outputs |
| M11 nested | 5000 | C:optimistic | always | 4 | 5.23 | 1.70 | 0.42 | 0.62 | 19356 | 3630 | ok 34 outputs |
| M11 nested | 5000 | C:optimistic | always | 8 | 4.76 | 1.87 | 0.23 | 0.56 | 18796 | 3630 | ok 34 outputs |
| M11 nested | 5000 | C:optimistic | always | 16 | 5.60 | 1.59 | 0.10 | 0.66 | 18810 | 3630 | ok 34 outputs |
| M11 nested | 5000 | A:locked | always | 1 | 8.26 | 1.00 | 1.00 | 0.97 | 18685 | 3630 | ok 34 outputs |
| M11 nested | 5000 | A:locked | always | 2 | 8.46 | 0.98 | 0.49 | 0.99 | 18731 | 3630 | ok 34 outputs |
| M11 nested | 5000 | A:locked | always | 4 | 11.28 | 0.73 | 0.18 | 1.33 | 17908 | 3630 | ok 34 outputs |
| M11 nested | 5000 | A:locked | always | 8 | 23.22 | 0.36 | 0.04 | 2.73 | 17965 | 3630 | ok 34 outputs |
| M11 nested | 5000 | A:locked | always | 16 | 36.85 | 0.22 | 0.01 | 4.33 | 17921 | 3630 | ok 34 outputs |
| M11 nested | 20000 | C:optimistic | never | 1 | 29.58 | | | 1.00 | 0 | 0 | ref 34 outputs |
| M11 nested | 20000 | C:optimistic | lazy | 1 | 29.59 | 1.00 | 1.00 | 1.00 | 0 | 0 | ok 34 outputs |
| M11 nested | 20000 | C:optimistic | lazy | 2 | 27.59 | 1.07 | 0.54 | 0.93 | 10 | 7 | ok 34 outputs |
| M11 nested | 20000 | C:optimistic | lazy | 4 | 27.29 | 1.08 | 0.27 | 0.92 | 10 | 7 | ok 34 outputs |
| M11 nested | 20000 | C:optimistic | lazy | 8 | 30.09 | 0.98 | 0.12 | 1.02 | 10 | 7 | ok 34 outputs |
| M11 nested | 20000 | C:optimistic | lazy | 16 | 27.54 | 1.07 | 0.07 | 0.93 | 10 | 7 | ok 34 outputs |
| M11 nested | 20000 | C:optimistic | lazy-stop | 1 | 28.90 | 1.00 | 1.00 | 0.98 | 0 | 0 | ok 34 outputs |
| M11 nested | 20000 | C:optimistic | lazy-stop | 2 | 27.69 | 1.04 | 0.52 | 0.94 | 10 | 7 | ok 34 outputs |
| M11 nested | 20000 | C:optimistic | lazy-stop | 4 | 28.00 | 1.03 | 0.26 | 0.95 | 10 | 7 | ok 34 outputs |
| M11 nested | 20000 | C:optimistic | lazy-stop | 8 | 29.70 | 0.97 | 0.12 | 1.00 | 10 | 7 | ok 34 outputs |
| M11 nested | 20000 | C:optimistic | lazy-stop | 16 | 29.32 | 0.99 | 0.06 | 0.99 | 10 | 7 | ok 34 outputs |
| M11 nested | 20000 | C:optimistic | always | 1 | 33.06 | 1.00 | 1.00 | 1.12 | 55095 | 3630 | ok 34 outputs |
| M11 nested | 20000 | C:optimistic | always | 2 | 22.34 | 1.48 | 0.74 | 0.75 | 55854 | 3630 | ok 34 outputs |
| M11 nested | 20000 | C:optimistic | always | 4 | 17.49 | 1.89 | 0.47 | 0.59 | 54935 | 3630 | ok 34 outputs |
| M11 nested | 20000 | C:optimistic | always | 8 | 16.05 | 2.06 | 0.26 | 0.54 | 56964 | 3630 | ok 34 outputs |
| M11 nested | 20000 | C:optimistic | always | 16 | 20.51 | 1.61 | 0.10 | 0.69 | 59326 | 3630 | ok 34 outputs |
| M11 nested | 20000 | A:locked | always | 1 | 27.93 | 1.00 | 1.00 | 0.94 | 55095 | 3630 | ok 34 outputs |
| M11 nested | 20000 | A:locked | always | 2 | 30.14 | 0.93 | 0.46 | 1.02 | 55633 | 3630 | ok 34 outputs |
| M11 nested | 20000 | A:locked | always | 4 | 45.19 | 0.62 | 0.15 | 1.53 | 51658 | 3630 | ok 34 outputs |
| M11 nested | 20000 | A:locked | always | 8 | 90.95 | 0.31 | 0.04 | 3.07 | 51893 | 3630 | ok 34 outputs |
| M11 nested | 20000 | A:locked | always | 16 | 146.31 | 0.19 | 0.01 | 4.95 | 51712 | 3630 | ok 34 outputs |

all checks ok
