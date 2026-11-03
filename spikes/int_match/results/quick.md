|---|---|---|---|---|---|---|---|---|---|---|
| case | n | config | threads | ms | speedup vs seq | vs spike 2 | splits | redos | check | note |
| R4 AC rules | 1000 | spike 2 indexed/smart | 1 | 2.43 | | 1.00 | | | | spike 2's sequential evaluator |
| R4 AC rules | 1000 | ref | 1 | 2.92 |  | 1.20 (s2 check ok) | 0 | 0 | ok | 623 of 1000 subjects rewritten |
| R4 AC rules | 1000 | seq | 1 | 2.69 | 1.00 | 1.10 (s2 check ok) | 0 | 0 | ok | 623 of 1000 subjects rewritten |
| R4 AC rules | 1000 | lazy | 1 | 3.20 | 0.84 |  | 0 | 0 | ok | 623 of 1000 subjects rewritten |
| R4 AC rules | 1000 | lazy | 2 | 1.49 | 1.81 |  | 4 | 0 | ok | 623 of 1000 subjects rewritten |
| R4 AC rules | 1000 | lazy | 4 | 1.58 | 1.71 |  | 4 | 0 | ok | 623 of 1000 subjects rewritten |
| R4 AC rules | 1000 | always | 4 | 1.13 | 2.38 |  | 999 | 0 | ok | 623 of 1000 subjects rewritten |
| R4 AC rules | 1000 | seq hashed-memo | 1 | 4.54 | 0.59 |  | 0 | 0 | ok | 623 of 1000 subjects rewritten |
| R4 AC rules | 1000 | lazy hashed-memo | 4 | 2.14 | 1.25 |  | 4 | 0 | ok | 623 of 1000 subjects rewritten |
| R4 AC rules | 1000 | lazy tiny-memo | 4 | 1.30 | 2.07 |  | 4 | 0 | ok | 623 of 1000 subjects rewritten |
| R4 AC rules | 1000 | lazy locked-index | 4 | 1.61 | 1.67 |  | 4 | 0 | ok | 623 of 1000 subjects rewritten |
| R5 literal rules | 5000 | spike 2 indexed | 1 | 1.40 | | 1.00 | | | | spike 2's sequential evaluator |
| R5 literal rules | 5000 | ref | 1 | 1.66 |  | 1.19 (s2 check ok) | 0 | 0 | ok | 4983 of 10000 calls hit a literal rule |
| R5 literal rules | 5000 | seq | 1 | 1.69 | 1.00 | 1.21 (s2 check ok) | 0 | 0 | ok | 4983 of 10000 calls hit a literal rule |
| R5 literal rules | 5000 | lazy | 1 | 1.86 | 0.91 |  | 0 | 0 | ok | 4983 of 10000 calls hit a literal rule |
| R5 literal rules | 5000 | lazy | 2 | 1.02 | 1.65 |  | 9 | 0 | ok | 4983 of 10000 calls hit a literal rule |
| R5 literal rules | 5000 | lazy | 4 | 0.69 | 2.43 |  | 15 | 0 | ok | 4983 of 10000 calls hit a literal rule |
| R5 literal rules | 5000 | always | 4 | 1.80 | 0.94 |  | 9999 | 0 | ok | 4983 of 10000 calls hit a literal rule |
| R5 literal rules | 5000 | lazy locked-index | 4 | 0.77 | 2.20 |  | 15 | 0 | ok | 4983 of 10000 calls hit a literal rule |
| R13 arg-head rules | 100 | spike 2 arg-head | 1 | 1.25 | | 1.00 | | | | spike 2's sequential evaluator |
| R13 arg-head rules | 100 | ref | 1 | 2.17 |  | 1.74 (s2 check ok) | 0 | 0 | ok | 1049 calls fell through to the generic rule |
| R13 arg-head rules | 100 | seq | 1 | 2.00 | 1.00 | 1.59 (s2 check ok) | 0 | 0 | ok | 1049 calls fell through to the generic rule |
| R13 arg-head rules | 100 | lazy | 1 | 2.04 | 0.98 |  | 0 | 0 | ok | 1049 calls fell through to the generic rule |
| R13 arg-head rules | 100 | lazy | 2 | 1.10 | 1.81 |  | 11 | 0 | ok | 1049 calls fell through to the generic rule |
| R13 arg-head rules | 100 | lazy | 4 | 0.63 | 3.18 |  | 18 | 0 | ok | 1049 calls fell through to the generic rule |
| R13 arg-head rules | 100 | always | 4 | 1.66 | 1.20 |  | 9999 | 0 | ok | 1049 calls fell through to the generic rule |
| R13 arg-head rules | 100 | lazy locked-index | 4 | 0.71 | 2.83 |  | 17 | 0 | ok | 1049 calls fell through to the generic rule |
| R14 read sets | 100000 | spike 2 memo/mask | 1 | 24.43 | | 1.00 | | | | spike 2's sequential evaluator |
| R14 read sets | 100000 | ref | 1 | 59.10 |  | 2.42 (s2 check ok) | 0 | 0 | ok | phases 17.53 / 14.00 / 13.21 / 14.35 ms; result changed |
| R14 read sets | 100000 | seq | 1 | 23.52 | 1.00 | 0.96 (s2 check ok) | 0 | 0 | ok | phases 20.85 / 0.00 / 1.01 / 1.66 ms; result changed |
| R14 read sets | 100000 | lazy | 1 | 20.93 | 1.12 |  | 0 | 0 | ok | phases 18.51 / 0.00 / 0.95 / 1.48 ms; result changed |
| R14 read sets | 100000 | lazy | 2 | 12.86 | 1.83 |  | 48 | 0 | ok | phases 11.16 / 0.00 / 0.79 / 0.91 ms; result changed |
| R14 read sets | 100000 | lazy | 4 | 7.85 | 3.00 |  | 121 | 0 | ok | phases 6.59 / 0.00 / 0.60 / 0.67 ms; result changed |
| R14 read sets | 100000 | always | 4 | 21.07 | 1.12 |  | 299997 | 0 | ok | phases 17.90 / 0.00 / 1.44 / 1.73 ms; result changed |
| R14 read sets | 100000 | seq hashed-memo | 1 | 63.27 | 0.37 |  | 0 | 0 | ok | phases 51.89 / 0.00 / 5.35 / 6.03 ms; result changed |
| R14 read sets | 100000 | lazy hashed-memo | 4 | 19.67 | 1.20 |  | 170 | 0 | ok | phases 16.20 / 0.00 / 1.79 / 1.68 ms; result changed |
| R14 read sets | 100000 | lazy tiny-memo | 4 | 19.15 | 1.23 |  | 141 | 0 | ok | phases 16.22 / 0.00 / 1.63 / 1.30 ms; result changed |
| R14 read sets | 100000 | lazy locked-index | 4 | 8.80 | 2.67 |  | 151 | 0 | ok | phases 7.39 / 0.00 / 0.64 / 0.77 ms; result changed |
| S7 cases | 100000 | spike 2 smart | 1 | 8.22 | | 1.00 | | | | spike 2's sequential evaluator |
| S7 cases | 100000 | ref | 1 | 10.64 |  | 1.29 (s2 check ok) | 0 | 0 | ok | selected [14545, 24930, 5590, 2649, 14292] |
| S7 cases | 100000 | seq | 1 | 10.28 | 1.00 | 1.25 (s2 check ok) | 0 | 0 | ok | selected [14545, 24930, 5590, 2649, 14292] |
| S7 cases | 100000 | lazy | 1 | 10.19 | 1.01 |  | 0 | 0 | ok | selected [14545, 24930, 5590, 2649, 14292] |
| S7 cases | 100000 | lazy | 2 | 5.86 | 1.75 |  | 29 | 0 | ok | selected [14545, 24930, 5590, 2649, 14292] |
| S7 cases | 100000 | lazy | 4 | 3.46 | 2.97 |  | 85 | 0 | ok | selected [14545, 24930, 5590, 2649, 14292] |
| S7 cases | 100000 | always | 4 | 21.96 | 0.47 |  | 499999 | 0 | ok | selected [14545, 24930, 5590, 2649, 14292] |
| S7 cases | 100000 | lazy locked-index | 4 | 3.11 | 3.31 |  | 91 | 0 | ok | selected [14545, 24930, 5590, 2649, 14292] |
| R14e native cond | 20000 | ref | 1 | 16.44 |  |  | 0 | 0 | ok | phases 3.72 / 3.08 / 3.14 / 3.31 / 3.20 ms; hi: 1586 / 1589 / 1589 of 20000; 20001 terms interned by the first evaluation |
| R14e native cond | 20000 | seq | 1 | 7.98 | 1.00 |  | 0 | 0 | ok | phases 4.46 / 0.00 / 0.00 / 3.52 / 0.00 ms; hi: 1586 / 1589 / 1589 of 20000; 20001 terms interned by the first evaluation |
| R14e native cond | 20000 | lazy | 1 | 7.22 | 1.10 |  | 0 | 0 | ok | phases 3.88 / 0.00 / 0.00 / 3.34 / 0.00 ms; hi: 1586 / 1589 / 1589 of 20000; 20001 terms interned by the first evaluation |
| R14e native cond | 20000 | lazy | 2 | 4.38 | 1.82 |  | 22 | 0 | ok | phases 2.26 / 0.00 / 0.00 / 2.12 / 0.00 ms; hi: 1586 / 1589 / 1589 of 20000; 20001 terms interned by the first evaluation |
| R14e native cond | 20000 | lazy | 4 | 2.37 | 3.37 |  | 48 | 0 | ok | phases 1.34 / 0.00 / 0.00 / 1.03 / 0.00 ms; hi: 1586 / 1589 / 1589 of 20000; 20001 terms interned by the first evaluation |
| R14e native cond | 20000 | always | 4 | 6.89 | 1.16 |  | 39998 | 0 | ok | phases 3.56 / 0.00 / 0.00 / 3.33 / 0.00 ms; hi: 1586 / 1589 / 1589 of 20000; 20001 terms interned by the first evaluation |
| R14e native cond | 20000 | seq hashed-memo | 1 | 31.34 | 0.25 |  | 0 | 0 | ok | phases 25.35 / 0.00 / 0.00 / 5.98 / 0.00 ms; hi: 1586 / 1589 / 1589 of 20000; 20001 terms interned by the first evaluation |
| R14e native cond | 20000 | lazy hashed-memo | 4 | 9.66 | 0.83 |  | 47 | 0 | ok | phases 8.11 / 0.00 / 0.00 / 1.55 / 0.00 ms; hi: 1586 / 1589 / 1589 of 20000; 20001 terms interned by the first evaluation |
| R14e native cond | 20000 | lazy tiny-memo | 4 | 9.63 | 0.83 |  | 47 | 0 | ok | phases 8.09 / 0.00 / 0.00 / 1.54 / 0.00 ms; hi: 1586 / 1589 / 1589 of 20000; 20001 terms interned by the first evaluation |
| R14e native cond | 20000 | lazy locked-index | 4 | 2.90 | 2.75 |  | 48 | 0 | ok | phases 1.67 / 0.00 / 0.00 / 1.23 / 0.00 ms; hi: 1586 / 1589 / 1589 of 20000; 20001 terms interned by the first evaluation |
| R14e eval cond | 20000 | ref | 1 | 74.41 |  |  | 0 | 0 | ok | phases 18.11 / 12.48 / 11.91 / 15.72 / 16.19 ms; hi: 1586 / 1589 / 3421 of 20000; 180001 terms interned by the first evaluation |
| R14e eval cond | 20000 | seq | 1 | 57.73 | 1.00 |  | 0 | 0 | ok | phases 20.27 / 0.00 / 0.00 / 19.16 / 18.29 ms; hi: 1586 / 1589 / 3421 of 20000; 180001 terms interned by the first evaluation |
| R14e eval cond | 20000 | lazy | 1 | 51.71 | 1.12 |  | 0 | 0 | ok | phases 19.68 / 0.00 / 0.00 / 16.96 / 15.07 ms; hi: 1586 / 1589 / 3421 of 20000; 180001 terms interned by the first evaluation |
| R14e eval cond | 20000 | lazy | 2 | 33.99 | 1.70 |  | 39 | 0 | ok | phases 13.65 / 0.00 / 0.00 / 11.23 / 9.11 ms; hi: 1586 / 1589 / 3421 of 20000; 180001 terms interned by the first evaluation |
| R14e eval cond | 20000 | lazy | 4 | 19.40 | 2.98 |  | 103 | 0 | ok | phases 8.21 / 0.00 / 0.00 / 6.66 / 4.53 ms; hi: 1586 / 1589 / 3421 of 20000; 180001 terms interned by the first evaluation |
| R14e eval cond | 20000 | always | 4 | 32.39 | 1.78 |  | 157997 | 0 | ok | phases 12.40 / 0.00 / 0.00 / 10.85 / 9.14 ms; hi: 1586 / 1589 / 3421 of 20000; 180001 terms interned by the first evaluation |
| R14e eval cond | 20000 | seq hashed-memo | 1 | 104.36 | 0.55 |  | 0 | 0 | ok | phases 50.96 / 0.00 / 0.00 / 28.80 / 24.60 ms; hi: 1586 / 1589 / 3421 of 20000; 180001 terms interned by the first evaluation |
| R14e eval cond | 20000 | lazy hashed-memo | 4 | 32.13 | 1.80 |  | 115 | 0 | ok | phases 16.39 / 0.00 / 0.00 / 8.95 / 6.79 ms; hi: 1586 / 1589 / 3421 of 20000; 180001 terms interned by the first evaluation |
| R14e eval cond | 20000 | lazy tiny-memo | 4 | 33.77 | 1.71 |  | 122 | 0 | ok | phases 16.94 / 0.00 / 0.00 / 9.50 / 7.33 ms; hi: 1586 / 1589 / 3421 of 20000; 180001 terms interned by the first evaluation |
| R14e eval cond | 20000 | lazy locked-index | 4 | 21.36 | 2.70 |  | 117 | 0 | ok | phases 8.98 / 0.00 / 0.00 / 7.13 / 5.26 ms; hi: 1586 / 1589 / 3421 of 20000; 180001 terms interned by the first evaluation |
| R14e eval cond untracked | 20000 | ref | 1 | 75.30 |  |  | 0 | 0 | ok | phases 19.36 / 11.54 / 11.22 / 15.27 / 17.92 ms; hi: 1586 / 1589 / 3421 of 20000; 180001 terms interned by the first evaluation |
| R14e eval cond untracked | 20000 | seq | 1 | 22.18 | 1.00 |  | 0 | 0 | stale (expected) | phases 22.18 / 0.00 / 0.00 / 0.00 / 0.00 ms; hi: 1586 / 1586 / 1586 of 20000; 180001 terms interned by the first evaluation |
| R14e eval cond untracked | 20000 | lazy | 1 | 20.03 | 1.11 |  | 0 | 0 | stale (expected) | phases 20.02 / 0.00 / 0.00 / 0.00 / 0.00 ms; hi: 1586 / 1586 / 1586 of 20000; 180001 terms interned by the first evaluation |
| R14e eval cond untracked | 20000 | lazy | 2 | 13.28 | 1.67 |  | 13 | 0 | stale (expected) | phases 13.28 / 0.00 / 0.00 / 0.00 / 0.00 ms; hi: 1586 / 1586 / 1586 of 20000; 180001 terms interned by the first evaluation |
| R14e eval cond untracked | 20000 | lazy | 4 | 8.63 | 2.57 |  | 30 | 0 | stale (expected) | phases 8.63 / 0.00 / 0.00 / 0.00 / 0.00 ms; hi: 1586 / 1586 / 1586 of 20000; 180001 terms interned by the first evaluation |
| R14e eval cond untracked | 20000 | always | 4 | 12.28 | 1.81 |  | 59999 | 0 | stale (expected) | phases 12.28 / 0.00 / 0.00 / 0.00 / 0.00 ms; hi: 1586 / 1586 / 1586 of 20000; 180001 terms interned by the first evaluation |
| R14e eval cond untracked | 20000 | seq hashed-memo | 1 | 53.05 | 0.42 |  | 0 | 0 | stale (expected) | phases 53.05 / 0.00 / 0.00 / 0.00 / 0.00 ms; hi: 1586 / 1586 / 1586 of 20000; 180001 terms interned by the first evaluation |
| R14e eval cond untracked | 20000 | lazy hashed-memo | 4 | 16.81 | 1.32 |  | 40 | 0 | stale (expected) | phases 16.81 / 0.00 / 0.00 / 0.00 / 0.00 ms; hi: 1586 / 1586 / 1586 of 20000; 180001 terms interned by the first evaluation |
| R14e eval cond untracked | 20000 | lazy tiny-memo | 4 | 17.30 | 1.28 |  | 39 | 0 | stale (expected) | phases 17.30 / 0.00 / 0.00 / 0.00 / 0.00 ms; hi: 1586 / 1586 / 1586 of 20000; 180001 terms interned by the first evaluation |
| R14e eval cond untracked | 20000 | lazy locked-index | 4 | 8.72 | 2.54 |  | 30 | 0 | stale (expected) | phases 8.72 / 0.00 / 0.00 / 0.00 / 0.00 ms; hi: 1586 / 1586 / 1586 of 20000; 180001 terms interned by the first evaluation |
| R14e eval cond untracked | 20000 | | | | | | | | | memoised configurations must be stale: condition reads untracked |
| MR map + redefinitions | 20000 | ref | 1 | 20.36 |  |  | 0 | 0 | ok | 1 outputs, F has 1008 rules, epoch 1066 |
| MR map + redefinitions | 20000 | seq | 1 | 13.14 | 1.00 |  | 0 | 0 | ok | 1 outputs, F has 1008 rules, epoch 1066 |
| MR map + redefinitions | 20000 | lazy | 1 | 12.40 | 1.06 |  | 0 | 0 | ok | 1 outputs, F has 1008 rules, epoch 1066 |
| MR map + redefinitions | 20000 | lazy | 2 | 12.48 | 1.05 |  | 25 | 14979 | ok | 1 outputs, F has 1008 rules, epoch 1066 |
| MR map + redefinitions | 20000 | lazy | 4 | 9.63 | 1.36 |  | 52 | 15000 | ok | 1 outputs, F has 1008 rules, epoch 1066 |
| MR map + redefinitions | 20000 | always | 4 | 13.80 | 0.95 |  | 52250 | 18000 | ok | 1 outputs, F has 1008 rules, epoch 1066 |
| MR map + redefinitions | 20000 | seq hashed-memo | 1 | 20.29 | 0.65 |  | 0 | 0 | ok | 1 outputs, F has 1008 rules, epoch 1066 |
| MR map + redefinitions | 20000 | lazy hashed-memo | 4 | 13.87 | 0.95 |  | 56 | 15000 | ok | 1 outputs, F has 1008 rules, epoch 1066 |
| MR map + redefinitions | 20000 | lazy tiny-memo | 4 | 20.86 | 0.63 |  | 86 | 15000 | ok | 1 outputs, F has 1008 rules, epoch 1066 |
| MR map + redefinitions | 20000 | lazy locked-index | 4 | 10.93 | 1.20 |  | 53 | 15000 | ok | 1 outputs, F has 1008 rules, epoch 1066 |
| MR fuzz | 500 | ref | 1 | 12.83 |  |  | 0 | 0 | ok | 20 random layouts |
| MR fuzz | 500 | seq | 1 | 10.88 | 1.00 |  | 0 | 0 | ok | 20 random layouts |
| MR fuzz | 500 | lazy | 1 | 11.65 | 0.93 |  | 0 | 0 | ok | 20 random layouts |
| MR fuzz | 500 | lazy | 2 | 9.69 | 1.12 |  | 79 | 5953 | ok | 20 random layouts |
| MR fuzz | 500 | lazy | 4 | 9.82 | 1.11 |  | 88 | 6063 | ok | 20 random layouts |
| MR fuzz | 500 | always | 4 | 10.77 | 1.01 |  | 30877 | 7768 | ok | 20 random layouts |
| MR fuzz | 500 | seq hashed-memo | 1 | 40.55 | 0.27 |  | 0 | 0 | ok | 20 random layouts |
| MR fuzz | 500 | lazy hashed-memo | 4 | 25.93 | 0.42 |  | 87 | 6063 | ok | 20 random layouts |
| MR fuzz | 500 | lazy tiny-memo | 4 | 10.68 | 1.02 |  | 93 | 6173 | ok | 20 random layouts |
| MR fuzz | 500 | lazy locked-index | 4 | 9.34 | 1.17 |  | 87 | 6061 | ok | 20 random layouts |
[1/96 1%] R4 AC rules n=1000 spike 2
[2/96 2%] R4 AC rules n=1000 ref t=1
[3/96 3%] R4 AC rules n=1000 seq t=1
[4/96 4%] R4 AC rules n=1000 lazy t=1
[5/96 5%] R4 AC rules n=1000 lazy t=2
[6/96 6%] R4 AC rules n=1000 lazy t=4
[7/96 7%] R4 AC rules n=1000 always t=4
[8/96 8%] R4 AC rules n=1000 seq hashed-memo t=1
[9/96 9%] R4 AC rules n=1000 lazy hashed-memo t=4
[10/96 10%] R4 AC rules n=1000 lazy tiny-memo t=4
[11/96 11%] R4 AC rules n=1000 lazy locked-index t=4
[12/96 12%] R5 literal rules n=5000 spike 2
[13/96 14%] R5 literal rules n=5000 ref t=1
[14/96 15%] R5 literal rules n=5000 seq t=1
[15/96 16%] R5 literal rules n=5000 lazy t=1
[16/96 17%] R5 literal rules n=5000 lazy t=2
[17/96 18%] R5 literal rules n=5000 lazy t=4
[18/96 19%] R5 literal rules n=5000 always t=4
[19/96 20%] R5 literal rules n=5000 lazy locked-index t=4
[20/96 21%] R13 arg-head rules n=100 spike 2
[21/96 22%] R13 arg-head rules n=100 ref t=1
[22/96 23%] R13 arg-head rules n=100 seq t=1
[23/96 24%] R13 arg-head rules n=100 lazy t=1
[24/96 25%] R13 arg-head rules n=100 lazy t=2
[25/96 26%] R13 arg-head rules n=100 lazy t=4
[26/96 27%] R13 arg-head rules n=100 always t=4
[27/96 28%] R13 arg-head rules n=100 lazy locked-index t=4
[28/96 29%] R14 read sets n=100000 spike 2
[29/96 30%] R14 read sets n=100000 ref t=1
[30/96 31%] R14 read sets n=100000 seq t=1
[31/96 32%] R14 read sets n=100000 lazy t=1
[32/96 33%] R14 read sets n=100000 lazy t=2
[33/96 34%] R14 read sets n=100000 lazy t=4
[34/96 35%] R14 read sets n=100000 always t=4
[35/96 36%] R14 read sets n=100000 seq hashed-memo t=1
[36/96 38%] R14 read sets n=100000 lazy hashed-memo t=4
[37/96 39%] R14 read sets n=100000 lazy tiny-memo t=4
[38/96 40%] R14 read sets n=100000 lazy locked-index t=4
[39/96 41%] S7 cases n=100000 spike 2
[40/96 42%] S7 cases n=100000 ref t=1
[41/96 43%] S7 cases n=100000 seq t=1
[42/96 44%] S7 cases n=100000 lazy t=1
[43/96 45%] S7 cases n=100000 lazy t=2
[44/96 46%] S7 cases n=100000 lazy t=4
[45/96 47%] S7 cases n=100000 always t=4
[46/96 48%] S7 cases n=100000 lazy locked-index t=4
[47/96 49%] R14e native cond n=20000 ref t=1
[48/96 50%] R14e native cond n=20000 seq t=1
[49/96 51%] R14e native cond n=20000 lazy t=1
[50/96 52%] R14e native cond n=20000 lazy t=2
[51/96 53%] R14e native cond n=20000 lazy t=4
[52/96 54%] R14e native cond n=20000 always t=4
[53/96 55%] R14e native cond n=20000 seq hashed-memo t=1
[54/96 56%] R14e native cond n=20000 lazy hashed-memo t=4
[55/96 57%] R14e native cond n=20000 lazy tiny-memo t=4
[56/96 58%] R14e native cond n=20000 lazy locked-index t=4
[57/96 59%] R14e eval cond n=20000 ref t=1
[58/96 60%] R14e eval cond n=20000 seq t=1
[59/96 61%] R14e eval cond n=20000 lazy t=1
[60/96 62%] R14e eval cond n=20000 lazy t=2
[61/96 64%] R14e eval cond n=20000 lazy t=4
[62/96 65%] R14e eval cond n=20000 always t=4
[63/96 66%] R14e eval cond n=20000 seq hashed-memo t=1
[64/96 67%] R14e eval cond n=20000 lazy hashed-memo t=4
[65/96 68%] R14e eval cond n=20000 lazy tiny-memo t=4
[66/96 69%] R14e eval cond n=20000 lazy locked-index t=4
[67/96 70%] R14e eval cond untracked n=20000 ref t=1
[68/96 71%] R14e eval cond untracked n=20000 seq t=1
[69/96 72%] R14e eval cond untracked n=20000 lazy t=1
[70/96 73%] R14e eval cond untracked n=20000 lazy t=2
[71/96 74%] R14e eval cond untracked n=20000 lazy t=4
[72/96 75%] R14e eval cond untracked n=20000 always t=4
[73/96 76%] R14e eval cond untracked n=20000 seq hashed-memo t=1
[74/96 77%] R14e eval cond untracked n=20000 lazy hashed-memo t=4
[75/96 78%] R14e eval cond untracked n=20000 lazy tiny-memo t=4
[76/96 79%] R14e eval cond untracked n=20000 lazy locked-index t=4
[77/96 80%] MR map + redefinitions n=20000 ref t=1
[78/96 81%] MR map + redefinitions n=20000 seq t=1
[79/96 82%] MR map + redefinitions n=20000 lazy t=1
[80/96 83%] MR map + redefinitions n=20000 lazy t=2
[81/96 84%] MR map + redefinitions n=20000 lazy t=4
[82/96 85%] MR map + redefinitions n=20000 always t=4
[83/96 86%] MR map + redefinitions n=20000 seq hashed-memo t=1
[84/96 88%] MR map + redefinitions n=20000 lazy hashed-memo t=4
[85/96 89%] MR map + redefinitions n=20000 lazy tiny-memo t=4
[86/96 90%] MR map + redefinitions n=20000 lazy locked-index t=4
[87/96 91%] MR fuzz n=500 ref t=1
[88/96 92%] MR fuzz n=500 seq t=1
[89/96 93%] MR fuzz n=500 lazy t=1
[90/96 94%] MR fuzz n=500 lazy t=2
[91/96 95%] MR fuzz n=500 lazy t=4
[92/96 96%] MR fuzz n=500 always t=4
[93/96 97%] MR fuzz n=500 seq hashed-memo t=1
[94/96 98%] MR fuzz n=500 lazy hashed-memo t=4
[95/96 99%] MR fuzz n=500 lazy tiny-memo t=4
[96/96 100%] MR fuzz n=500 lazy locked-index t=4
