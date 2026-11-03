
|---|---|---|---|---|---|---|
Machine: AMD64 Family 25 Model 97 Stepping 2, AuthenticAMD (16 hardware threads), windows; v1 comparison on.
# Numbers spike: full run


## Cost breakdown of pi (ms)
| digits | backend | series Q, T | sqrt(10005) | 426880 s Q | / T | to decimal (incl. 10^d product) |
| 10000 | own | 0.727 | 0.755 | 0.152 | 0.508 | 0.816 |
| 10000 | num-bigint | 0.601 | 1.11 | 0.088 | 0.198 | 0.305 |
| 100000 | own | 20.4 | 18.0 | 1.49 | 9.30 | 20.5 |
| 100000 | num-bigint | 17.9 | 58.2 | 3.01 | 8.32 | 8.60 |
| 1000000 | own | 363 | 229 | 30.5 | 129 | 274 |
| 1000000 | num-bigint | 627 | 2205 | 108 | 245 | 262 |

Total wall time: 32.7 s
