# Numbers spike: parts of the --full run on a busy machine (preliminary)

Taken while three other spikes ran. mul and cross with --features v1; const without v1 (v1 at 10^6 digits exceeds the 2-minute rule for agents); see REPORT.md.

## Multiplication of two n-digit integers

Times in ms (median); peak heap in MB above the inputs; check = fingerprint equal to own/auto.

| n digits | implementation | median | min | runs | peak heap | check |
|---|---|---|---|---|---|---|
| 1000 | own auto | 0.002 | 0.002 | 50 | 0.0 | ok |
| 1000 | own schoolbook | 0.002 | 0.002 | 50 | 0.0 | ok |
| 1000 | own Karatsuba (no NTT) | 0.002 | 0.002 | 50 | 0.0 | ok |
| 1000 | own NTT (top level) | 0.009 | 0.009 | 50 | 0.0 | ok |
| 1000 | num-bigint 0.4 | 0.002 | 0.002 | 50 | 0.0 | ok |
| 1000 | v1 kernel | 0.009 | 0.009 | 50 | 0.0 | ok |
| 10000 | own auto | 0.069 | 0.068 | 50 | 0.1 | ok |
| 10000 | own schoolbook | 0.191 | 0.190 | 50 | 0.0 | ok |
| 10000 | own Karatsuba (no NTT) | 0.069 | 0.068 | 50 | 0.1 | ok |
| 10000 | own NTT (top level) | 0.164 | 0.160 | 50 | 0.3 | ok |
| 10000 | num-bigint 0.4 | 0.065 | 0.063 | 50 | 0.0 | ok |
| 10000 | v1 kernel | 0.247 | 0.245 | 50 | 0.1 | ok |
| 100000 | own auto | 1.51 | 1.49 | 50 | 2.0 | ok |
| 100000 | own schoolbook | 19.0 | 18.9 | 50 | 0.2 | ok |
| 100000 | own Karatsuba (no NTT) | 2.80 | 2.79 | 50 | 0.5 | ok |
| 100000 | own NTT (top level) | 1.51 | 1.49 | 50 | 0.6 | ok |
| 100000 | num-bigint 0.4 | 2.14 | 2.09 | 50 | 0.5 | ok |
| 100000 | v1 kernel | 9.79 | 9.73 | 50 | 0.5 | ok |
| 1000000 | own auto | 15.0 | 14.5 | 50 | 16.0 | ok |
| 1000000 | own schoolbook | 1918 | 1913 | 3 | 1.7 | ok |
| 1000000 | own Karatsuba (no NTT) | 112 | 111 | 17 | 5.0 | ok |
| 1000000 | own NTT (top level) | 14.7 | 14.4 | 50 | 5.0 | ok |
| 1000000 | num-bigint 0.4 | 62.2 | 61.6 | 30 | 4.8 | ok |
| 1000000 | v1 kernel | 404 | 402 | 3 | 5.0 | ok |
| 10000000 | own auto | 135 | 135 | 11 | 129.9 | ok |
| 10000000 | own Karatsuba (no NTT) | 4201 | 4094 | 3 | 49.8 | ok |
| 10000000 | own NTT (top level) | 133 | 133 | 15 | 41.9 | ok |
| 10000000 | num-bigint 0.4 | 1812 | 1807 | 3 | 49.0 | ok |
| 10000000 | v1 kernel | 14438 | 14438 | 1 | 41.5 | ok |


## Crossovers

Compiled defaults: Karatsuba from 24 limbs, NTT from 1500 limbs (64-bit limbs; 1 limb = 19.3 digits).

### Karatsuba threshold (sum of times for 64, 128, 256, 512, 1024-limb products, us)

| threshold (limbs) | time |
|---|---|
| 8 | 513.9 |
| 12 | 350.8 |
| 16 | 351.6 |
| 20 | 443.5 |
| 24 | 309.9 |
| 32 | 306.2 |
| 40 | 313.4 |
| 48 | 312.9 |
| 64 | 309.7 |
| 96 | 372.4 |

Best Karatsuba threshold: **32 limbs**.

### Karatsuba against NTT (balanced products, us per product)

