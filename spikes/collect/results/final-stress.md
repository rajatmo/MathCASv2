| case | size | store | total ms | phases (ms) | GCs | pause total | pause max | pause mean | live nodes after last GC | reserved after last GC MB | max reserved after GC MB | peak heap MB | overhead | check | notes |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| S3 parts | 10000 | hc+imm+stamps | 22.39 | build 0.134, 10^4 replacements 22.26 | 0 | 0.000 | 0.000 | 0.000 | - | - | - | 43.2 | +4% | ok |  |
| S3 parts | 10000 | none | 21.44 | build 0.146, 10^4 replacements 21.30 | 0 | 0.000 | 0.000 | 0.000 | - | - | - | 43.2 | +0% | ok |  |
| S3 parts | 10000 | A | 268 | build 0.113, 10^4 replacements 268 | 10001 | 247 | 0.080 | 0.025 | 1001 | 4.3 | 4.3 | 9.5 | +1149% | ok |  |
| S3 parts | 10000 | A-fl | 235 | build 0.135, 10^4 replacements 235 | 10001 | 222 | 0.097 | 0.022 | 1001 | 0.1 | 0.1 | 1.6 | +996% | ok |  |
| S3 parts | 10000 | B | 346 | build 0.161, 10^4 replacements 346 | 10001 | 334 | 0.111 | 0.033 | 1001 | 0.1 | 0.1 | 1.6 | +1516% | ok |  |
| S3 parts | 10000 | C | 300 | build 0.120, 10^4 replacements 300 | 12501 (10001 minor) | 287 | 0.092 | 0.023 | 1003 | 0.1 | 0.1 | 1.8 | +1298% | ok |  |
| S3 parts | 10000 | C1 | 304 | build 0.102, 10^4 replacements 303 | 12501 (10001 minor) | 290 | 0.405 | 0.023 | 1003 | 0.1 | 0.1 | 1.8 | +1316% | ok |  |
| S3 parts | 10000 | C0 | 274 | build 0.165, 10^4 replacements 274 | 12501 (10001 minor) | 261 | 0.156 | 0.021 | 1003 | 0.1 | 0.1 | 1.8 | +1178% | ok |  |
| S8 alloc | 100000 | hc+imm+stamps | 19.28 | build 19.28 | 0 | 0.000 | 0.000 | 0.000 | - | - | - | 11.9 | +42% | ok |  |
| S8 alloc | 100000 | none | 13.61 | build 13.61 | 0 | 0.000 | 0.000 | 0.000 | - | - | - | 11.9 | +0% | ok |  |
| S8 alloc | 100000 | A | 170 | build 170 | 100 | 158 | 3.25 | 1.58 | 200100 | 11.6 | 11.6 | 11.9 | +1147% | ok |  |
| S8 alloc | 100000 | A-fl | 170 | build 170 | 100 | 158 | 3.23 | 1.58 | 200100 | 11.6 | 11.6 | 11.9 | +1149% | ok |  |
| S8 alloc | 100000 | C | 177 | build 177 | 125 (100 minor) | 166 | 4.46 | 1.33 | 200100 | 11.7 | 11.7 | 11.9 | +1199% | ok |  |
| S8 alloc | 100000 | B | 233 | build 233 | 100 | 221 | 4.67 | 2.21 | 200100 | 11.6 | 11.6 | 11.9 | +1611% | ok |  |
| S8 alloc | 100000 | C0 | 174 | build 174 | 125 (100 minor) | 164 | 4.43 | 1.31 | 200100 | 11.7 | 11.7 | 11.9 | +1177% | ok |  |
| S8 alloc | 100000 | C1 | 176 | build 176 | 125 (100 minor) | 165 | 4.40 | 1.32 | 200100 | 11.7 | 11.7 | 11.9 | +1192% | ok |  |
| R10 replace | 10000 | hc+imm+stamps | 1.16 | build 0.529, safe point (tree live) 0.000, replace 0.629, safe point (tree dead) 0.000 | 0 | 0.000 | 0.000 | 0.000 | - | - | - | 0.7 | +1% | ok |  |
| R10 replace | 10000 | none | 1.15 | build 0.510, safe point (tree live) 0.000, replace 0.635, safe point (tree dead) 0.000 | 0 | 0.000 | 0.000 | 0.000 | - | - | - | 0.7 | +0% | ok |  |
| R10 replace | 10000 | A-fl | 1.44 | build 0.499, safe point (tree live) 0.116, replace 0.642, safe point (tree dead) 0.179 | 2 | 0.295 | 0.179 | 0.148 | 4502 | 0.7 | 0.7 | 0.7 | +25% | ok |  |
| R10 replace | 10000 | B | 1.52 | build 0.514, safe point (tree live) 0.167, replace 0.639, safe point (tree dead) 0.200 | 2 | 0.365 | 0.199 | 0.183 | 4502 | 0.7 | 0.7 | 0.7 | +33% | ok |  |
| R10 replace | 10000 | C | 1.57 | build 0.520, safe point (tree live) 0.207, replace 0.604, safe point (tree dead) 0.241 | 2 (2 minor) | 0.448 | 0.241 | 0.224 | 8500 | 0.7 | 0.7 | 0.8 | +37% | ok |  |
| R10 replace | 10000 | A | 1.42 | build 0.497, safe point (tree live) 0.119, replace 0.640, safe point (tree dead) 0.165 | 2 | 0.283 | 0.164 | 0.141 | 4502 | 0.7 | 0.7 | 0.7 | +24% | ok |  |
| R10 replace | 10000 | C1 | 1.57 | build 0.511, safe point (tree live) 0.215, replace 0.610, safe point (tree dead) 0.237 | 2 (2 minor) | 0.451 | 0.237 | 0.226 | 8500 | 0.7 | 0.7 | 0.8 | +37% | ok |  |
| R10 replace | 10000 | C0 | 1.55 | build 0.498, safe point (tree live) 0.208, replace 0.605, safe point (tree dead) 0.238 | 2 (2 minor) | 0.446 | 0.238 | 0.223 | 8500 | 0.7 | 0.7 | 0.8 | +35% | ok |  |
| R11 re-eval | 10000 | hc+imm+stamps | 1.31 | build 0.566, eval 0.582, collect 0.000, re-eval (unrelated change) 0.000, collect 0.000, re-eval (s3 changed) 0.163 | 0 | 0.000 | 0.000 | 0.000 | - | - | - | 0.4 | -1% | ok | misses eval 4557, misses unrelated 0, misses s3 1274 |
| R11 re-eval | 10000 | A-fl | 1.61 | build 0.565, eval 0.587, collect 0.155, re-eval (unrelated change) 0.000, collect 0.138, re-eval (s3 changed) 0.166 | 2 | 0.293 | 0.155 | 0.146 | 5959 | 0.4 | 0.4 | 0.4 | +22% | ok | misses eval 6025, misses unrelated 0, misses s3 1568 |
| R11 re-eval | 10000 | none | 1.32 | build 0.555, eval 0.599, collect 0.000, re-eval (unrelated change) 0.000, collect 0.000, re-eval (s3 changed) 0.166 | 0 | 0.000 | 0.000 | 0.000 | - | - | - | 0.4 | +0% | ok | misses eval 6025, misses unrelated 0, misses s3 1568 |
| R11 re-eval | 10000 | A | 1.63 | build 0.568, eval 0.587, collect 0.160, re-eval (unrelated change) 0.000, collect 0.153, re-eval (s3 changed) 0.166 | 2 | 0.313 | 0.160 | 0.157 | 5959 | 0.4 | 0.4 | 0.4 | +24% | ok | misses eval 6025, misses unrelated 0, misses s3 1568 |
| R11 re-eval | 10000 | B | 1.75 | build 0.554, eval 0.590, collect 0.224, re-eval (unrelated change) 0.000, collect 0.217, re-eval (s3 changed) 0.167 | 2 | 0.440 | 0.223 | 0.220 | 5959 | 0.4 | 0.4 | 0.4 | +33% | ok | misses eval 6025, misses unrelated 0, misses s3 1568 |
| R11 re-eval | 10000 | C | 1.59 | build 0.559, eval 0.601, collect 0.262, re-eval (unrelated change) 0.000, collect 0.000, re-eval (s3 changed) 0.166 | 1 (1 minor) | 0.261 | 0.261 | 0.261 | 5959 | 0.5 | 0.5 | 0.5 | +20% | ok | misses eval 6025, misses unrelated 0, misses s3 1568 |
| R11 re-eval | 10000 | C1 | 1.55 | build 0.543, eval 0.584, collect 0.260, re-eval (unrelated change) 0.000, collect 0.000, re-eval (s3 changed) 0.166 | 1 (1 minor) | 0.260 | 0.260 | 0.260 | 5959 | 0.5 | 0.5 | 0.5 | +18% | ok | misses eval 6025, misses unrelated 0, misses s3 1568 |
| R11 re-eval | 10000 | C0 | 1.57 | build 0.555, eval 0.588, collect 0.260, re-eval (unrelated change) 0.000, collect 0.000, re-eval (s3 changed) 0.166 | 1 (1 minor) | 0.260 | 0.260 | 0.260 | 5959 | 0.5 | 0.5 | 0.5 | +19% | ok | misses eval 6025, misses unrelated 0, misses s3 1568 |
| LS long session | 2000 | hc+imm+stamps | 310 | 1000 evaluations 310 | 0 | 0.000 | 0.000 | 0.000 | - | - | - | 106.8 | -30% | ok | memo misses 734951 |
| LS long session | 2000 | none | 445 | 1000 evaluations 445 | 0 | 0.000 | 0.000 | 0.000 | - | - | - | 106.6 | +0% | ok | memo misses 934468 |
| LS long session | 2000 | B | 2447 | 1000 evaluations 2447 | 1000 | 2212 | 5.77 | 2.21 | 112276 | 7.9 | 7.9 | 8.8 | +450% | ok | memo misses 1137091 |
| LS long session | 2000 | A | 2203 | 1000 evaluations 2203 | 1000 | 1894 | 4.05 | 1.89 | 112276 | 12.8 | 12.8 | 18.8 | +395% | ok | memo misses 1137091 |
| LS long session | 2000 | A-fl | 1937 | 1000 evaluations 1937 | 1000 | 1692 | 3.60 | 1.69 | 112276 | 7.9 | 7.9 | 8.8 | +335% | ok | memo misses 1137091 |
| LS long session | 2000 | C1 | 1571 | 1000 evaluations 1571 | 1250 (1000 minor) | 1362 | 4.65 | 1.09 | 112276 | 7.4 | 7.4 | 7.6 | +253% | ok | memo misses 1137057 |
| LS long session | 2000 | C | 1746 | 1000 evaluations 1746 | 1250 (1000 minor) | 1476 | 4.52 | 1.18 | 112276 | 7.4 | 7.4 | 7.6 | +292% | ok | memo misses 1137057 |
| LS long session | 2000 | C0 | 1582 | 1000 evaluations 1582 | 1250 (1000 minor) | 1371 | 5.18 | 1.10 | 112276 | 7.4 | 7.4 | 7.6 | +256% | ok | memo misses 1137057 |
