# Numbers spike: full run

Machine: AMD64 Family 25 Model 97 Stepping 2, AuthenticAMD (16 hardware threads), windows; v1 comparison on.

binary = pi 2^p as an integer; total = including the decimal conversion. Times in ms.
| digits | implementation | binary | total | peak heap | digits agree |

|---|---|---|---|---|---|
## Constants (N1, N2)

### N1: digits of pi (Chudnovsky, binary splitting)

| 10000 | own | 2.18 | 2.97 | 0.1 | reference |
| 10000 | own, Machin (check only) | | 34.6 | | true |
| 10000 | num-bigint 0.4, same algorithm | | 2.32 | 0.1 | true |
| 10000 | v1 kernel (BigFloat::pi, to_decimal) | 5.15 | 8.85 | 0.1 | true |
| 10000 | checks | | | | ok (leading 50 digits: ok; six nines at 762: ok; self-consistent with d + 100: ok; Machin's formula: ok) |
| 100000 | own | 50.3 | 89.6 | 1.4 | reference |
| 100000 | own, Machin (check only) | | 3274 | | true |
| 100000 | num-bigint 0.4, same algorithm | | 96.2 | 1.6 | true |
| 100000 | v1 kernel (BigFloat::pi, to_decimal) | 457 | 734 | 1.0 | true |
| 100000 | checks | | | | ok (leading 50 digits: ok; six nines at 762: ok; self-consistent with d + 100: ok; Machin's formula: ok) |
| 1000000 | own | 760 | 1025 | 16.1 | reference |
| 1000000 | num-bigint 0.4, same algorithm | | 3459 | 17.9 | true |
| 1000000 | checks | | | | ok (leading 50 digits: ok; six nines at 762: ok; self-consistent with d + 100: ok; digit counts of the first 10^6 decimals: ok) |
| 1000000 | v1 kernel (BigFloat::pi, to_decimal) | | > 60000 (limit; `v1big`) | | - |


targeted = ((num 10^off) mod den) 10^k / den on the binary-splitting fraction, no full conversion; full = all digits converted. Times in ms.

| off | implementation | time | peak heap | digits | agree |
### N2: 20 digits of e starting after position off
|---|---|---|---|---|---|
| 10000 | own, targeted | 1.11 | 0.1 | 56743028597460014378 | reference |
| 10000 | own, full conversion | 1.57 | 0.1 | | true |
| 10000 | own, continued fraction (check only) | 9.10 | | | true |
| 10000 | num-bigint 0.4, targeted | 0.550 | 0.1 | | true |
| 10000 | v1 kernel (BigFloat::e, to_decimal) | 63.0 | 0.1 | | true |
| 100000 | own, targeted | 18.9 | 1.0 | 15778308922988924267 | reference |
| 100000 | own, full conversion | 31.1 | 1.1 | | true |
| 100000 | own, continued fraction (check only) | 112 | | | true |
| 100000 | num-bigint 0.4, targeted | 14.6 | 0.8 | | true |
| 100000 | v1 kernel (BigFloat::e, to_decimal) | 6222 | 0.7 | | true |
| 1000000 | own, targeted | 242 | 11.3 | 83747115156239682713 | reference |
| 1000000 | own, full conversion | 400 | 12.3 | | true |
| 1000000 | own, continued fraction (check only) | 1332 | | | true |
| 1000000 | num-bigint 0.4, targeted | 407 | 8.4 | | true |
| 1000000 | v1 kernel (BigFloat::e, to_decimal) | > 60000 (limit; `v1big`) | | | - |

Total wall time: 50.5 s
