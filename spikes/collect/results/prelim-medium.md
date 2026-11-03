| case | size | store | total ms | phases (ms) | GCs | pause total | pause max | pause mean | live nodes after last GC | reserved after last GC MB | max reserved after GC MB | peak heap MB | overhead | check | notes |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| S3 parts | 10000 | hc+imm+stamps | 21.56 | build 0.114, 10^4 replacements 21.44 | 0 | 0.000 | 0.000 | 0.000 | - | - | - | 43.2 | +0% | ok |  |
| S3 parts | 10000 | none | 21.47 | build 0.171, 10^4 replacements 21.30 | 0 | 0.000 | 0.000 | 0.000 | - | - | - | 43.2 | +0% | ok |  |
| S3 parts | 10000 | A | 17.27 | build 0.156, 10^4 replacements 17.12 | 9 | 1.09 | 0.139 | 0.121 | 1001 | 4.5 | 4.5 | 9.7 | -20% | ok |  |
| S3 parts | 10000 | A-fl | 13.37 | build 0.098, 10^4 replacements 13.28 | 9 | 0.362 | 0.065 | 0.040 | 1001 | 5.4 | 5.4 | 5.5 | -38% | ok |  |
| S3 parts | 10000 | B | 13.46 | build 0.110, 10^4 replacements 13.35 | 9 | 0.408 | 0.060 | 0.045 | 1001 | 4.4 | 4.4 | 5.4 | -37% | ok |  |
| S3 parts | 10000 | C | 19.25 | build 0.103, 10^4 replacements 19.15 | 4 (4 minor) | 0.710 | 0.190 | 0.178 | 3609 | 4.8 | 4.8 | 10.8 | -10% | ok |  |
| S3 parts | 10000 | C1 | 12.77 | build 0.118, 10^4 replacements 12.65 | 39 (39 minor) | 0.472 | 0.049 | 0.012 | 9686 | 3.2 | 3.2 | 3.3 | -41% | ok |  |
| S3 parts | 10000 | C0 | 29.39 | build 0.119, 10^4 replacements 29.27 | 10010 (10001 minor) | 15.43 | 0.068 | 0.002 | 2625 | 4.5 | 5.4 | 5.9 | +37% | ok |  |
| S3 parts | 100000 | hc+imm+stamps | 24.00 | build 0.434, 10^4 replacements 23.56 | 0 | 0.000 | 0.000 | 0.000 | - | - | - | 53.7 | -1% | ok |  |
| S3 parts | 100000 | none | 24.30 | build 0.469, 10^4 replacements 23.83 | 0 | 0.000 | 0.000 | 0.000 | - | - | - | 53.7 | +0% | ok |  |
| S3 parts | 100000 | A | 19.69 | build 0.407, 10^4 replacements 19.28 | 10 | 2.05 | 0.278 | 0.205 | 1001 | 5.2 | 5.2 | 11.7 | -19% | ok |  |
| S3 parts | 100000 | A-fl | 15.24 | build 0.359, 10^4 replacements 14.88 | 10 | 0.824 | 0.159 | 0.082 | 1001 | 6.7 | 6.7 | 6.8 | -37% | ok |  |
| S3 parts | 100000 | B | 15.47 | build 0.375, 10^4 replacements 15.10 | 10 | 1.48 | 0.217 | 0.148 | 1001 | 5.2 | 5.2 | 6.7 | -36% | ok |  |
| S3 parts | 100000 | C | 21.88 | build 0.381, 10^4 replacements 21.50 | 5 (5 minor) | 1.51 | 0.509 | 0.302 | 4383 | 8.1 | 8.1 | 14.5 | -10% | ok |  |
| S3 parts | 100000 | C1 | 15.91 | build 0.377, 10^4 replacements 15.53 | 43 (42 minor) | 1.73 | 0.190 | 0.040 | 1216 | 5.6 | 7.2 | 7.2 | -35% | ok |  |
| S3 parts | 100000 | C0 | 29.16 | build 0.394, 10^4 replacements 28.77 | 10011 (10001 minor) | 14.70 | 0.223 | 0.001 | 2397 | 5.2 | 6.7 | 6.8 | +20% | ok |  |
| S8 alloc | 100000 | hc+imm+stamps | 13.47 | build 13.47 | 0 | 0.000 | 0.000 | 0.000 | - | - | - | 11.9 | -1% | ok |  |
| S8 alloc | 100000 | none | 13.62 | build 13.62 | 0 | 0.000 | 0.000 | 0.000 | - | - | - | 11.9 | +0% | ok |  |
| S8 alloc | 100000 | A | 13.19 | build 13.19 | 1 | 0.569 | 0.569 | 0.569 | 106053 | 5.8 | 5.8 | 11.9 | -3% | ok |  |
| S8 alloc | 100000 | A-fl | 13.35 | build 13.35 | 1 | 0.572 | 0.572 | 0.572 | 106053 | 5.8 | 5.8 | 11.9 | -2% | ok |  |
| S8 alloc | 100000 | B | 14.28 | build 14.28 | 1 | 1.21 | 1.21 | 1.21 | 106053 | 5.8 | 5.8 | 11.9 | +5% | ok |  |
| S8 alloc | 100000 | C | 14.00 | build 14.00 | 0 | 0.000 | 0.000 | 0.000 | - | - | - | 11.9 | +3% | ok |  |
| S8 alloc | 100000 | C1 | 18.27 | build 18.27 | 8 (7 minor) | 7.34 | 2.38 | 0.918 | 196098 | 11.8 | 11.8 | 12.6 | +34% | ok |  |
| S8 alloc | 100000 | C0 | 16.28 | build 16.28 | 101 (100 minor) | 5.88 | 1.21 | 0.058 | 200100 | 11.7 | 11.7 | 11.9 | +20% | ok |  |
| S8 alloc | 1000000 | hc+imm+stamps | 241 | build 241 | 0 | 0.000 | 0.000 | 0.000 | - | - | - | 115.4 | -1% | ok |  |
| S8 alloc | 1000000 | none | 244 | build 244 | 0 | 0.000 | 0.000 | 0.000 | - | - | - | 115.4 | +0% | ok |  |
| S8 alloc | 1000000 | A | 254 | build 254 | 5 | 21.44 | 11.20 | 4.29 | 1920096 | 102.2 | 102.2 | 115.4 | +4% | ok |  |
| S8 alloc | 1000000 | A-fl | 251 | build 251 | 5 | 21.49 | 11.15 | 4.30 | 1920096 | 102.2 | 102.2 | 115.4 | +3% | ok |  |
| S8 alloc | 1000000 | B | 274 | build 274 | 5 | 45.05 | 23.73 | 9.01 | 1920096 | 102.2 | 102.2 | 115.4 | +12% | ok |  |
| S8 alloc | 1000000 | C | 295 | build 295 | 13 (9 minor) | 154 | 51.64 | 11.82 | 1980099 | 103.4 | 103.4 | 116.7 | +21% | ok |  |
| S8 alloc | 1000000 | C1 | 276 | build 276 | 55 (50 minor) | 152 | 45.43 | 2.77 | 2000100 | 102.5 | 102.5 | 115.7 | +13% | ok |  |
| S8 alloc | 1000000 | C0 | 282 | build 282 | 105 (100 minor) | 156 | 46.01 | 1.49 | 2000100 | 102.4 | 102.4 | 115.5 | +16% | ok |  |
| R10 replace | 10000 | hc+imm+stamps | 1.16 | build 0.515, safe point (tree live) 0.000, replace 0.647, safe point (tree dead) 0.000 | 0 | 0.000 | 0.000 | 0.000 | - | - | - | 0.7 | -1% | ok |  |
| R10 replace | 10000 | none | 1.18 | build 0.518, safe point (tree live) 0.000, replace 0.660, safe point (tree dead) 0.000 | 0 | 0.000 | 0.000 | 0.000 | - | - | - | 0.7 | +0% | ok |  |
| R10 replace | 10000 | A | 1.41 | build 0.504, safe point (tree live) 0.059, replace 0.739, safe point (tree dead) 0.105 | 2 | 0.164 | 0.105 | 0.082 | 4502 | 0.7 | 0.7 | 0.7 | +20% | ok |  |
| R10 replace | 10000 | A-fl | 1.38 | build 0.500, safe point (tree live) 0.056, replace 0.711, safe point (tree dead) 0.111 | 2 | 0.167 | 0.111 | 0.083 | 4502 | 0.7 | 0.7 | 0.7 | +17% | ok |  |
| R10 replace | 10000 | B | 1.40 | build 0.499, safe point (tree live) 0.103, replace 0.653, safe point (tree dead) 0.142 | 2 | 0.244 | 0.142 | 0.122 | 4502 | 0.7 | 0.7 | 0.7 | +19% | ok |  |
| R10 replace | 10000 | C | 1.36 | build 0.503, safe point (tree live) 0.143, replace 0.566, safe point (tree dead) 0.147 | 2 (2 minor) | 0.289 | 0.147 | 0.145 | 8500 | 0.7 | 0.7 | 0.8 | +15% | ok |  |
| R10 replace | 10000 | C1 | 1.35 | build 0.493, safe point (tree live) 0.144, replace 0.572, safe point (tree dead) 0.145 | 2 (2 minor) | 0.289 | 0.144 | 0.144 | 8500 | 0.7 | 0.7 | 0.8 | +15% | ok |  |
| R10 replace | 10000 | C0 | 1.35 | build 0.508, safe point (tree live) 0.141, replace 0.563, safe point (tree dead) 0.141 | 2 (2 minor) | 0.281 | 0.141 | 0.141 | 8500 | 0.7 | 0.7 | 0.8 | +15% | ok |  |
| R10 replace | 100000 | hc+imm+stamps | 13.29 | build 5.45, safe point (tree live) 0.000, replace 7.83, safe point (tree dead) 0.000 | 0 | 0.000 | 0.000 | 0.000 | - | - | - | 5.9 | -2% | ok |  |
| R10 replace | 100000 | none | 13.55 | build 5.65, safe point (tree live) 0.000, replace 7.89, safe point (tree dead) 0.000 | 0 | 0.000 | 0.000 | 0.000 | - | - | - | 5.9 | +0% | ok |  |
| R10 replace | 100000 | A | 14.56 | build 5.49, safe point (tree live) 0.607, replace 7.24, safe point (tree dead) 1.22 | 2 | 1.83 | 1.22 | 0.914 | 40391 | 6.1 | 6.1 | 6.2 | +7% | ok |  |
| R10 replace | 100000 | A-fl | 15.05 | build 5.69, safe point (tree live) 0.602, replace 7.41, safe point (tree dead) 1.35 | 2 | 1.95 | 1.35 | 0.974 | 40391 | 6.3 | 6.3 | 6.4 | +11% | ok |  |
| R10 replace | 100000 | B | 15.11 | build 5.53, safe point (tree live) 0.921, replace 7.28, safe point (tree dead) 1.38 | 2 | 2.30 | 1.38 | 1.15 | 40391 | 5.8 | 5.8 | 5.9 | +12% | ok |  |
| R10 replace | 100000 | C | 15.90 | build 5.67, safe point (tree live) 1.36, replace 7.11, safe point (tree dead) 1.76 | 2 (2 minor) | 3.12 | 1.76 | 1.56 | 77603 | 6.1 | 6.1 | 6.6 | +17% | ok |  |
| R10 replace | 100000 | C1 | 15.57 | build 5.56, safe point (tree live) 1.37, replace 7.02, safe point (tree dead) 1.61 | 2 (2 minor) | 2.99 | 1.61 | 1.49 | 77603 | 6.1 | 6.1 | 6.6 | +15% | ok |  |
| R10 replace | 100000 | C0 | 15.65 | build 5.69, safe point (tree live) 1.33, replace 6.95, safe point (tree dead) 1.68 | 2 (2 minor) | 3.00 | 1.68 | 1.50 | 77603 | 6.1 | 6.1 | 6.6 | +16% | ok |  |
| R11 re-eval | 10000 | hc+imm+stamps | 1.29 | build 0.561, eval 0.575, collect 0.000, re-eval (unrelated change) 0.000, collect 0.000, re-eval (s3 changed) 0.152 | 0 | 0.000 | 0.000 | 0.000 | - | - | - | 0.4 | -0% | ok | misses eval 4557, misses unrelated 0, misses s3 1274 |
| R11 re-eval | 10000 | none | 1.29 | build 0.553, eval 0.584, collect 0.000, re-eval (unrelated change) 0.000, collect 0.000, re-eval (s3 changed) 0.156 | 0 | 0.000 | 0.000 | 0.000 | - | - | - | 0.4 | +0% | ok | misses eval 6025, misses unrelated 0, misses s3 1568 |
| R11 re-eval | 10000 | A | 1.43 | build 0.545, eval 0.579, collect 0.079, re-eval (unrelated change) 0.000, collect 0.071, re-eval (s3 changed) 0.156 | 2 | 0.149 | 0.078 | 0.075 | 5959 | 0.4 | 0.4 | 0.4 | +11% | ok | misses eval 6025, misses unrelated 0, misses s3 1568 |
| R11 re-eval | 10000 | A-fl | 1.41 | build 0.551, eval 0.581, collect 0.073, re-eval (unrelated change) 0.000, collect 0.052, re-eval (s3 changed) 0.156 | 2 | 0.125 | 0.073 | 0.062 | 5959 | 0.4 | 0.4 | 0.4 | +9% | ok | misses eval 6025, misses unrelated 0, misses s3 1568 |
| R11 re-eval | 10000 | B | 1.58 | build 0.557, eval 0.583, collect 0.143, re-eval (unrelated change) 0.000, collect 0.136, re-eval (s3 changed) 0.158 | 2 | 0.279 | 0.143 | 0.139 | 5959 | 0.4 | 0.4 | 0.4 | +22% | ok | misses eval 6025, misses unrelated 0, misses s3 1568 |
| R11 re-eval | 10000 | C | 1.48 | build 0.556, eval 0.586, collect 0.185, re-eval (unrelated change) 0.000, collect 0.000, re-eval (s3 changed) 0.156 | 1 (1 minor) | 0.185 | 0.185 | 0.185 | 5959 | 0.5 | 0.5 | 0.5 | +15% | ok | misses eval 6025, misses unrelated 0, misses s3 1568 |
| R11 re-eval | 10000 | C1 | 1.49 | build 0.555, eval 0.590, collect 0.184, re-eval (unrelated change) 0.000, collect 0.000, re-eval (s3 changed) 0.162 | 1 (1 minor) | 0.184 | 0.184 | 0.184 | 5959 | 0.5 | 0.5 | 0.5 | +15% | ok | misses eval 6025, misses unrelated 0, misses s3 1568 |
| R11 re-eval | 10000 | C0 | 1.48 | build 0.550, eval 0.588, collect 0.183, re-eval (unrelated change) 0.000, collect 0.000, re-eval (s3 changed) 0.157 | 1 (1 minor) | 0.183 | 0.183 | 0.183 | 5959 | 0.5 | 0.5 | 0.5 | +14% | ok | misses eval 6025, misses unrelated 0, misses s3 1568 |
| R11 re-eval | 100000 | hc+imm+stamps | 13.45 | build 6.13, eval 5.81, collect 0.000, re-eval (unrelated change) 0.000, collect 0.000, re-eval (s3 changed) 1.51 | 0 | 0.000 | 0.000 | 0.000 | - | - | - | 3.6 | +0% | ok | misses eval 40785, misses unrelated 0, misses s3 11890 |
| R11 re-eval | 100000 | none | 13.45 | build 6.05, eval 5.86, collect 0.000, re-eval (unrelated change) 0.000, collect 0.000, re-eval (s3 changed) 1.53 | 0 | 0.000 | 0.000 | 0.000 | - | - | - | 3.6 | +0% | ok | misses eval 52520, misses unrelated 0, misses s3 14347 |
| R11 re-eval | 100000 | A | 14.87 | build 6.04, eval 5.85, collect 0.733, re-eval (unrelated change) 0.000, collect 0.748, re-eval (s3 changed) 1.50 | 2 | 1.48 | 0.747 | 0.740 | 54333 | 3.6 | 3.6 | 3.6 | +11% | ok | misses eval 52520, misses unrelated 0, misses s3 14347 |
| R11 re-eval | 100000 | A-fl | 14.83 | build 6.06, eval 5.81, collect 0.730, re-eval (unrelated change) 0.000, collect 0.734, re-eval (s3 changed) 1.50 | 2 | 1.46 | 0.733 | 0.732 | 54333 | 3.6 | 3.6 | 3.6 | +10% | ok | misses eval 52520, misses unrelated 0, misses s3 14347 |
| R11 re-eval | 100000 | B | 15.89 | build 6.02, eval 5.80, collect 1.28, re-eval (unrelated change) 0.000, collect 1.27, re-eval (s3 changed) 1.52 | 2 | 2.54 | 1.28 | 1.27 | 54333 | 3.6 | 3.6 | 3.6 | +18% | ok | misses eval 52520, misses unrelated 0, misses s3 14347 |
| R11 re-eval | 100000 | C | 15.18 | build 6.06, eval 5.87, collect 1.74, re-eval (unrelated change) 0.000, collect 0.000, re-eval (s3 changed) 1.51 | 1 (1 minor) | 1.74 | 1.74 | 1.74 | 54333 | 3.8 | 3.8 | 4.1 | +13% | ok | misses eval 52520, misses unrelated 0, misses s3 14347 |
| R11 re-eval | 100000 | C1 | 15.14 | build 6.05, eval 5.82, collect 1.75, re-eval (unrelated change) 0.000, collect 0.000, re-eval (s3 changed) 1.53 | 1 (1 minor) | 1.75 | 1.75 | 1.75 | 54333 | 3.8 | 3.8 | 4.1 | +13% | ok | misses eval 52520, misses unrelated 0, misses s3 14347 |
| R11 re-eval | 100000 | C0 | 15.25 | build 6.10, eval 5.87, collect 1.75, re-eval (unrelated change) 0.000, collect 0.000, re-eval (s3 changed) 1.53 | 1 (1 minor) | 1.75 | 1.75 | 1.75 | 54333 | 3.8 | 3.8 | 4.1 | +13% | ok | misses eval 52520, misses unrelated 0, misses s3 14347 |
| LS long session | 2000 | hc+imm+stamps | 317 | 1000 evaluations 317 | 0 | 0.000 | 0.000 | 0.000 | - | - | - | 106.8 | -3% | ok | memo misses 734951 |
| LS long session | 2000 | none | 327 | 1000 evaluations 327 | 0 | 0.000 | 0.000 | 0.000 | - | - | - | 106.6 | +0% | ok | memo misses 934468 |
| LS long session | 2000 | A | 267 | 1000 evaluations 267 | 12 | 28.70 | 3.80 | 2.39 | 110011 | 20.2 | 20.2 | 26.2 | -18% | ok | memo misses 1006783 |
| LS long session | 2000 | A-fl | 253 | 1000 evaluations 253 | 12 | 26.32 | 3.22 | 2.19 | 110011 | 16.9 | 16.9 | 18.5 | -23% | ok | memo misses 1006783 |
| LS long session | 2000 | B | 240 | 1000 evaluations 240 | 12 | 27.04 | 3.78 | 2.25 | 110011 | 15.8 | 15.8 | 17.5 | -26% | ok | memo misses 1006783 |
| LS long session | 2000 | C | 228 | 1000 evaluations 228 | 7 (6 minor) | 9.95 | 2.67 | 1.42 | 110362 | 18.9 | 18.9 | 26.7 | -30% | ok | memo misses 980531 |
| LS long session | 2000 | C1 | 207 | 1000 evaluations 207 | 51 (50 minor) | 8.50 | 2.64 | 0.167 | 111144 | 12.7 | 12.7 | 12.8 | -37% | ok | memo misses 1054954 |
| LS long session | 2000 | C0 | 205 | 1000 evaluations 205 | 1001 (1000 minor) | 8.83 | 2.58 | 0.009 | 112347 | 7.4 | 7.4 | 7.6 | -37% | ok | memo misses 1128419 |
| LS long session | 10000 | hc+imm+stamps | 2111 | 1000 evaluations 2111 | 0 | 0.000 | 0.000 | 0.000 | - | - | - | 461.5 | -1% | ok | memo misses 3439726 |
| LS long session | 10000 | none | 2141 | 1000 evaluations 2141 | 0 | 0.000 | 0.000 | 0.000 | - | - | - | 461.2 | +0% | ok | memo misses 4299478 |
| LS long session | 10000 | A | 1662 | 1000 evaluations 1662 | 29 | 229 | 21.11 | 7.89 | 530644 | 67.2 | 67.3 | 79.7 | -22% | ok | memo misses 4699225 |
| LS long session | 10000 | A-fl | 1620 | 1000 evaluations 1620 | 29 | 233 | 19.93 | 8.02 | 530644 | 66.8 | 66.8 | 72.9 | -24% | ok | memo misses 4699225 |
| LS long session | 10000 | B | 1375 | 1000 evaluations 1375 | 29 | 213 | 18.30 | 7.34 | 530644 | 61.5 | 61.6 | 68.1 | -36% | ok | memo misses 4699225 |
| LS long session | 10000 | C | 1211 | 1000 evaluations 1211 | 32 (29 minor) | 65.78 | 12.28 | 2.06 | 523200 | 64.3 | 64.3 | 67.8 | -43% | ok | memo misses 4707648 |
| LS long session | 10000 | C1 | 1119 | 1000 evaluations 1119 | 212 (209 minor) | 59.94 | 11.09 | 0.283 | 536375 | 52.0 | 52.0 | 52.8 | -48% | ok | memo misses 5005850 |
| LS long session | 10000 | C0 | 1119 | 1000 evaluations 1119 | 1003 (1000 minor) | 59.43 | 11.00 | 0.059 | 537199 | 51.8 | 51.8 | 52.6 | -48% | ok | memo misses 5158630 |
