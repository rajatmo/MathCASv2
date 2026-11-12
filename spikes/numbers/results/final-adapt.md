# Numbers spike: full run
Machine: AMD64 Family 25 Model 97 Stepping 2, AuthenticAMD (16 hardware threads), windows; v1 comparison on.


Start at 50 log2(10) + 16 = 183 bits, double until the ball has 50 correct digits (cap 2^14 bits). fixed = one evaluation at the final precision.

| id | expression | rounds | final bits | reached | encloses exact | time (ms) | fixed (ms) | value (50 digits) |

## N11: adaptive precision, n(e, 50)
|---|---|---|---|---|---|---|---|---|
| E1 | sqrt(10^60 + 1) - 10^30 | 3 | 732 | true | true | 0.012 | 0.004 | 5.0000000000000000000000000000000000000000000000000e-31 |
| E2 | Muller recurrence u_40 | 3 | 732 | true | true | 0.185 | 0.075 | 6.0009079941545270832407027114923527473913627837936e0 |
| E3 | pi - 51-digit truncation | 2 | 366 | true | true | 0.014 | 0.007 | 5.8209749445923078164062862089986280348253421170680e-51 |
| E4 | Rump's polynomial | 1 | 183 | true | true | 0.003 | 0.003 | -8.2739605994682136814116509547981629199903311578438e-1 |
| E5 | sqrt(2)^2 - 2 (exact zero) | 7 | 11712 | false | true | 0.189 | 0.120 | failure(precision_not_reached): radius 2^-11709 |


Correct bits by round (relative to the midpoint; 0 = the ball contains zero; - = undefined):
| id | 183 | 366 | 732 | 1464 |
|---|---|---|---|---|
| E1 | 0 | 165 | 531 | 1263 |
| E2 | - | 163 | 529 | 1261 |
| E3 | 14 | 197 | 563 | 1295 |
| E4 | 182 | 365 | 731 | 1463 |
| E5 | 0 | 0 | 0 | 0 |

Total wall time: 0.0 s