| limbs | digits | Karatsuba | NTT | num-bigint | NTT / Karatsuba |
|---|---|---|---|---|---|
| 256 | 4932 | 21.3 | 34.2 | 22.2 | 1.60 |
| 384 | 7398 | 41.9 | 69.8 | 41.6 | 1.66 |
| 512 | 9864 | 66.1 | 71.8 | 61.0 | 1.09 |
| 640 | 12330 | 93.9 | 145.1 | 87.7 | 1.55 |
| 768 | 14796 | 129.0 | 147.6 | 112.3 | 1.14 |
| 1024 | 19728 | 203.5 | 152.2 | 176.3 | 0.75 |
| 1280 | 24660 | 285.6 | 310.3 | 251.4 | 1.09 |
| 1536 | 29592 | 392.5 | 314.0 | 322.0 | 0.80 |
| 2048 | 39456 | 611.7 | 322.4 | 517.9 | 0.53 |
| 2560 | 49320 | 865.1 | 663.3 | 671.4 | 0.77 |
| 3072 | 59184 | 1187.8 | 671.7 | 918.9 | 0.57 |
| 4096 | 78913 | 1844.2 | 684.5 | 1428.0 | 0.37 |
| 6144 | 118369 | 3564.9 | 1408.5 | 2683.7 | 0.40 |
| 8192 | 157826 | 5570.9 | 1440.6 | 3969.3 | 0.26 |
| 12288 | 236739 | 10816.6 | 2967.9 | 7316.8 | 0.27 |
| 16384 | 315652 | 16992.7 | 3041.6 | 11091.5 | 0.18 |
| 32768 | 631305 | 51162.7 | 6412.5 | 31849.0 | 0.13 |

NTT faster from **1536 limbs** on (at every measured size from there).


## Constants (N1, N2)

### N1: digits of pi (Chudnovsky, binary splitting)

binary = pi 2^p as an integer; total = including the decimal conversion. Times in ms.

