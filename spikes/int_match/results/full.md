| case | n | config | threads | ms | speedup vs seq | vs spike 2 | splits | redos | check | note |
|---|---|---|---|---|---|---|---|---|---|---|
| R4 AC rules | 10000 | spike 2 indexed/smart | 1 | 120.71 | | 1.00 | | | | spike 2's sequential evaluator |
| R4 AC rules | 10000 | ref | 1 | 126.43 |  | 1.05 (s2 check ok) | 0 | 0 | ok | 9609 of 10000 subjects rewritten |
| R4 AC rules | 10000 | seq | 1 | 126.67 | 1.00 | 1.05 (s2 check ok) | 0 | 0 | ok | 9609 of 10000 subjects rewritten |
| R4 AC rules | 10000 | lazy | 1 | 128.86 | 0.98 |  | 0 | 0 | ok | 9609 of 10000 subjects rewritten |
| R4 AC rules | 10000 | lazy | 2 | 66.01 | 1.92 |  | 15 | 0 | ok | 9609 of 10000 subjects rewritten |
| R4 AC rules | 10000 | lazy | 4 | 33.52 | 3.78 |  | 30 | 0 | ok | 9609 of 10000 subjects rewritten |
| R4 AC rules | 10000 | always | 4 | 35.85 | 3.53 |  | 9999 | 0 | ok | 9609 of 10000 subjects rewritten |
| R4 AC rules | 10000 | seq hashed-memo | 1 | 133.46 | 0.95 |  | 0 | 0 | ok | 9609 of 10000 subjects rewritten |
| R4 AC rules | 10000 | lazy hashed-memo | 4 | 34.43 | 3.68 |  | 30 | 0 | ok | 9609 of 10000 subjects rewritten |
| R4 AC rules | 10000 | lazy tiny-memo | 4 | 32.53 | 3.89 |  | 30 | 0 | ok | 9609 of 10000 subjects rewritten |
| R4 AC rules | 10000 | lazy locked-index | 4 | 32.55 | 3.89 |  | 30 | 0 | ok | 9609 of 10000 subjects rewritten |
| R5 literal rules | 50000 | spike 2 indexed | 1 | 186.89 | | 1.00 | | | | spike 2's sequential evaluator |
| R5 literal rules | 50000 | ref | 1 | 188.41 |  | 1.01 (s2 check ok) | 0 | 0 | ok | 499681 of 1000000 calls hit a literal rule |
| R5 literal rules | 50000 | seq | 1 | 199.32 | 1.00 | 1.07 (s2 check ok) | 0 | 0 | ok | 499681 of 1000000 calls hit a literal rule |
| R5 literal rules | 50000 | lazy | 1 | 200.31 | 1.00 |  | 0 | 0 | ok | 499681 of 1000000 calls hit a literal rule |
| R5 literal rules | 50000 | lazy | 2 | 101.20 | 1.97 |  | 23 | 0 | ok | 499681 of 1000000 calls hit a literal rule |
| R5 literal rules | 50000 | lazy | 4 | 52.34 | 3.81 |  | 112 | 0 | ok | 499681 of 1000000 calls hit a literal rule |
| R5 literal rules | 50000 | always | 4 | 179.14 | 1.11 |  | 999999 | 0 | ok | 499681 of 1000000 calls hit a literal rule |
| R5 literal rules | 50000 | lazy locked-index | 4 | 66.66 | 2.99 |  | 80 | 0 | ok | 499681 of 1000000 calls hit a literal rule |
| R13 arg-head rules | 1000 | spike 2 arg-head | 1 | 222.10 | | 1.00 | | | | spike 2's sequential evaluator |
| R13 arg-head rules | 1000 | ref | 1 | 225.27 |  | 1.01 (s2 check ok) | 0 | 0 | ok | 99866 calls fell through to the generic rule |
| R13 arg-head rules | 1000 | seq | 1 | 227.57 | 1.00 | 1.02 (s2 check ok) | 0 | 0 | ok | 99866 calls fell through to the generic rule |
| R13 arg-head rules | 1000 | lazy | 1 | 230.97 | 0.99 |  | 0 | 0 | ok | 99866 calls fell through to the generic rule |
| R13 arg-head rules | 1000 | lazy | 2 | 111.49 | 2.04 |  | 35 | 0 | ok | 99866 calls fell through to the generic rule |
| R13 arg-head rules | 1000 | lazy | 4 | 78.69 | 2.89 |  | 99 | 0 | ok | 99866 calls fell through to the generic rule |
| R13 arg-head rules | 1000 | always | 4 | 153.60 | 1.48 |  | 999999 | 0 | ok | 99866 calls fell through to the generic rule |
| R13 arg-head rules | 1000 | lazy locked-index | 4 | 61.19 | 3.72 |  | 85 | 0 | ok | 99866 calls fell through to the generic rule |
| R14 read sets | 1000000 | spike 2 memo/mask | 1 | 311.70 | | 1.00 | | | | spike 2's sequential evaluator |
| R14 read sets | 1000000 | ref | 1 | 763.45 |  | 2.45 (s2 check ok) | 0 | 0 | ok | phases 204.16 / 183.67 / 191.67 / 183.96 ms; result changed |
| R14 read sets | 1000000 | seq | 1 | 256.56 | 1.00 | 0.82 (s2 check ok) | 0 | 0 | ok | phases 224.67 / 0.00 / 12.11 / 19.68 ms; result changed |
| R14 read sets | 1000000 | lazy | 1 | 257.94 | 0.99 |  | 0 | 0 | ok | phases 226.23 / 0.00 / 12.22 / 19.49 ms; result changed |
| R14 read sets | 1000000 | lazy | 2 | 155.67 | 1.65 |  | 72 | 0 | ok | phases 132.28 / 0.00 / 10.70 / 13.99 ms; result changed |
| R14 read sets | 1000000 | lazy | 4 | 92.05 | 2.79 |  | 230 | 0 | ok | phases 72.54 / 0.00 / 8.60 / 10.68 ms; result changed |
| R14 read sets | 1000000 | always | 4 | 220.18 | 1.17 |  | 2999997 | 0 | ok | phases 182.56 / 0.00 / 17.14 / 20.05 ms; result changed |
| R14 read sets | 1000000 | seq hashed-memo | 1 | 479.93 | 0.53 |  | 0 | 0 | ok | phases 387.07 / 0.00 / 49.90 / 62.81 ms; result changed |
| R14 read sets | 1000000 | lazy hashed-memo | 4 | 151.05 | 1.70 |  | 201 | 0 | ok | phases 110.60 / 0.00 / 18.53 / 21.65 ms; result changed |
| R14 read sets | 1000000 | lazy tiny-memo | 4 | 151.42 | 1.69 |  | 267 | 0 | ok | phases 110.24 / 0.00 / 18.51 / 21.76 ms; result changed |
| R14 read sets | 1000000 | lazy locked-index | 4 | 100.89 | 2.54 |  | 241 | 0 | ok | phases 80.96 / 0.00 / 9.14 / 10.78 ms; result changed |
| S7 cases | 1000000 | spike 2 smart | 1 | 81.31 | | 1.00 | | | | spike 2's sequential evaluator |
| S7 cases | 1000000 | ref | 1 | 101.43 |  | 1.25 (s2 check ok) | 0 | 0 | ok | selected [147176, 250028, 56090, 26255, 140523] |
| S7 cases | 1000000 | seq | 1 | 100.90 | 1.00 | 1.24 (s2 check ok) | 0 | 0 | ok | selected [147176, 250028, 56090, 26255, 140523] |
| S7 cases | 1000000 | lazy | 1 | 101.42 | 0.99 |  | 0 | 0 | ok | selected [147176, 250028, 56090, 26255, 140523] |
| S7 cases | 1000000 | lazy | 2 | 54.26 | 1.86 |  | 68 | 0 | ok | selected [147176, 250028, 56090, 26255, 140523] |
| S7 cases | 1000000 | lazy | 4 | 29.63 | 3.41 |  | 120 | 0 | ok | selected [147176, 250028, 56090, 26255, 140523] |
| S7 cases | 1000000 | always | 4 | 207.32 | 0.49 |  | 4999999 | 0 | ok | selected [147176, 250028, 56090, 26255, 140523] |
| S7 cases | 1000000 | lazy locked-index | 4 | 29.72 | 3.40 |  | 130 | 0 | ok | selected [147176, 250028, 56090, 26255, 140523] |
| R14e native cond | 200000 | ref | 1 | 192.01 |  |  | 0 | 0 | ok | phases 44.87 / 41.79 / 38.17 / 37.32 / 37.43 ms; hi: 15726 / 15737 / 15737 of 200000; 200001 terms interned by the first evaluation |
| R14e native cond | 200000 | seq | 1 | 98.04 | 1.00 |  | 0 | 0 | ok | phases 44.95 / 0.00 / 0.00 / 41.77 / 0.00 ms; hi: 15726 / 15737 / 15737 of 200000; 200001 terms interned by the first evaluation |
| R14e native cond | 200000 | lazy | 1 | 88.95 | 1.10 |  | 0 | 0 | ok | phases 50.04 / 0.00 / 0.00 / 53.67 / 0.00 ms; hi: 15726 / 15737 / 15737 of 200000; 200001 terms interned by the first evaluation |
| R14e native cond | 200000 | lazy | 2 | 63.80 | 1.54 |  | 40 | 0 | ok | phases 35.82 / 0.00 / 0.00 / 27.98 / 0.00 ms; hi: 15726 / 15737 / 15737 of 200000; 200001 terms interned by the first evaluation |
| R14e native cond | 200000 | lazy | 4 | 28.40 | 3.45 |  | 91 | 0 | ok | phases 14.05 / 0.00 / 0.00 / 11.71 / 0.00 ms; hi: 15726 / 15737 / 15737 of 200000; 200001 terms interned by the first evaluation |
| R14e native cond | 200000 | always | 4 | 71.86 | 1.36 |  | 399998 | 0 | ok | phases 36.42 / 0.00 / 0.00 / 37.13 / 0.00 ms; hi: 15726 / 15737 / 15737 of 200000; 200001 terms interned by the first evaluation |
| R14e native cond | 200000 | seq hashed-memo | 1 | 190.71 | 0.51 |  | 0 | 0 | ok | phases 96.90 / 0.00 / 0.00 / 73.84 / 0.00 ms; hi: 15726 / 15737 / 15737 of 200000; 200001 terms interned by the first evaluation |
| R14e native cond | 200000 | lazy hashed-memo | 4 | 47.55 | 2.06 |  | 111 | 0 | ok | phases 28.45 / 0.00 / 0.00 / 19.10 / 0.00 ms; hi: 15726 / 15737 / 15737 of 200000; 200001 terms interned by the first evaluation |
| R14e native cond | 200000 | lazy tiny-memo | 4 | 47.17 | 2.08 |  | 102 | 0 | ok | phases 27.83 / 0.00 / 0.00 / 19.34 / 0.00 ms; hi: 15726 / 15737 / 15737 of 200000; 200001 terms interned by the first evaluation |
| R14e native cond | 200000 | lazy locked-index | 4 | 28.69 | 3.42 |  | 88 | 0 | ok | phases 15.60 / 0.00 / 0.00 / 12.90 / 0.00 ms; hi: 15726 / 15737 / 15737 of 200000; 200001 terms interned by the first evaluation |
| R14e eval cond | 200000 | ref | 1 | 1475.05 |  |  | 0 | 0 | ok | phases 311.94 / 292.00 / 283.35 / 290.47 / 297.29 ms; hi: 15726 / 15737 / 34199 of 200000; 1928819 terms interned by the first evaluation |
| R14e eval cond | 200000 | seq | 1 | 883.85 | 1.00 |  | 0 | 0 | ok | phases 279.94 / 0.00 / 0.00 / 277.92 / 238.96 ms; hi: 15726 / 15737 / 34199 of 200000; 1928819 terms interned by the first evaluation |
| R14e eval cond | 200000 | lazy | 1 | 781.49 | 1.13 |  | 0 | 0 | ok | phases 264.29 / 0.00 / 0.00 / 286.03 / 238.42 ms; hi: 15726 / 15737 / 34199 of 200000; 1928819 terms interned by the first evaluation |
| R14e eval cond | 200000 | lazy | 2 | 463.78 | 1.91 |  | 76 | 0 | ok | phases 176.22 / 0.00 / 0.00 / 161.50 / 126.05 ms; hi: 15726 / 15737 / 34199 of 200000; 1928819 terms interned by the first evaluation |
| R14e eval cond | 200000 | lazy | 4 | 256.94 | 3.44 |  | 174 | 0 | ok | phases 95.14 / 0.00 / 0.00 / 86.26 / 67.32 ms; hi: 15726 / 15737 / 34199 of 200000; 1928819 terms interned by the first evaluation |
| R14e eval cond | 200000 | always | 4 | 378.08 | 2.34 |  | 1579997 | 0 | ok | phases 136.43 / 0.00 / 0.00 / 130.10 / 111.55 ms; hi: 15726 / 15737 / 34199 of 200000; 1928819 terms interned by the first evaluation |
| R14e eval cond | 200000 | seq hashed-memo | 1 | 1122.87 | 0.79 |  | 0 | 0 | ok | phases 407.40 / 0.00 / 0.00 / 397.22 / 326.07 ms; hi: 15726 / 15737 / 34199 of 200000; 1928819 terms interned by the first evaluation |
| R14e eval cond | 200000 | lazy hashed-memo | 4 | 329.48 | 2.68 |  | 195 | 0 | ok | phases 133.88 / 0.00 / 0.00 / 119.11 / 95.58 ms; hi: 15726 / 15737 / 34199 of 200000; 1928819 terms interned by the first evaluation |
| R14e eval cond | 200000 | lazy tiny-memo | 4 | 360.28 | 2.45 |  | 293 | 0 | ok | phases 147.76 / 0.00 / 0.00 / 119.38 / 93.14 ms; hi: 15726 / 15737 / 34199 of 200000; 1928819 terms interned by the first evaluation |
| R14e eval cond | 200000 | lazy locked-index | 4 | 305.70 | 2.89 |  | 212 | 0 | ok | phases 106.54 / 0.00 / 0.00 / 96.07 / 79.83 ms; hi: 15726 / 15737 / 34199 of 200000; 1928819 terms interned by the first evaluation |
| R14e eval cond untracked | 20000 | ref | 1 | 64.65 |  |  | 0 | 0 | ok | phases 16.30 / 10.98 / 10.94 / 13.42 / 13.01 ms; hi: 1586 / 1589 / 3421 of 20000; 180001 terms interned by the first evaluation |
| R14e eval cond untracked | 20000 | seq | 1 | 16.51 | 1.00 |  | 0 | 0 | stale (expected) | phases 16.80 / 0.00 / 0.00 / 0.00 / 0.00 ms; hi: 1586 / 1586 / 1586 of 20000; 180001 terms interned by the first evaluation |
| R14e eval cond untracked | 20000 | lazy | 1 | 16.85 | 0.98 |  | 0 | 0 | stale (expected) | phases 17.35 / 0.00 / 0.00 / 0.00 / 0.00 ms; hi: 1586 / 1586 / 1586 of 20000; 180001 terms interned by the first evaluation |
| R14e eval cond untracked | 20000 | lazy | 2 | 10.88 | 1.52 |  | 13 | 0 | stale (expected) | phases 10.75 / 0.00 / 0.00 / 0.00 / 0.00 ms; hi: 1586 / 1586 / 1586 of 20000; 180001 terms interned by the first evaluation |
| R14e eval cond untracked | 20000 | lazy | 4 | 6.68 | 2.47 |  | 30 | 0 | stale (expected) | phases 6.68 / 0.00 / 0.00 / 0.00 / 0.00 ms; hi: 1586 / 1586 / 1586 of 20000; 180001 terms interned by the first evaluation |
| R14e eval cond untracked | 20000 | always | 4 | 10.48 | 1.58 |  | 59999 | 0 | stale (expected) | phases 10.41 / 0.00 / 0.00 / 0.00 / 0.00 ms; hi: 1586 / 1586 / 1586 of 20000; 180001 terms interned by the first evaluation |
| R14e eval cond untracked | 20000 | seq hashed-memo | 1 | 57.32 | 0.29 |  | 0 | 0 | stale (expected) | phases 48.43 / 0.00 / 0.00 / 0.00 / 0.00 ms; hi: 1586 / 1586 / 1586 of 20000; 180001 terms interned by the first evaluation |
| R14e eval cond untracked | 20000 | lazy hashed-memo | 4 | 15.03 | 1.10 |  | 31 | 0 | stale (expected) | phases 15.03 / 0.00 / 0.00 / 0.00 / 0.00 ms; hi: 1586 / 1586 / 1586 of 20000; 180001 terms interned by the first evaluation |
| R14e eval cond untracked | 20000 | lazy tiny-memo | 4 | 15.14 | 1.09 |  | 30 | 0 | stale (expected) | phases 15.20 / 0.00 / 0.00 / 0.00 / 0.00 ms; hi: 1586 / 1586 / 1586 of 20000; 180001 terms interned by the first evaluation |
| R14e eval cond untracked | 20000 | lazy locked-index | 4 | 6.81 | 2.42 |  | 38 | 0 | stale (expected) | phases 6.66 / 0.00 / 0.00 / 0.00 / 0.00 ms; hi: 1586 / 1586 / 1586 of 20000; 180001 terms interned by the first evaluation |
| R14e eval cond untracked | 20000 | | | | | | | | | memoised configurations must be stale: condition reads untracked |
| MR map + redefinitions | 200000 | ref | 1 | 150.71 |  |  | 0 | 0 | ok | 1 outputs, F has 1008 rules, epoch 1066 |
| MR map + redefinitions | 200000 | seq | 1 | 34.86 | 1.00 |  | 0 | 0 | ok | 1 outputs, F has 1008 rules, epoch 1066 |
| MR map + redefinitions | 200000 | lazy | 1 | 34.01 | 1.02 |  | 0 | 0 | ok | 1 outputs, F has 1008 rules, epoch 1066 |
| MR map + redefinitions | 200000 | lazy | 2 | 39.85 | 0.87 |  | 49 | 149979 | ok | 1 outputs, F has 1008 rules, epoch 1066 |
| MR map + redefinitions | 200000 | lazy | 4 | 28.89 | 1.21 |  | 103 | 150000 | ok | 1 outputs, F has 1008 rules, epoch 1066 |
| MR map + redefinitions | 200000 | always | 4 | 41.07 | 0.85 |  | 428764 | 180000 | ok | 1 outputs, F has 1008 rules, epoch 1066 |
| MR map + redefinitions | 200000 | seq hashed-memo | 1 | 51.04 | 0.68 |  | 0 | 0 | ok | 1 outputs, F has 1008 rules, epoch 1066 |
| MR map + redefinitions | 200000 | lazy hashed-memo | 4 | 38.16 | 0.91 |  | 128 | 150000 | ok | 1 outputs, F has 1008 rules, epoch 1066 |
| MR map + redefinitions | 200000 | lazy tiny-memo | 4 | 153.70 | 0.23 |  | 177 | 150000 | ok | 1 outputs, F has 1008 rules, epoch 1066 |
| MR map + redefinitions | 200000 | lazy locked-index | 4 | 31.27 | 1.12 |  | 113 | 150000 | ok | 1 outputs, F has 1008 rules, epoch 1066 |
| MR fuzz | 3000 | ref | 1 | 43.07 |  |  | 0 | 0 | ok | 20 random layouts |
| MR fuzz | 3000 | seq | 1 | 41.65 | 1.00 |  | 0 | 0 | ok | 20 random layouts |
| MR fuzz | 3000 | lazy | 1 | 41.38 | 1.01 |  | 0 | 0 | ok | 20 random layouts |
| MR fuzz | 3000 | lazy | 2 | 39.47 | 1.06 |  | 234 | 40718 | ok | 20 random layouts |
| MR fuzz | 3000 | lazy | 4 | 36.07 | 1.15 |  | 342 | 43738 | ok | 20 random layouts |
| MR fuzz | 3000 | always | 4 | 44.55 | 0.93 |  | 174797 | 47580 | ok | 20 random layouts |
| MR fuzz | 3000 | seq hashed-memo | 1 | 124.67 | 0.33 |  | 0 | 0 | ok | 20 random layouts |
| MR fuzz | 3000 | lazy hashed-memo | 4 | 74.19 | 0.56 |  | 360 | 42289 | ok | 20 random layouts |
| MR fuzz | 3000 | lazy tiny-memo | 4 | 49.45 | 0.84 |  | 416 | 43489 | ok | 20 random layouts |
| MR fuzz | 3000 | lazy locked-index | 4 | 36.78 | 1.13 |  | 363 | 42977 | ok | 20 random layouts |
[1/96 1%] R4 AC rules n=10000 spike 2
[2/96 2%] R4 AC rules n=10000 ref t=1
[3/96 3%] R4 AC rules n=10000 seq t=1
[4/96 4%] R4 AC rules n=10000 lazy t=1
[5/96 5%] R4 AC rules n=10000 lazy t=2
[6/96 6%] R4 AC rules n=10000 lazy t=4
[7/96 7%] R4 AC rules n=10000 always t=4
[8/96 8%] R4 AC rules n=10000 seq hashed-memo t=1
[9/96 9%] R4 AC rules n=10000 lazy hashed-memo t=4
[10/96 10%] R4 AC rules n=10000 lazy tiny-memo t=4
[11/96 11%] R4 AC rules n=10000 lazy locked-index t=4
[12/96 12%] R5 literal rules n=50000 spike 2
[13/96 14%] R5 literal rules n=50000 ref t=1
[14/96 15%] R5 literal rules n=50000 seq t=1
[15/96 16%] R5 literal rules n=50000 lazy t=1
[16/96 17%] R5 literal rules n=50000 lazy t=2
[17/96 18%] R5 literal rules n=50000 lazy t=4
[18/96 19%] R5 literal rules n=50000 always t=4
[19/96 20%] R5 literal rules n=50000 lazy locked-index t=4
[20/96 21%] R13 arg-head rules n=1000 spike 2
[21/96 22%] R13 arg-head rules n=1000 ref t=1
[22/96 23%] R13 arg-head rules n=1000 seq t=1
[23/96 24%] R13 arg-head rules n=1000 lazy t=1
[24/96 25%] R13 arg-head rules n=1000 lazy t=2
[25/96 26%] R13 arg-head rules n=1000 lazy t=4
[26/96 27%] R13 arg-head rules n=1000 always t=4
[27/96 28%] R13 arg-head rules n=1000 lazy locked-index t=4
[28/96 29%] R14 read sets n=1000000 spike 2
[29/96 30%] R14 read sets n=1000000 ref t=1
[30/96 31%] R14 read sets n=1000000 seq t=1
[31/96 32%] R14 read sets n=1000000 lazy t=1
[32/96 33%] R14 read sets n=1000000 lazy t=2
[33/96 34%] R14 read sets n=1000000 lazy t=4
[34/96 35%] R14 read sets n=1000000 always t=4
[35/96 36%] R14 read sets n=1000000 seq hashed-memo t=1
[36/96 38%] R14 read sets n=1000000 lazy hashed-memo t=4
[37/96 39%] R14 read sets n=1000000 lazy tiny-memo t=4
[38/96 40%] R14 read sets n=1000000 lazy locked-index t=4
[39/96 41%] S7 cases n=1000000 spike 2
[40/96 42%] S7 cases n=1000000 ref t=1
[41/96 43%] S7 cases n=1000000 seq t=1
[42/96 44%] S7 cases n=1000000 lazy t=1
[43/96 45%] S7 cases n=1000000 lazy t=2
[44/96 46%] S7 cases n=1000000 lazy t=4
[45/96 47%] S7 cases n=1000000 always t=4
[46/96 48%] S7 cases n=1000000 lazy locked-index t=4
[47/96 49%] R14e native cond n=200000 ref t=1
[48/96 50%] R14e native cond n=200000 seq t=1
[49/96 51%] R14e native cond n=200000 lazy t=1
[50/96 52%] R14e native cond n=200000 lazy t=2
[51/96 53%] R14e native cond n=200000 lazy t=4
[52/96 54%] R14e native cond n=200000 always t=4
[53/96 55%] R14e native cond n=200000 seq hashed-memo t=1
[54/96 56%] R14e native cond n=200000 lazy hashed-memo t=4
[55/96 57%] R14e native cond n=200000 lazy tiny-memo t=4
[56/96 58%] R14e native cond n=200000 lazy locked-index t=4
[57/96 59%] R14e eval cond n=200000 ref t=1
[58/96 60%] R14e eval cond n=200000 seq t=1
[59/96 61%] R14e eval cond n=200000 lazy t=1
[60/96 62%] R14e eval cond n=200000 lazy t=2
[61/96 64%] R14e eval cond n=200000 lazy t=4
[62/96 65%] R14e eval cond n=200000 always t=4
[63/96 66%] R14e eval cond n=200000 seq hashed-memo t=1
[64/96 67%] R14e eval cond n=200000 lazy hashed-memo t=4
[65/96 68%] R14e eval cond n=200000 lazy tiny-memo t=4
[66/96 69%] R14e eval cond n=200000 lazy locked-index t=4
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
[77/96 80%] MR map + redefinitions n=200000 ref t=1
[78/96 81%] MR map + redefinitions n=200000 seq t=1
[79/96 82%] MR map + redefinitions n=200000 lazy t=1
[80/96 83%] MR map + redefinitions n=200000 lazy t=2
[81/96 84%] MR map + redefinitions n=200000 lazy t=4
[82/96 85%] MR map + redefinitions n=200000 always t=4
[83/96 86%] MR map + redefinitions n=200000 seq hashed-memo t=1
[84/96 88%] MR map + redefinitions n=200000 lazy hashed-memo t=4
[85/96 89%] MR map + redefinitions n=200000 lazy tiny-memo t=4
[86/96 90%] MR map + redefinitions n=200000 lazy locked-index t=4
[87/96 91%] MR fuzz n=3000 ref t=1
[88/96 92%] MR fuzz n=3000 seq t=1
[89/96 93%] MR fuzz n=3000 lazy t=1
[90/96 94%] MR fuzz n=3000 lazy t=2
[91/96 95%] MR fuzz n=3000 lazy t=4
[92/96 96%] MR fuzz n=3000 always t=4
[93/96 97%] MR fuzz n=3000 seq hashed-memo t=1
[94/96 98%] MR fuzz n=3000 lazy hashed-memo t=4
[95/96 99%] MR fuzz n=3000 lazy tiny-memo t=4
[96/96 100%] MR fuzz n=3000 lazy locked-index t=4
