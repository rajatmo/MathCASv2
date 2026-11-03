| case | size | store | total ms | phases (ms) | GCs | pause total | pause max | pause mean | live nodes after last GC | reserved after last GC MB | max reserved after GC MB | peak heap MB | overhead | check | notes |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| LS long session | 2000 | hc+imm+stamps | 311 | 1000 evaluations 311 | 0 | 0.000 | 0.000 | 0.000 | - | - | - | 106.8 | -4% | ok | memo misses 734951 |
| LS long session | 2000 | none | 324 | 1000 evaluations 324 | 0 | 0.000 | 0.000 | 0.000 | - | - | - | 106.6 | +0% | ok | memo misses 934468 |
| LS long session | 2000 | A-fl | 264 | 1000 evaluations 264 | 12 | 29.18 | 3.60 | 2.43 | 110011 | 16.9 | 16.9 | 18.5 | -18% | ok | memo misses 1006783 |
| LS long session | 2000 | A | 268 | 1000 evaluations 268 | 12 | 31.82 | 4.27 | 2.65 | 110011 | 20.2 | 20.2 | 26.2 | -17% | ok | memo misses 1006783 |
| LS long session | 2000 | B | 244 | 1000 evaluations 244 | 12 | 28.07 | 3.88 | 2.34 | 110011 | 15.8 | 15.8 | 17.5 | -24% | ok | memo misses 1006783 |
| LS long session | 2000 | C | 237 | 1000 evaluations 237 | 7 (6 minor) | 10.32 | 2.69 | 1.47 | 110362 | 18.9 | 18.9 | 26.7 | -27% | ok | memo misses 980531 |
| LS long session | 2000 | C1 | 211 | 1000 evaluations 211 | 51 (50 minor) | 8.75 | 2.68 | 0.172 | 111144 | 12.7 | 12.7 | 12.8 | -35% | ok | memo misses 1054954 |
| LS long session | 2000 | C0 | 216 | 1000 evaluations 216 | 1001 (1000 minor) | 8.72 | 2.60 | 0.009 | 112347 | 7.4 | 7.4 | 7.6 | -33% | ok | memo misses 1128419 |
| LS long session | 10000 | hc+imm+stamps | 2011 | 1000 evaluations 2011 | 0 | 0.000 | 0.000 | 0.000 | - | - | - | 461.5 | +1% | ok | memo misses 3439726 |
| LS long session | 10000 | none | 1999 | 1000 evaluations 1999 | 0 | 0.000 | 0.000 | 0.000 | - | - | - | 461.2 | +0% | ok | memo misses 4299478 |
| LS long session | 10000 | A | 1374 | 1000 evaluations 1374 | 29 | 203 | 19.04 | 6.99 | 530644 | 67.2 | 67.3 | 79.7 | -31% | ok | memo misses 4699225 |
| LS long session | 10000 | B | 1391 | 1000 evaluations 1391 | 29 | 215 | 18.43 | 7.40 | 530644 | 61.5 | 61.6 | 68.1 | -30% | ok | memo misses 4699225 |
| LS long session | 10000 | C | 1168 | 1000 evaluations 1168 | 32 (29 minor) | 60.66 | 12.36 | 1.90 | 523200 | 64.3 | 64.3 | 67.8 | -42% | ok | memo misses 4707648 |
| LS long session | 10000 | C1 | 1113 | 1000 evaluations 1113 | 212 (209 minor) | 66.15 | 15.75 | 0.312 | 536375 | 52.0 | 52.0 | 52.8 | -44% | ok | memo misses 5005850 |
| LS long session | 10000 | A-fl | 1396 | 1000 evaluations 1396 | 29 | 207 | 19.17 | 7.12 | 530644 | 66.8 | 66.8 | 72.9 | -30% | ok | memo misses 4699225 |
| LS long session | 10000 | C0 | 1078 | 1000 evaluations 1078 | 1003 (1000 minor) | 51.86 | 10.32 | 0.052 | 537199 | 51.8 | 51.8 | 52.6 | -46% | ok | memo misses 5158630 |
| LS long session | 50000 | hc+imm+stamps | 12121 | 1000 evaluations 12121 | 0 | 0.000 | 0.000 | 0.000 | - | - | - | 1869.2 | -0% | ok | memo misses 16100808 |
| LS long session | 50000 | A | 10988 | 1000 evaluations 10988 | 43 | 1437 | 114 | 33.43 | 2489152 | 406.7 | 406.7 | 480.4 | -10% | ok | memo misses 21775620 |
| LS long session | 50000 | A-fl | 12001 | 1000 evaluations 12001 | 43 | 1386 | 141 | 32.24 | 2489152 | 444.9 | 444.9 | 452.3 | -1% | ok | memo misses 21775620 |
| LS long session | 50000 | B | 11276 | 1000 evaluations 11276 | 43 | 1587 | 131 | 36.90 | 2489152 | 318.1 | 318.7 | 326.0 | -7% | ok | memo misses 21775620 |
| LS long session | 50000 | C | 7968 | 1000 evaluations 7968 | 147 (142 minor) | 325 | 65.47 | 2.21 | 2493983 | 180.5 | 180.5 | 183.9 | -34% | ok | memo misses 22626412 |
| LS long session | 50000 | C1 | 6238 | 1000 evaluations 6238 | 1005 (1000 minor) | 285 | 47.65 | 0.283 | 2501948 | 208.8 | 208.8 | 211.6 | -49% | ok | memo misses 24045799 |
| LS long session | 50000 | C0 | 5958 | 1000 evaluations 5958 | 1005 (1000 minor) | 277 | 46.87 | 0.275 | 2501948 | 208.8 | 208.8 | 211.6 | -51% | ok | memo misses 24045799 |

