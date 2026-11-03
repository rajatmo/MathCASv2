|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| case | size | store | total ms | phases (ms) | GCs | pause total | pause max | pause mean | live nodes after last GC | reserved after last GC MB | max reserved after GC MB | peak heap MB | overhead | check | notes |
| S3 parts | 10000 | hc+imm+stamps | 21.35 | build 0.128, 10^4 replacements 21.21 | 0 | 0.000 | 0.000 | 0.000 | - | - | - | 43.2 | +0% | ok |  |
| S3 parts | 10000 | none | 21.31 | build 0.121, 10^4 replacements 21.17 | 0 | 0.000 | 0.000 | 0.000 | - | - | - | 43.2 | +0% | ok |  |
| S3 parts | 10000 | A-fl | 13.63 | build 0.107, 10^4 replacements 13.53 | 9 | 0.344 | 0.057 | 0.038 | 1001 | 5.4 | 5.4 | 5.5 | -36% | ok |  |
| S3 parts | 10000 | B | 13.58 | build 0.094, 10^4 replacements 13.49 | 9 | 0.405 | 0.057 | 0.045 | 1001 | 4.4 | 4.4 | 5.4 | -36% | ok |  |
| S3 parts | 10000 | A | 17.18 | build 0.105, 10^4 replacements 17.07 | 9 | 1.13 | 0.146 | 0.126 | 1001 | 4.5 | 4.5 | 9.7 | -19% | ok |  |
| S3 parts | 10000 | C | 18.92 | build 0.105, 10^4 replacements 18.81 | 4 (4 minor) | 0.668 | 0.181 | 0.167 | 3609 | 4.8 | 4.8 | 10.8 | -11% | ok |  |
| S3 parts | 10000 | C0 | 26.71 | build 0.121, 10^4 replacements 26.59 | 10010 (10001 minor) | 13.00 | 0.068 | 0.001 | 2625 | 4.5 | 5.4 | 5.9 | +25% | ok |  |
| S3 parts | 10000 | C1 | 13.08 | build 0.108, 10^4 replacements 12.97 | 39 (39 minor) | 0.496 | 0.063 | 0.013 | 9686 | 3.2 | 3.2 | 3.3 | -39% | ok |  |
| S3 parts | 100000 | hc+imm+stamps | 24.20 | build 0.434, 10^4 replacements 23.76 | 0 | 0.000 | 0.000 | 0.000 | - | - | - | 53.7 | +0% | ok |  |
| S3 parts | 100000 | none | 24.10 | build 0.387, 10^4 replacements 23.72 | 0 | 0.000 | 0.000 | 0.000 | - | - | - | 53.7 | +0% | ok |  |
| S3 parts | 100000 | A | 20.12 | build 0.347, 10^4 replacements 19.77 | 10 | 2.13 | 0.285 | 0.213 | 1001 | 5.2 | 5.2 | 11.7 | -17% | ok |  |
| S3 parts | 100000 | A-fl | 16.04 | build 0.356, 10^4 replacements 15.68 | 10 | 0.895 | 0.170 | 0.089 | 1001 | 6.7 | 6.7 | 6.8 | -33% | ok |  |
| S3 parts | 100000 | B | 16.71 | build 0.381, 10^4 replacements 16.34 | 10 | 1.59 | 0.222 | 0.159 | 1001 | 5.2 | 5.2 | 6.7 | -31% | ok |  |
| S3 parts | 100000 | C | 23.30 | build 0.371, 10^4 replacements 22.93 | 5 (5 minor) | 1.23 | 0.315 | 0.246 | 4383 | 8.1 | 8.1 | 14.5 | -3% | ok |  |
| S3 parts | 100000 | C1 | 16.54 | build 0.355, 10^4 replacements 16.19 | 43 (42 minor) | 1.66 | 0.185 | 0.039 | 1216 | 5.6 | 7.2 | 7.2 | -31% | ok |  |
| S3 parts | 100000 | C0 | 30.13 | build 0.393, 10^4 replacements 29.71 | 10011 (10001 minor) | 14.95 | 0.214 | 0.001 | 2397 | 5.2 | 6.7 | 6.8 | +25% | ok |  |
| S3 parts | 1000000 | hc+imm+stamps | 50.59 | build 3.19, 10^4 replacements 47.57 | 0 | 0.000 | 0.000 | 0.000 | - | - | - | 132.4 | -1% | ok |  |
| S3 parts | 1000000 | A | 55.83 | build 3.19, 10^4 replacements 52.66 | 20 | 20.84 | 1.24 | 1.04 | 1001 | 12.3 | 12.3 | 28.7 | +9% | ok |  |
| S3 parts | 1000000 | none | 51.02 | build 3.09, 10^4 replacements 47.94 | 0 | 0.000 | 0.000 | 0.000 | - | - | - | 132.4 | +0% | ok |  |
| S3 parts | 1000000 | B | 46.31 | build 3.06, 10^4 replacements 43.25 | 20 | 19.64 | 1.14 | 0.982 | 1001 | 8.3 | 8.3 | 8.3 | -9% | ok |  |
| S3 parts | 1000000 | C | 40.56 | build 3.15, 10^4 replacements 37.41 | 15 (10 minor) | 11.94 | 1.22 | 0.796 | 1001 | 16.5 | 16.6 | 16.6 | -20% | ok |  |
| S3 parts | 1000000 | A-fl | 35.88 | build 3.17, 10^4 replacements 32.71 | 20 | 8.85 | 0.628 | 0.443 | 1001 | 8.3 | 8.3 | 8.4 | -30% | ok |  |
| S3 parts | 1000000 | C1 | 45.98 | build 3.18, 10^4 replacements 42.71 | 86 (77 minor) | 19.40 | 1.13 | 0.226 | 1379 | 16.5 | 16.5 | 16.6 | -10% | ok |  |
| S3 parts | 1000000 | C0 | 71.45 | build 3.18, 10^4 replacements 67.58 | 10021 (10001 minor) | 43.94 | 1.54 | 0.004 | 1163 | 8.3 | 8.3 | 9.8 | +40% | ok |  |
