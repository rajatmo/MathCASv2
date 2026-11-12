
# Numbers spike: full run
Machine: AMD64 Family 25 Model 97 Stepping 2, AuthenticAMD (16 hardware threads), windows; v1 comparison on.

### Karatsuba threshold (sum of times for 64, 128, 256, 512, 1024-limb products, us)
| threshold (limbs) | time |
|---|---|

## Crossovers


Compiled defaults: Karatsuba from 24 limbs, NTT from 1500 limbs (64-bit limbs; 1 limb = 19.3 digits).
| 8 | 521.1 |
| 12 | 433.2 |
| 16 | 345.4 |
| 20 | 301.6 |
| 24 | 299.9 |
| 32 | 325.0 |
| 40 | 311.1 |
| 48 | 311.0 |
| 64 | 310.3 |
| 96 | 602.6 |


### Karatsuba against NTT (balanced products, us per product)
| limbs | digits | Karatsuba | NTT | num-bigint | NTT / Karatsuba |
Best Karatsuba threshold: **24 limbs**.

|---|---|---|---|---|---|
| 256 | 4932 | 21.4 | 34.3 | 22.1 | 1.61 |
| 384 | 7398 | 41.7 | 69.8 | 41.1 | 1.67 |
| 512 | 9864 | 65.9 | 72.0 | 60.9 | 1.09 |
| 640 | 12330 | 94.5 | 149.3 | 88.4 | 1.58 |
| 768 | 14796 | 130.8 | 152.4 | 112.8 | 1.17 |
| 1024 | 19728 | 201.7 | 154.6 | 176.0 | 0.77 |
| 1280 | 24660 | 290.7 | 317.8 | 251.7 | 1.09 |
| 1536 | 29592 | 398.4 | 319.8 | 325.5 | 0.80 |
| 2048 | 39456 | 617.9 | 328.1 | 511.5 | 0.53 |
| 2560 | 49320 | 878.6 | 675.8 | 706.5 | 0.77 |
| 3072 | 59184 | 1200.1 | 681.3 | 912.5 | 0.57 |
| 4096 | 78913 | 1867.5 | 703.9 | 1476.1 | 0.38 |
| 6144 | 118369 | 3664.1 | 1457.4 | 2701.8 | 0.40 |
| 8192 | 157826 | 5663.3 | 1467.6 | 6255.4 | 0.26 |
| 12288 | 236739 | 10921.9 | 3442.5 | 9505.7 | 0.32 |
| 16384 | 315652 | 17377.5 | 3136.0 | 11423.6 | 0.18 |
| 32768 | 631305 | 52394.2 | 6570.8 | 32449.3 | 0.13 |


Total wall time: 20.6 s
NTT faster from **1536 limbs** on (at every measured size from there).
