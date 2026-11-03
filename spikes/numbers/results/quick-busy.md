# Numbers spike: quick run

Machine: AMD64 Family 25 Model 97 Stepping 2, AuthenticAMD (16 hardware threads), windows; v1 comparison on.

## Multiplication of two n-digit integers

Times in ms (median); peak heap in MB above the inputs; check = fingerprint equal to own/auto.

| n digits | implementation | median | min | runs | peak heap | check |
|---|---|---|---|---|---|---|
| 1000 | own auto | 0.002 | 0.002 | 5 | 0.0 | ok |
| 1000 | own schoolbook | 0.002 | 0.002 | 5 | 0.0 | ok |
| 1000 | own Karatsuba (no NTT) | 0.002 | 0.002 | 5 | 0.0 | ok |
| 1000 | own NTT (top level) | 0.009 | 0.009 | 5 | 0.0 | ok |
| 1000 | num-bigint 0.4 | 0.002 | 0.002 | 5 | 0.0 | ok |
| 1000 | v1 kernel | 0.008 | 0.008 | 5 | 0.0 | ok |
| 10000 | own auto | 0.068 | 0.068 | 5 | 0.1 | ok |
| 10000 | own schoolbook | 0.188 | 0.188 | 5 | 0.0 | ok |
| 10000 | own Karatsuba (no NTT) | 0.069 | 0.068 | 5 | 0.1 | ok |
| 10000 | own NTT (top level) | 0.162 | 0.160 | 5 | 0.3 | ok |
| 10000 | num-bigint 0.4 | 0.088 | 0.071 | 5 | 0.0 | ok |
| 10000 | v1 kernel | 0.257 | 0.254 | 5 | 0.1 | ok |
| 100000 | own auto | 1.55 | 1.53 | 5 | 2.0 | ok |
| 100000 | own schoolbook | 18.8 | 18.7 | 5 | 0.2 | ok |
| 100000 | own Karatsuba (no NTT) | 2.80 | 2.76 | 5 | 0.5 | ok |
| 100000 | own NTT (top level) | 1.63 | 1.50 | 5 | 0.6 | ok |
| 100000 | num-bigint 0.4 | 2.20 | 2.19 | 5 | 0.5 | ok |
| 100000 | v1 kernel | 9.80 | 9.74 | 5 | 0.5 | ok |

## Crossovers

Compiled defaults: Karatsuba from 24 limbs, NTT from 1500 limbs (64-bit limbs; 1 limb = 19.3 digits).

### Karatsuba threshold (sum of times for 64, 128, 256, 512, 1024-limb products, us)

| threshold (limbs) | time |
|---|---|
| 8 | 484.8 |
| 12 | 333.8 |
| 16 | 333.7 |
| 20 | 298.3 |
| 24 | 296.8 |
| 32 | 297.1 |
| 40 | 312.5 |
| 48 | 312.3 |
| 64 | 323.3 |
| 96 | 376.7 |

Best Karatsuba threshold: **24 limbs**.

### Karatsuba against NTT (balanced products, us per product)

| limbs | digits | Karatsuba | NTT | num-bigint | NTT / Karatsuba |
|---|---|---|---|---|---|
| 256 | 4932 | 21.5 | 34.2 | 22.1 | 1.59 |
| 384 | 7398 | 41.7 | 69.1 | 42.6 | 1.66 |
| 512 | 9864 | 78.4 | 71.7 | 62.0 | 0.91 |
| 640 | 12330 | 94.7 | 147.3 | 91.7 | 1.56 |
| 768 | 14796 | 129.2 | 149.1 | 113.9 | 1.15 |
| 1024 | 19728 | 201.6 | 152.6 | 179.4 | 0.76 |
| 1280 | 24660 | 285.7 | 312.8 | 374.5 | 1.09 |
| 1536 | 29592 | 398.5 | 322.4 | 329.3 | 0.81 |
| 2048 | 39456 | 614.0 | 325.9 | 511.9 | 0.53 |
| 2560 | 49320 | 869.9 | 754.6 | 673.9 | 0.87 |
| 3072 | 59184 | 1198.1 | 753.2 | 907.6 | 0.63 |
| 4096 | 78913 | 1928.8 | 764.5 | 1449.8 | 0.40 |
| 6144 | 118369 | 3597.8 | 1511.9 | 2733.7 | 0.42 |
| 8192 | 157826 | 5632.2 | 1599.9 | 3948.3 | 0.28 |

NTT faster from **1536 limbs** on (at every measured size from there).

## Ball arithmetic

Enclosure: 3000 random cases (precisions 24 to 700 bits, exact and wide radii), 243264 exact checks of add/sub/mul/div/sqrt/compare at the inputs' end, middle and interior points, plus 300 chains of 30 operations: **0 failures**; 523 undefined results (divisor containing zero, sqrt of a non-positive ball). 153 ms.

Overhead of radius bookkeeping: ns per operation (median), bare = the same rounded midpoint operation.