trace (MB reserved after collection): LS long session 2000 A: 10.9 11.9 12.0 16.3 16.4 16.5 16.6 16.7 16.7 16.9 18.3 20.2

trace (MB reserved after collection): LS long session 2000 A-fl: 8.1 9.0 8.9 14.2 14.1 14.1 14.1 14.0 13.9 13.8 15.3 16.9

trace (MB reserved after collection): LS long session 2000 B: 7.1 7.9 7.9 13.1 13.1 13.1 13.0 12.9 12.9 12.8 14.2 15.8

trace (MB reserved after collection): LS long session 2000 C: 11.8 13.1 14.1 17.7 18.9 18.9 18.9

trace (MB reserved after collection): LS long session 2000 C1: 1.9 1.9 3.2 3.3 3.3 3.7 3.7 3.8 3.8 6.4 6.4 6.4 6.4 6.4 6.7 6.7 6.7 7.5 7.5 7.5 7.5 7.5 7.5 7.5 8.5 12.7

trace (MB reserved after collection): LS long session 2000 C0: 0.2 0.8 0.9 1.7 1.9 2.0 3.3 3.3 3.3 3.7 3.7 4.0 6.6 6.6 6.6 6.6 6.6 7.4 7.4 7.4 7.4

| LS long session | 50000 | none | 12149 | 1000 evaluations 12149 | 0 | 0.000 | 0.000 | 0.000 | - | - | - | 1868.9 | +0% | ok | memo misses 19682589 |
trace (MB reserved after collection): LS long session 10000 A: 10.6 11.7 11.9 16.2 16.4 17.8 17.9 18.0 17.9 19.9 19.9 28.7 29.2 29.5 32.0 32.2 32.0 32.3 35.4 36.3 54.0 54.4 54.3 60.0 59.8 60.5 60.0 67.3 67.2

trace (MB reserved after collection): LS long session 10000 B: 6.8 7.7 7.7 12.9 12.8 14.1 14.0 13.9 13.8 15.4 15.4 25.8 25.8 25.7 28.2 28.0 27.8 27.6 30.8 30.7 51.6 51.6 51.4 56.4 56.2 55.8 55.3 61.6 61.5

trace (MB reserved after collection): LS long session 10000 C: 11.5 13.2 15.1 18.5 19.0 19.0 31.6 31.6 31.6 31.6 31.6 31.6 32.7 32.7 32.7 32.7 32.7 32.7 36.9 36.9 36.9 36.9 36.9 36.9 56.8 56.8 62.0 64.3 64.3 64.3 64.3 64.3
trace (MB reserved after collection): LS long session 10000 A-fl: 8.1 9.0 9.0 14.2 14.1 15.4 15.3 15.2 15.1 16.7 16.7 27.2 27.6 27.6 30.1 29.9 29.6 29.5 33.1 33.4 55.3 55.2 55.1 60.1 59.9 59.4 60.0 66.8 66.8


trace (MB reserved after collection): LS long session 10000 C1: 1.9 3.7 7.1 7.4 13.2 14.5 14.5 14.5 14.5 26.2 26.2 26.2 28.8 28.8 28.8 28.8 28.8 28.8 28.8 31.1 52.0 52.0

trace (MB reserved after collection): LS long session 10000 C0: 0.5 3.6 6.5 7.2 13.0 14.3 14.3 14.3 15.5 26.0 26.0 26.0 28.6 28.6 28.6 28.6 28.6 28.6 30.9 30.9 51.8
trace (MB reserved after collection): LS long session 50000 A: 15.9 15.9 16.6 19.7 19.9 29.3 32.5 35.6 36.1 54.9 60.8 67.7 104.5 105.3 117.2 130.7 132.6 206.8 230.3 256.6 261.1 406.7

trace (MB reserved after collection): LS long session 50000 A-fl: 14.3 14.2 14.1 16.9 16.9 27.8 30.2 33.5 33.7 55.4 60.3 67.5 111.3 110.8 120.8 135.1 151.7 222.0 242.1 268.1 269.8 444.9

trace (MB reserved after collection): LS long session 50000 B: 10.0 9.8 15.1 17.9 17.9 19.9 33.0 35.7 35.7 39.5 65.8 71.4 79.6 79.1 131.8 143.0 159.5 158.6 264.1 286.0 285.6 318.1


trace (MB reserved after collection): LS long session 50000 C: 11.7 25.2 39.5 43.7 51.6 77.9 77.9 90.8 90.8 90.8 102.1 102.1 154.6 154.6 154.6 180.5 180.5 180.5 180.5 180.5 180.5

trace (MB reserved after collection): LS long session 50000 C1: 3.3 14.7 29.2 31.5 52.4 58.0 58.0 62.6 104.6 104.6 104.6 115.8 115.8 115.8 125.0 125.0 208.8 208.8 208.8 208.8 208.8

trace (MB reserved after collection): LS long session 50000 C0: 3.3 14.7 29.2 31.5 52.4 58.0 58.0 62.6 104.6 104.6 104.6 115.8 115.8 115.8 125.0 125.0 208.8 208.8 208.8 208.8 208.8
