
# Numbers spike: full run
Machine: AMD64 Family 25 Model 97 Stepping 2, AuthenticAMD (16 hardware threads), windows; v1 comparison on.


## Multiplication of two n-digit integers
Times in ms (median); peak heap in MB above the inputs; check = fingerprint equal to own/auto.

| n digits | implementation | median | min | runs | peak heap | check |
|---|---|---|---|---|---|---|
| 1000 | own auto | 0.002 | 0.002 | 50 | 0.0 | ok |
| 1000 | own schoolbook | 0.003 | 0.003 | 50 | 0.0 | ok |
| 1000 | own Karatsuba (no NTT) | 0.003 | 0.003 | 50 | 0.0 | ok |
| 1000 | own NTT (top level) | 0.009 | 0.009 | 50 | 0.0 | ok |
| 1000 | num-bigint 0.4 | 0.002 | 0.002 | 50 | 0.0 | ok |
| 1000 | v1 kernel | 0.010 | 0.009 | 50 | 0.0 | ok |
| 10000 | own auto | 0.070 | 0.069 | 50 | 0.1 | ok |
| 10000 | own schoolbook | 0.192 | 0.192 | 50 | 0.0 | ok |
| 10000 | own Karatsuba (no NTT) | 0.069 | 0.069 | 50 | 0.1 | ok |
| 10000 | own NTT (top level) | 0.163 | 0.158 | 50 | 0.3 | ok |
| 10000 | num-bigint 0.4 | 0.068 | 0.063 | 50 | 0.0 | ok |
| 10000 | v1 kernel | 0.248 | 0.245 | 50 | 0.1 | ok |
| 100000 | own auto | 1.53 | 1.50 | 50 | 2.0 | ok |
| 100000 | own schoolbook | 19.0 | 18.7 | 50 | 0.2 | ok |
| 100000 | own Karatsuba (no NTT) | 2.75 | 2.72 | 50 | 0.5 | ok |
| 100000 | own NTT (top level) | 1.51 | 1.48 | 50 | 0.6 | ok |
| 100000 | num-bigint 0.4 | 2.17 | 2.07 | 50 | 0.5 | ok |
| 100000 | v1 kernel | 9.76 | 9.60 | 50 | 0.5 | ok |
| 1000000 | own auto | 14.5 | 14.4 | 50 | 16.0 | ok |
| 1000000 | own schoolbook | 1915 | 1902 | 3 | 1.7 | ok |
| 1000000 | own Karatsuba (no NTT) | 112 | 112 | 17 | 5.0 | ok |
| 1000000 | own NTT (top level) | 14.7 | 14.5 | 50 | 5.0 | ok |
| 1000000 | num-bigint 0.4 | 62.3 | 61.8 | 31 | 4.8 | ok |
| 1000000 | v1 kernel | 403 | 403 | 3 | 5.0 | ok |
| 10000000 | own auto | 137 | 135 | 11 | 129.9 | ok |
| 10000000 | own Karatsuba (no NTT) | 4211 | 4195 | 3 | 49.8 | ok |
| 10000000 | own NTT (top level) | 135 | 134 | 14 | 41.9 | ok |
| 10000000 | num-bigint 0.4 | 1872 | 1861 | 3 | 49.0 | ok |
| 10000000 | v1 kernel | 14796 | 14796 | 1 | 41.5 | ok |
Total wall time: 51.8 s