| digits | implementation | binary | total | peak heap | digits agree |
|---|---|---|---|---|---|
| 10000 | own | 2.10 | 2.93 | 0.1 | reference |
| 10000 | own, Machin (check only) | | 33.0 | | true |
| 10000 | num-bigint 0.4, same algorithm | | 2.27 | 0.1 | true |
| 10000 | checks | | | | ok (leading 50 digits: ok; six nines at 762: ok; self-consistent with d + 100: ok; Machin's formula: ok) |
| 100000 | own | 48.8 | 68.8 | 1.4 | reference |
| 100000 | own, Machin (check only) | | 3079 | | true |
| 100000 | num-bigint 0.4, same algorithm | | 96.3 | 1.6 | true |
| 100000 | checks | | | | ok (leading 50 digits: ok; six nines at 762: ok; self-consistent with d + 100: ok; Machin's formula: ok) |
| 1000000 | own | 732 | 997 | 16.1 | reference |
| 1000000 | num-bigint 0.4, same algorithm | | 3393 | 17.9 | true |
| 1000000 | checks | | | | ok (leading 50 digits: ok; six nines at 762: ok; self-consistent with d + 100: ok; digit counts of the first 10^6 decimals: ok) |

### N2: 20 digits of e starting after position off

targeted = ((num 10^off) mod den) 10^k / den on the binary-splitting fraction, no full conversion; full = all digits converted. Times in ms.

| off | implementation | time | peak heap | digits | agree |
|---|---|---|---|---|---|
| 10000 | own, targeted | 1.13 | 0.1 | 56743028597460014378 | reference |
| 10000 | own, full conversion | 1.61 | 0.1 | | true |
| 10000 | own, continued fraction (check only) | 9.46 | | | true |
| 10000 | num-bigint 0.4, targeted | 0.558 | 0.1 | | true |
| 100000 | own, targeted | 19.0 | 1.0 | 15778308922988924267 | reference |
| 100000 | own, full conversion | 31.1 | 1.1 | | true |
| 100000 | own, continued fraction (check only) | 114 | | | true |
| 100000 | num-bigint 0.4, targeted | 14.4 | 0.8 | | true |
| 1000000 | own, targeted | 241 | 11.3 | 83747115156239682713 | reference |
| 1000000 | own, full conversion | 400 | 12.3 | | true |
| 1000000 | own, continued fraction (check only) | 1399 | | | true |
| 1000000 | num-bigint 0.4, targeted | 478 | 8.4 | | true |


## Ball arithmetic

Enclosure: 30000 random cases (precisions 24 to 700 bits, exact and wide radii), 2430444 exact checks of add/sub/mul/div/sqrt/compare at the inputs' end, middle and interior points, plus 3000 chains of 30 operations: **0 failures**; 5310 undefined results (divisor containing zero, sqrt of a non-positive ball). 1484 ms.

Overhead of radius bookkeeping: ns per operation (median), bare = the same rounded midpoint operation.

| digits | bits | op | bare | ball | overhead | v1 BigFloat |
|---|---|---|---|---|---|---|
| 100 | 333 | add | 193 | 198 | +2.2% | 428 |
| 100 | 333 | mul | 153 | 182 | +18.8% | 299 |
| 100 | 333 | div | 352 | 396 | +12.6% | 837 |
| 100 | 333 | sqrt | 2066 | 2104 | +1.8% | 3462 |
| 1000 | 3322 | add | 338 | 348 | +2.9% | 870 |
| 1000 | 3322 | mul | 1632 | 1661 | +1.8% | 6331 |
| 1000 | 3322 | div | 3576 | 3613 | +1.0% | 19417 |
| 1000 | 3322 | sqrt | 13297 | 13348 | +0.4% | 97462 |
| 10000 | 33220 | add | 1044 | 1053 | +0.8% | 4555 |
| 10000 | 33220 | mul | 69987 | 70637 | +0.9% | 249343 |
| 10000 | 33220 | div | 331731 | 332556 | +0.2% | 1365555 |
| 10000 | 33220 | sqrt | 678237 | 678254 | +0.0% | 9253350 |
| 100000 | 332193 | add | 14617 | 14766 | +1.0% | 47909 |
| 100000 | 332193 | mul | 1418769 | 1420917 | +0.2% | 9647950 |
| 100000 | 332193 | div | 7669017 | 7651225 | -0.2% | 130800900 |
| 100000 | 332193 | sqrt | 17328080 | 17320460 | -0.0% | 1166465300 |

## N11: adaptive precision, n(e, 50)

Start at 50 log2(10) + 16 = 183 bits, double until the ball has 50 correct digits (cap 2^14 bits). fixed = one evaluation at the final precision.

| id | expression | rounds | final bits | reached | encloses exact | time (ms) | fixed (ms) | value (50 digits) |
|---|---|---|---|---|---|---|---|---|
| E1 | sqrt(10^60 + 1) - 10^30 | 3 | 732 | true | true | 0.012 | 0.006 | 5.0000000000000000000000000000000000000000000000000e-31 |
| E2 | Muller recurrence u_40 | 3 | 732 | true | true | 0.185 | 0.075 | 6.0009079941545270832407027114923527473913627837936e0 |
| E3 | pi - 51-digit truncation | 2 | 366 | true | true | 0.013 | 0.007 | 5.8209749445923078164062862089986280348253421170680e-51 |
| E4 | Rump's polynomial | 1 | 183 | true | true | 0.003 | 0.003 | -8.2739605994682136814116509547981629199903311578438e-1 |
| E5 | sqrt(2)^2 - 2 (exact zero) | 7 | 11712 | false | true | 0.190 | 0.120 | failure(precision_not_reached): radius 2^-11709 |

Correct bits by round (relative to the midpoint; 0 = the ball contains zero; - = undefined):

| id | 183 | 366 | 732 | 1464 |
|---|---|---|---|---|
| E1 | 0 | 165 | 531 | 1263 |
| E2 | - | 163 | 529 | 1261 |
| E3 | 14 | 197 | 563 | 1295 |
| E4 | 182 | 365 | 731 | 1463 |
| E5 | 0 | 0 | 0 | 0 |


## v1 at 10^6 digits (probe `v1big`)

pi to 10^6 digits: over the 60 s limit (the probe returned between 60 and 120 s after start: v1's quadratic division and conversion do not poll the interrupt flag). e at offset 10^6: still running when the probe was killed at 120 s.
