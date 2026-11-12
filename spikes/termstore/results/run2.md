| case | size | store | phase | median | peak heap | check |
|---|---|---|---|---|---|---|
| R3 derivative | 10000 | tree | build | 0.792 ms | 2.1 MB | ok |
| R3 derivative | 10000 | tree | derivative | 1.59 ms | 2.1 MB | ok |
| R3 derivative | 10000 | hc | build | 0.655 ms | 1.5 MB | ok |
| R3 derivative | 10000 | hc | derivative | 1.52 ms | 1.5 MB | ok |
| R3 derivative | 10000 | hc+imm | build | 0.629 ms | 1.5 MB | ok |
| R3 derivative | 10000 | hc+imm | derivative | 1.41 ms | 1.5 MB | ok |
| R3 derivative | 100000 | tree | build | 7.18 ms | 20.9 MB | ok |
| R3 derivative | 100000 | tree | derivative | 19.64 ms | 20.9 MB | ok |
| R3 derivative | 100000 | hc | build | 7.42 ms | 11.8 MB | ok |
| R3 derivative | 100000 | hc | derivative | 17.35 ms | 11.8 MB | ok |
| R3 derivative | 100000 | hc+imm | build | 7.42 ms | 11.8 MB | ok |
| R3 derivative | 100000 | hc+imm | derivative | 16.89 ms | 11.8 MB | ok |
| R3 derivative | 300000 | tree | build | 21.48 ms | 62.6 MB | ok |
| R3 derivative | 300000 | tree | derivative | 61.07 ms | 62.6 MB | ok |
| R3 derivative | 300000 | hc | build | 22.31 ms | 43.0 MB | ok |
| R3 derivative | 300000 | hc | derivative | 65.30 ms | 43.0 MB | ok |
| R3 derivative | 300000 | hc+imm | build | 21.18 ms | 43.0 MB | ok |
| R3 derivative | 300000 | hc+imm | derivative | 61.48 ms | 43.0 MB | ok |
| R8 expand | 10 | tree | expand | 1.46 ms | 0.3 MB | ok |
| R8 expand | 10 | hc | expand | 0.973 ms | 0.1 MB | ok |
| R8 expand | 10 | hc+imm | expand | 0.938 ms | 0.1 MB | ok |
| R8 expand | 20 | tree | expand | 19.48 ms | 2.0 MB | ok |
| R8 expand | 20 | hc | expand | 12.98 ms | 1.1 MB | ok |
| R8 expand | 20 | hc+imm | expand | 12.57 ms | 1.1 MB | ok |
| R8 expand | 30 | tree | expand | 94.82 ms | 6.5 MB | ok |
| R8 expand | 30 | hc | expand | 62.44 ms | 4.2 MB | ok |
| R8 expand | 30 | hc+imm | expand | 59.33 ms | 4.2 MB | ok |
| R9 deep | 10000 | tree | build | 0.517 ms | 2.0 MB | ok |
| R9 deep | 10000 | tree | depth | 0.320 ms | 2.0 MB | ok |
| R9 deep | 10000 | tree | replace | 0.835 ms | 2.0 MB | ok |
| R9 deep | 10000 | tree | drop | 0.660 ms | 2.0 MB | ok |
| R9 deep | 10000 | hc | build | 0.558 ms | 1.7 MB | ok |
| R9 deep | 10000 | hc | depth | 0.240 ms | 1.7 MB | ok |
| R9 deep | 10000 | hc | replace | 0.782 ms | 1.7 MB | ok |
| R9 deep | 10000 | hc | drop | 0.000 ms | 1.7 MB | ok |
| R9 deep | 10000 | hc+imm | build | 0.546 ms | 1.7 MB | ok |
| R9 deep | 10000 | hc+imm | depth | 0.240 ms | 1.7 MB | ok |
| R9 deep | 10000 | hc+imm | replace | 0.791 ms | 1.7 MB | ok |
| R9 deep | 10000 | hc+imm | drop | 0.000 ms | 1.7 MB | ok |
| R9 deep | 100000 | tree | build | 5.66 ms | 19.1 MB | ok |
| R9 deep | 100000 | tree | depth | 3.76 ms | 19.1 MB | ok |
| R9 deep | 100000 | tree | replace | 9.52 ms | 19.1 MB | ok |
| R9 deep | 100000 | tree | drop | 11.80 ms | 19.1 MB | ok |
| R9 deep | 100000 | hc | build | 6.04 ms | 13.9 MB | ok |
| R9 deep | 100000 | hc | depth | 2.51 ms | 13.9 MB | ok |
| R9 deep | 100000 | hc | replace | 8.51 ms | 13.9 MB | ok |
| R9 deep | 100000 | hc | drop | 0.000 ms | 13.9 MB | ok |
| R9 deep | 100000 | hc+imm | build | 6.17 ms | 13.9 MB | ok |
| R9 deep | 100000 | hc+imm | depth | 2.49 ms | 13.9 MB | ok |
| R9 deep | 100000 | hc+imm | replace | 8.38 ms | 13.9 MB | ok |
| R9 deep | 100000 | hc+imm | drop | 0.000 ms | 13.9 MB | ok |
| R9 deep | 1000000 | tree | build | 59.13 ms | 185.2 MB | ok |
| R9 deep | 1000000 | tree | depth | 58.57 ms | 185.2 MB | ok |
| R9 deep | 1000000 | tree | replace | 119 ms | 185.2 MB | ok |
| R9 deep | 1000000 | tree | drop | 169 ms | 185.2 MB | ok |
| R9 deep | 1000000 | hc | build | 90.11 ms | 121.6 MB | ok |
| R9 deep | 1000000 | hc | depth | 24.55 ms | 121.6 MB | ok |
| R9 deep | 1000000 | hc | replace | 151 ms | 121.6 MB | ok |
| R9 deep | 1000000 | hc | drop | 0.000 ms | 121.6 MB | ok |
| R9 deep | 1000000 | hc+imm | build | 89.11 ms | 121.6 MB | ok |
| R9 deep | 1000000 | hc+imm | depth | 24.49 ms | 121.6 MB | ok |
| R9 deep | 1000000 | hc+imm | replace | 152 ms | 121.6 MB | ok |
| R9 deep | 1000000 | hc+imm | drop | 0.000 ms | 121.6 MB | ok |
| R10 replace | 10000 | tree | build | 0.533 ms | 1.4 MB | ok |
| R10 replace | 10000 | tree | replace | 0.709 ms | 1.4 MB | ok |
| R10 replace | 10000 | hc | build | 0.416 ms | 0.7 MB | ok |
| R10 replace | 10000 | hc | replace | 0.558 ms | 0.7 MB | ok |
| R10 replace | 10000 | hc+imm | build | 0.422 ms | 0.7 MB | ok |
| R10 replace | 10000 | hc+imm | replace | 0.566 ms | 0.7 MB | ok |
| R10 replace | 100000 | tree | build | 5.83 ms | 14.3 MB | ok |
| R10 replace | 100000 | tree | replace | 9.21 ms | 14.3 MB | ok |
| R10 replace | 100000 | hc | build | 5.08 ms | 5.9 MB | ok |
| R10 replace | 100000 | hc | replace | 7.49 ms | 5.9 MB | ok |
| R10 replace | 100000 | hc+imm | build | 4.98 ms | 5.9 MB | ok |
| R10 replace | 100000 | hc+imm | replace | 7.36 ms | 5.9 MB | ok |
| R10 replace | 1000000 | tree | build | 61.48 ms | 142.9 MB | ok |
| R10 replace | 1000000 | tree | replace | 126 ms | 142.9 MB | ok |
| R10 replace | 1000000 | hc | build | 60.08 ms | 47.2 MB | ok |
| R10 replace | 1000000 | hc | replace | 84.30 ms | 47.2 MB | ok |
| R10 replace | 1000000 | hc+imm | build | 59.07 ms | 47.2 MB | ok |
| R10 replace | 1000000 | hc+imm | replace | 83.95 ms | 47.2 MB | ok |
| R11 re-eval | 10000 | tree | build | 0.522 ms | 2.5 MB | ok |
| R11 re-eval | 10000 | tree | eval | 0.825 ms | 2.5 MB | ok |
| R11 re-eval | 10000 | tree | re-eval (unrelated change) | 0.854 ms | 2.5 MB | ok |
| R11 re-eval | 10000 | tree | re-eval (s3 changed) | 0.812 ms | 2.5 MB | ok |
| R11 re-eval | 10000 | hc | build | 0.495 ms | 0.4 MB | ok |
| R11 re-eval | 10000 | hc | eval | 0.688 ms | 0.4 MB | ok |
| R11 re-eval | 10000 | hc | re-eval (unrelated change) | 0.665 ms | 0.4 MB | ok |
| R11 re-eval | 10000 | hc | re-eval (s3 changed) | 0.660 ms | 0.4 MB | ok |
| R11 re-eval | 10000 | hc+imm | build | 0.452 ms | 0.4 MB | ok |
| R11 re-eval | 10000 | hc+imm | eval | 0.682 ms | 0.4 MB | ok |
| R11 re-eval | 10000 | hc+imm | re-eval (unrelated change) | 0.662 ms | 0.4 MB | ok |
| R11 re-eval | 10000 | hc+imm | re-eval (s3 changed) | 0.664 ms | 0.4 MB | ok |
| R11 re-eval | 10000 | hc+imm+stamps | build | 0.450 ms | 0.4 MB | ok |
| R11 re-eval | 10000 | hc+imm+stamps | eval | 0.550 ms | 0.4 MB | ok |
| R11 re-eval | 10000 | hc+imm+stamps | re-eval (unrelated change) | 0.000 ms | 0.4 MB | ok |
| R11 re-eval | 10000 | hc+imm+stamps | re-eval (s3 changed) | 0.163 ms | 0.4 MB | ok |
| R11 re-eval | 100000 | tree | build | 6.20 ms | 24.9 MB | ok |
| R11 re-eval | 100000 | tree | eval | 10.10 ms | 24.9 MB | ok |
| R11 re-eval | 100000 | tree | re-eval (unrelated change) | 9.89 ms | 24.9 MB | ok |
| R11 re-eval | 100000 | tree | re-eval (s3 changed) | 9.23 ms | 24.9 MB | ok |
| R11 re-eval | 100000 | hc | build | 5.69 ms | 3.0 MB | ok |
| R11 re-eval | 100000 | hc | eval | 7.48 ms | 3.0 MB | ok |
| R11 re-eval | 100000 | hc | re-eval (unrelated change) | 6.89 ms | 3.0 MB | ok |
| R11 re-eval | 100000 | hc | re-eval (s3 changed) | 6.82 ms | 3.0 MB | ok |
| R11 re-eval | 100000 | hc+imm | build | 5.64 ms | 3.0 MB | ok |
| R11 re-eval | 100000 | hc+imm | eval | 7.47 ms | 3.0 MB | ok |
| R11 re-eval | 100000 | hc+imm | re-eval (unrelated change) | 6.83 ms | 3.0 MB | ok |
| R11 re-eval | 100000 | hc+imm | re-eval (s3 changed) | 6.78 ms | 3.0 MB | ok |
| R11 re-eval | 100000 | hc+imm+stamps | build | 5.59 ms | 3.6 MB | ok |
| R11 re-eval | 100000 | hc+imm+stamps | eval | 5.75 ms | 3.6 MB | ok |
| R11 re-eval | 100000 | hc+imm+stamps | re-eval (unrelated change) | 0.000 ms | 3.6 MB | ok |
| R11 re-eval | 100000 | hc+imm+stamps | re-eval (s3 changed) | 1.53 ms | 3.6 MB | ok |
| R11 re-eval | 1000000 | tree | build | 61.03 ms | 248.7 MB | ok |
| R11 re-eval | 1000000 | tree | eval | 134 ms | 248.7 MB | ok |
| R11 re-eval | 1000000 | tree | re-eval (unrelated change) | 134 ms | 248.7 MB | ok |
| R11 re-eval | 1000000 | tree | re-eval (s3 changed) | 135 ms | 248.7 MB | ok |
| R11 re-eval | 1000000 | hc | build | 63.23 ms | 28.8 MB | ok |
| R11 re-eval | 1000000 | hc | eval | 93.84 ms | 28.8 MB | ok |
| R11 re-eval | 1000000 | hc | re-eval (unrelated change) | 83.34 ms | 28.8 MB | ok |
| R11 re-eval | 1000000 | hc | re-eval (s3 changed) | 83.37 ms | 28.8 MB | ok |
| R11 re-eval | 1000000 | hc+imm | build | 62.47 ms | 28.8 MB | ok |
| R11 re-eval | 1000000 | hc+imm | eval | 91.11 ms | 28.8 MB | ok |
| R11 re-eval | 1000000 | hc+imm | re-eval (unrelated change) | 81.63 ms | 28.8 MB | ok |
| R11 re-eval | 1000000 | hc+imm | re-eval (s3 changed) | 82.43 ms | 28.8 MB | ok |
| R11 re-eval | 1000000 | hc+imm+stamps | build | 62.76 ms | 34.7 MB | ok |
| R11 re-eval | 1000000 | hc+imm+stamps | eval | 67.09 ms | 34.7 MB | ok |
| R11 re-eval | 1000000 | hc+imm+stamps | re-eval (unrelated change) | 0.000 ms | 34.7 MB | ok |
| R11 re-eval | 1000000 | hc+imm+stamps | re-eval (s3 changed) | 20.21 ms | 34.7 MB | ok |
| S3 parts | 10000 | tree | build | 0.153 ms | 0.4 MB | ok |
| S3 parts | 10000 | tree | 10^4 replacements | 16.03 ms | 0.4 MB | ok |
| S3 parts | 10000 | hc | build | 0.371 ms | 44.4 MB | ok |
| S3 parts | 10000 | hc | 10^4 replacements | 21.45 ms | 44.4 MB | ok |
| S3 parts | 10000 | hc+imm | build | 0.091 ms | 43.2 MB | ok |
| S3 parts | 10000 | hc+imm | 10^4 replacements | 20.02 ms | 43.2 MB | ok |
| S3 parts | 100000 | tree | build | 0.448 ms | 2.5 MB | ok |
| S3 parts | 100000 | tree | 10^4 replacements | 17.25 ms | 2.5 MB | ok |
| S3 parts | 100000 | hc | build | 3.57 ms | 58.0 MB | ok |
| S3 parts | 100000 | hc | 10^4 replacements | 24.28 ms | 58.0 MB | ok |
| S3 parts | 100000 | hc+imm | build | 0.298 ms | 53.7 MB | ok |
| S3 parts | 100000 | hc+imm | 10^4 replacements | 22.49 ms | 53.7 MB | ok |
| S3 parts | 1000000 | tree | build | 6.12 ms | 24.1 MB | ok |
| S3 parts | 1000000 | tree | 10^4 replacements | 30.63 ms | 24.1 MB | ok |
| S3 parts | 1000000 | hc | build | 58.01 ms | 175.2 MB | ok |
| S3 parts | 1000000 | hc | 10^4 replacements | 52.65 ms | 175.2 MB | ok |
| S3 parts | 1000000 | hc+imm | build | 2.72 ms | 132.4 MB | ok |
| S3 parts | 1000000 | hc+imm | 10^4 replacements | 44.63 ms | 132.4 MB | ok |
| S6 transpose | 10000 | tree | build | 0.008 ms | 0.7 MB | ok |
| S6 transpose | 10000 | tree | transpose | 0.036 ms | 0.7 MB | ok |
| S6 transpose | 10000 | hc | build | 0.152 ms | 0.8 MB | ok |
| S6 transpose | 10000 | hc | transpose | 0.025 ms | 0.8 MB | ok |
| S6 transpose | 10000 | hc+imm | build | 0.025 ms | 0.2 MB | ok |
| S6 transpose | 10000 | hc+imm | transpose | 0.022 ms | 0.2 MB | ok |
| S6 transpose | 100000 | tree | build | 0.075 ms | 7.3 MB | ok |
| S6 transpose | 100000 | tree | transpose | 0.257 ms | 7.3 MB | ok |
| S6 transpose | 100000 | hc | build | 2.77 ms | 6.6 MB | ok |
| S6 transpose | 100000 | hc | transpose | 0.463 ms | 6.6 MB | ok |
| S6 transpose | 100000 | hc+imm | build | 0.243 ms | 1.7 MB | ok |
| S6 transpose | 100000 | hc+imm | transpose | 0.283 ms | 1.7 MB | ok |
| S6 transpose | 1000000 | tree | build | 5.77 ms | 72.2 MB | ok |
| S6 transpose | 1000000 | tree | transpose | 16.39 ms | 72.2 MB | ok |
| S6 transpose | 1000000 | hc | build | 58.73 ms | 56.3 MB | ok |
| S6 transpose | 1000000 | hc | transpose | 7.47 ms | 56.3 MB | ok |
| S6 transpose | 1000000 | hc+imm | build | 3.03 ms | 12.3 MB | ok |
| S6 transpose | 1000000 | hc+imm | transpose | 2.92 ms | 12.3 MB | ok |
| S8 alloc | 100000 | tree | build | 8.26 ms | 25.0 MB | ok |
| S8 alloc | 100000 | hc | build | 18.70 ms | 25.7 MB | ok |
| S8 alloc | 100000 | hc+imm | build | 12.30 ms | 16.0 MB | ok |
| S8 alloc | 1000000 | tree | build | 116 ms | 241.6 MB | ok |
| S8 alloc | 1000000 | hc | build | 281 ms | 205.5 MB | ok |
| S8 alloc | 1000000 | hc+imm | build | 223 ms | 138.4 MB | ok |
| S8 alloc | 10000000 | tree | build | 1319 ms | 2616.9 MB | ok |
| S8 alloc | 10000000 | hc | build | 6402 ms | 2214.6 MB | ok |
| S8 alloc | 10000000 | hc+imm | build | 4476 ms | 2046.8 MB | ok |
| EQ same/hash | 10000 | tree | 100 x same | 6.22 ms | 1.8 MB | ok |
| EQ same/hash | 10000 | tree | 100 x hash | 4.13 ms | 1.8 MB | ok |
| EQ same/hash | 10000 | hc | 100 x same | 0.000 ms | 0.4 MB | ok |
| EQ same/hash | 10000 | hc | 100 x hash | 0.000 ms | 0.4 MB | ok |
| EQ same/hash | 10000 | hc+imm | 100 x same | 0.000 ms | 0.4 MB | ok |
| EQ same/hash | 10000 | hc+imm | 100 x hash | 0.000 ms | 0.4 MB | ok |
| EQ same/hash | 100000 | tree | 100 x same | 139 ms | 17.6 MB | ok |
| EQ same/hash | 100000 | tree | 100 x hash | 87.55 ms | 17.6 MB | ok |
| EQ same/hash | 100000 | hc | 100 x same | 0.000 ms | 3.0 MB | ok |
| EQ same/hash | 100000 | hc | 100 x hash | 0.000 ms | 3.0 MB | ok |
| EQ same/hash | 100000 | hc+imm | 100 x same | 0.000 ms | 3.0 MB | ok |
| EQ same/hash | 100000 | hc+imm | 100 x hash | 0.000 ms | 3.0 MB | ok |
| EQ same/hash | 1000000 | tree | 100 x same | 6783 ms | 175.4 MB | ok |
| EQ same/hash | 1000000 | tree | 100 x hash | 3822 ms | 175.4 MB | ok |
| EQ same/hash | 1000000 | hc | 100 x same | 0.000 ms | 23.6 MB | ok |
| EQ same/hash | 1000000 | hc | 100 x hash | 0.000 ms | 23.6 MB | ok |
| EQ same/hash | 1000000 | hc+imm | 100 x same | 0.000 ms | 23.6 MB | ok |
| EQ same/hash | 1000000 | hc+imm | 100 x hash | 0.000 ms | 23.6 MB | ok |