| digits | bits | op | bare | ball | overhead | v1 BigFloat |
|---|---|---|---|---|---|---|
| 100 | 333 | add | 194 | 197 | +1.8% | 436 |
| 100 | 333 | mul | 161 | 193 | +20.4% | 291 |
| 100 | 333 | div | 357 | 403 | +12.7% | 837 |
| 100 | 333 | sqrt | 2147 | 2207 | +2.8% | 3414 |
| 1000 | 3322 | add | 330 | 341 | +3.1% | 882 |
| 1000 | 3322 | mul | 1636 | 1664 | +1.7% | 6322 |
| 1000 | 3322 | div | 3590 | 3649 | +1.7% | 19597 |
| 1000 | 3322 | sqrt | 13377 | 13422 | +0.3% | 97811 |
| 10000 | 33220 | add | 1062 | 1076 | +1.3% | 4537 |
| 10000 | 33220 | mul | 67637 | 67656 | +0.0% | 245627 |
| 10000 | 33220 | div | 328966 | 328769 | -0.1% | 1355743 |
| 10000 | 33220 | sqrt | 673924 | 674293 | +0.1% | 9367800 |
| 100000 | 332193 | add | 10691 | 10707 | +0.2% | 42003 |
| 100000 | 332193 | mul | 1531075 | 1522475 | -0.6% | 9576850 |
| 100000 | 332193 | div | 8079450 | 7968250 | -1.4% | 129540200 |
| 100000 | 332193 | sqrt | 17460600 | 17445800 | -0.1% | 1165332300 |

## Constants (N1, N2)

### N1: digits of pi (Chudnovsky, binary splitting)

binary = pi 2^p as an integer; total = including the decimal conversion. Times in ms.

| digits | implementation | binary | total | peak heap | digits agree |
|---|---|---|---|---|---|
| 10000 | own | 2.08 | 2.92 | 0.1 | reference |
| 10000 | own, Machin (check only) | | 33.9 | | true |
| 10000 | num-bigint 0.4, same algorithm | | 2.51 | 0.1 | true |
| 10000 | v1 kernel (BigFloat::pi, to_decimal) | 4.99 | 8.63 | 0.1 | true |
| 10000 | checks | | | | ok (leading 50 digits: ok; six nines at 762: ok; self-consistent with d + 100: ok; Machin's formula: ok) |
| 100000 | own | 49.3 | 71.2 | 1.3 | reference |
| 100000 | num-bigint 0.4, same algorithm | | 99.5 | 1.5 | true |
| 100000 | v1 kernel (BigFloat::pi, to_decimal) | 451 | 726 | 0.9 | true |
| 100000 | checks | | | | ok (leading 50 digits: ok; six nines at 762: ok; self-consistent with d + 100: ok) |

### N2: 20 digits of e starting after position off

targeted = ((num 10^off) mod den) 10^k / den on the binary-splitting fraction, no full conversion; full = all digits converted. Times in ms.

| off | implementation | time | peak heap | digits | agree |
|---|---|---|---|---|---|
| 10000 | own, targeted | 1.12 | 0.1 | 56743028597460014378 | reference |
| 10000 | own, full conversion | 1.59 | 0.1 | | true |
| 10000 | own, continued fraction (check only) | 8.99 | | | true |
| 10000 | num-bigint 0.4, targeted | 0.558 | 0.1 | | true |
| 10000 | v1 kernel (BigFloat::e, to_decimal) | 62.0 | 0.1 | | true |
| 100000 | own, targeted | 19.2 | 1.0 | 15778308922988924267 | reference |
| 100000 | own, full conversion | 31.1 | 1.0 | | true |
| 100000 | own, continued fraction (check only) | 111 | | | true |
| 100000 | num-bigint 0.4, targeted | 14.9 | 0.8 | | true |
| 100000 | v1 kernel (BigFloat::e, to_decimal) | 6181 | 0.7 | | true |

## N11: adaptive precision, n(e, 50)

Start at 50 log2(10) + 16 = 183 bits, double until the ball has 50 correct digits (cap 2^14 bits). fixed = one evaluation at the final precision.

| id | expression | rounds | final bits | reached | encloses exact | time (ms) | fixed (ms) | value (50 digits) |
|---|---|---|---|---|---|---|---|---|
| E1 | sqrt(10^60 + 1) - 10^30 | 3 | 732 | true | true | 0.022 | 0.006 | 5.0000000000000000000000000000000000000000000000000e-31 |
| E2 | Muller recurrence u_40 | 3 | 732 | true | true | 0.225 | 0.091 | 6.0009079941545270832407027114923527473913627837936e0 |
| E3 | pi - 51-digit truncation | 2 | 366 | true | true | 0.013 | 0.008 | 5.8209749445923078164062862089986280348253421170680e-51 |
| E4 | Rump's polynomial | 1 | 183 | true | true | 0.004 | 0.003 | -8.2739605994682136814116509547981629199903311578438e-1 |
| E5 | sqrt(2)^2 - 2 (exact zero) | 7 | 11712 | false | true | 0.227 | 0.116 | failure(precision_not_reached): radius 2^-11709 |

Correct bits by round (relative to the midpoint; 0 = the ball contains zero; - = undefined):

| id | 183 | 366 | 732 | 1464 |
|---|---|---|---|---|
| E1 | 0 | 165 | 531 | 1263 |
| E2 | - | 163 | 529 | 1261 |
| E3 | 14 | 197 | 563 | 1295 |
| E4 | 182 | 365 | 731 | 1463 |
| E5 | 0 | 0 | 0 | 0 |

Total wall time: 21.7 s
