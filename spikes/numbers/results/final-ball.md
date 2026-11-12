Machine: AMD64 Family 25 Model 97 Stepping 2, AuthenticAMD (16 hardware threads), windows; v1 comparison on.

# Numbers spike: full run
## Ball arithmetic


Enclosure: 30000 random cases (precisions 24 to 700 bits, exact and wide radii), 2430444 exact checks of add/sub/mul/div/sqrt/compare at the inputs' end, middle and interior points, plus 3000 chains of 30 operations: **0 failures**; 5310 undefined results (divisor containing zero, sqrt of a non-positive ball). 1481 ms.

Overhead of radius bookkeeping: ns per operation (median), bare = the same rounded midpoint operation.

| digits | bits | op | bare | ball | overhead | v1 BigFloat |
|---|---|---|---|---|---|---|
| 100 | 333 | add | 190 | 194 | +2.5% | 432 |
| 100 | 333 | mul | 155 | 187 | +20.5% | 290 |
| 100 | 333 | div | 352 | 398 | +12.9% | 837 |
| 100 | 333 | sqrt | 2020 | 2060 | +2.0% | 3438 |
| 1000 | 3322 | add | 342 | 354 | +3.5% | 797 |
| 1000 | 3322 | mul | 1656 | 1679 | +1.4% | 6364 |
| 1000 | 3322 | div | 3639 | 3686 | +1.3% | 19631 |
| 1000 | 3322 | sqrt | 13300 | 13368 | +0.5% | 98696 |
| 10000 | 33220 | add | 1046 | 1056 | +1.0% | 4519 |
| 10000 | 33220 | mul | 69233 | 69384 | +0.2% | 247399 |
| 10000 | 33220 | div | 332666 | 332979 | +0.1% | 1370222 |
| 10000 | 33220 | sqrt | 686301 | 693120 | +1.0% | 9367610 |
| 100000 | 332193 | add | 10560 | 10599 | +0.4% | 49074 |
| 100000 | 332193 | mul | 1439766 | 1440761 | +0.1% | 9706690 |
| 100000 | 332193 | div | 7848950 | 7843758 | -0.1% | 132301400 |
| 100000 | 332193 | sqrt | 17697860 | 17718520 | +0.1% | 1207640900 |

Total wall time: 44.7 s
