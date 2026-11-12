| case | size | store | phase | median | peak heap | check |
|---|---|---|---|---|---|---|
| R3 derivative | 10000 | tree | build | 0.718 ms | 2.1 MB | ok |
| R3 derivative | 10000 | tree | derivative | 1.54 ms | 2.1 MB | ok |
| R3 derivative | 10000 | hc | build | 0.665 ms | 1.5 MB | ok |
| R3 derivative | 10000 | hc | derivative | 1.48 ms | 1.5 MB | ok |
| R3 derivative | 100000 | tree | build | 7.27 ms | 20.9 MB | ok |
| R3 derivative | 100000 | tree | derivative | 19.92 ms | 20.9 MB | ok |
| R3 derivative | 100000 | hc | build | 7.50 ms | 11.8 MB | ok |
| R3 derivative | 100000 | hc | derivative | 18.08 ms | 11.8 MB | ok |
| R3 derivative | 300000 | tree | build | 21.58 ms | 62.6 MB | ok |
| R3 derivative | 300000 | tree | derivative | 61.71 ms | 62.6 MB | ok |
| R3 derivative | 300000 | hc | build | 22.80 ms | 43.0 MB | ok |
| R3 derivative | 300000 | hc | derivative | 64.23 ms | 43.0 MB | ok |
| R8 expand | 10 | tree | expand | 1.48 ms | 0.3 MB | ok |
| R8 expand | 10 | hc | expand | 0.966 ms | 0.1 MB | ok |
| R8 expand | 20 | tree | expand | 19.34 ms | 2.0 MB | ok |
| R8 expand | 20 | hc | expand | 12.78 ms | 1.1 MB | ok |
| R8 expand | 30 | tree | expand | 96.07 ms | 6.5 MB | ok |
| R8 expand | 30 | hc | expand | 62.20 ms | 4.2 MB | ok |
| R9 deep | 10000 | tree | build | 0.508 ms | 2.0 MB | ok |
| R9 deep | 10000 | tree | depth | 0.326 ms | 2.0 MB | ok |
| R9 deep | 10000 | tree | replace | 0.873 ms | 2.0 MB | ok |
| R9 deep | 10000 | tree | drop | 0.677 ms | 2.0 MB | ok |
| R9 deep | 10000 | hc | build | 0.553 ms | 1.7 MB | ok |
| R9 deep | 10000 | hc | depth | 0.243 ms | 1.7 MB | ok |
| R9 deep | 10000 | hc | replace | 0.848 ms | 1.7 MB | ok |
| R9 deep | 10000 | hc | drop | 0.000 ms | 1.7 MB | ok |
| R9 deep | 100000 | tree | build | 5.76 ms | 19.1 MB | ok |
| R9 deep | 100000 | tree | depth | 3.69 ms | 19.1 MB | ok |
| R9 deep | 100000 | tree | replace | 9.84 ms | 19.1 MB | ok |
| R9 deep | 100000 | tree | drop | 14.94 ms | 19.1 MB | ok |
| R9 deep | 100000 | hc | build | 6.32 ms | 13.9 MB | ok |
| R9 deep | 100000 | hc | depth | 2.41 ms | 13.9 MB | ok |
| R9 deep | 100000 | hc | replace | 8.98 ms | 13.9 MB | ok |
| R9 deep | 100000 | hc | drop | 0.000 ms | 13.9 MB | ok |
| R9 deep | 1000000 | tree | build | 60.74 ms | 185.2 MB | ok |
| R9 deep | 1000000 | tree | depth | 60.88 ms | 185.2 MB | ok |
| R9 deep | 1000000 | tree | replace | 123 ms | 185.2 MB | ok |
| R9 deep | 1000000 | tree | drop | 209 ms | 185.2 MB | ok |
| R9 deep | 1000000 | hc | build | 96.14 ms | 121.6 MB | ok |
| R9 deep | 1000000 | hc | depth | 24.58 ms | 121.6 MB | ok |
| R9 deep | 1000000 | hc | replace | 154 ms | 121.6 MB | ok |
| R9 deep | 1000000 | hc | drop | 0.000 ms | 121.6 MB | ok |
| R10 replace | 10000 | tree | build | 0.528 ms | 1.4 MB | ok |
| R10 replace | 10000 | tree | replace | 0.718 ms | 1.4 MB | ok |
| R10 replace | 10000 | hc | build | 0.505 ms | 0.7 MB | ok |
| R10 replace | 10000 | hc | replace | 0.624 ms | 0.7 MB | ok |
| R10 replace | 100000 | tree | build | 5.86 ms | 14.3 MB | ok |
| R10 replace | 100000 | tree | replace | 9.78 ms | 14.3 MB | ok |
| R10 replace | 100000 | hc | build | 5.34 ms | 5.9 MB | ok |
| R10 replace | 100000 | hc | replace | 7.43 ms | 5.9 MB | ok |
| R10 replace | 1000000 | tree | build | 60.55 ms | 142.9 MB | ok |
| R10 replace | 1000000 | tree | replace | 128 ms | 142.9 MB | ok |
| R10 replace | 1000000 | hc | build | 61.29 ms | 47.2 MB | ok |
| R10 replace | 1000000 | hc | replace | 85.46 ms | 47.2 MB | ok |
| R11 re-eval | 10000 | tree | build | 0.521 ms | 2.5 MB | ok |
| R11 re-eval | 10000 | tree | eval | 0.856 ms | 2.5 MB | ok |
| R11 re-eval | 10000 | tree | re-eval (unrelated change) | 0.830 ms | 2.5 MB | ok |
| R11 re-eval | 10000 | tree | re-eval (s3 changed) | 0.796 ms | 2.5 MB | ok |
| R11 re-eval | 10000 | hc | build | 0.476 ms | 0.4 MB | ok |
| R11 re-eval | 10000 | hc | eval | 0.681 ms | 0.4 MB | ok |
| R11 re-eval | 10000 | hc | re-eval (unrelated change) | 0.659 ms | 0.4 MB | ok |
| R11 re-eval | 10000 | hc | re-eval (s3 changed) | 0.655 ms | 0.4 MB | ok |
| R11 re-eval | 10000 | hc+stamps | build | 0.458 ms | 0.4 MB | ok |
| R11 re-eval | 10000 | hc+stamps | eval | 0.564 ms | 0.4 MB | ok |
| R11 re-eval | 10000 | hc+stamps | re-eval (unrelated change) | 0.000 ms | 0.4 MB | ok |
| R11 re-eval | 10000 | hc+stamps | re-eval (s3 changed) | 0.159 ms | 0.4 MB | ok |
| R11 re-eval | 100000 | tree | build | 5.88 ms | 24.9 MB | ok |
| R11 re-eval | 100000 | tree | eval | 10.29 ms | 24.9 MB | ok |
| R11 re-eval | 100000 | tree | re-eval (unrelated change) | 9.48 ms | 24.9 MB | ok |
| R11 re-eval | 100000 | tree | re-eval (s3 changed) | 9.21 ms | 24.9 MB | ok |
| R11 re-eval | 100000 | hc | build | 5.71 ms | 3.0 MB | ok |
| R11 re-eval | 100000 | hc | eval | 7.46 ms | 3.0 MB | ok |
| R11 re-eval | 100000 | hc | re-eval (unrelated change) | 6.72 ms | 3.0 MB | ok |
| R11 re-eval | 100000 | hc | re-eval (s3 changed) | 6.73 ms | 3.0 MB | ok |
| R11 re-eval | 100000 | hc+stamps | build | 5.92 ms | 3.6 MB | ok |
| R11 re-eval | 100000 | hc+stamps | eval | 5.63 ms | 3.6 MB | ok |
| R11 re-eval | 100000 | hc+stamps | re-eval (unrelated change) | 0.000 ms | 3.6 MB | ok |
| R11 re-eval | 100000 | hc+stamps | re-eval (s3 changed) | 1.51 ms | 3.6 MB | ok |
| R11 re-eval | 1000000 | tree | build | 60.76 ms | 248.7 MB | ok |
| R11 re-eval | 1000000 | tree | eval | 134 ms | 248.7 MB | ok |
| R11 re-eval | 1000000 | tree | re-eval (unrelated change) | 134 ms | 248.7 MB | ok |
| R11 re-eval | 1000000 | tree | re-eval (s3 changed) | 135 ms | 248.7 MB | ok |
| R11 re-eval | 1000000 | hc | build | 65.33 ms | 28.8 MB | ok |
| R11 re-eval | 1000000 | hc | eval | 94.42 ms | 28.8 MB | ok |
| R11 re-eval | 1000000 | hc | re-eval (unrelated change) | 81.89 ms | 28.8 MB | ok |
| R11 re-eval | 1000000 | hc | re-eval (s3 changed) | 81.07 ms | 28.8 MB | ok |
| R11 re-eval | 1000000 | hc+stamps | build | 62.76 ms | 34.7 MB | ok |
| R11 re-eval | 1000000 | hc+stamps | eval | 66.95 ms | 34.7 MB | ok |
| R11 re-eval | 1000000 | hc+stamps | re-eval (unrelated change) | 0.000 ms | 34.7 MB | ok |
| R11 re-eval | 1000000 | hc+stamps | re-eval (s3 changed) | 20.63 ms | 34.7 MB | ok |
| S3 parts | 10000 | tree | build | 0.137 ms | 0.4 MB | ok |
| S3 parts | 10000 | tree | 10^4 replacements | 15.73 ms | 0.4 MB | ok |
| S3 parts | 10000 | hc | build | 0.372 ms | 44.4 MB | ok |
| S3 parts | 10000 | hc | 10^4 replacements | 20.66 ms | 44.4 MB | ok |
| S3 parts | 100000 | tree | build | 0.409 ms | 2.5 MB | ok |
| S3 parts | 100000 | tree | 10^4 replacements | 17.32 ms | 2.5 MB | ok |
| S3 parts | 100000 | hc | build | 3.37 ms | 58.0 MB | ok |
| S3 parts | 100000 | hc | 10^4 replacements | 23.98 ms | 58.0 MB | ok |
| S3 parts | 1000000 | tree | build | 6.15 ms | 24.1 MB | ok |
| S3 parts | 1000000 | tree | 10^4 replacements | 30.87 ms | 24.1 MB | ok |
| S3 parts | 1000000 | hc | build | 58.87 ms | 175.2 MB | ok |
| S3 parts | 1000000 | hc | 10^4 replacements | 53.17 ms | 175.2 MB | ok |
| S6 transpose | 10000 | tree | build | 0.007 ms | 0.7 MB | ok |
| S6 transpose | 10000 | tree | transpose | 0.031 ms | 0.7 MB | ok |
| S6 transpose | 10000 | hc | build | 0.149 ms | 0.8 MB | ok |
| S6 transpose | 10000 | hc | transpose | 0.030 ms | 0.8 MB | ok |
| S6 transpose | 100000 | tree | build | 0.355 ms | 7.3 MB | ok |
| S6 transpose | 100000 | tree | transpose | 1.23 ms | 7.3 MB | ok |
| S6 transpose | 100000 | hc | build | 2.80 ms | 6.6 MB | ok |
| S6 transpose | 100000 | hc | transpose | 0.513 ms | 6.6 MB | ok |
| S6 transpose | 1000000 | tree | build | 5.85 ms | 72.2 MB | ok |
| S6 transpose | 1000000 | tree | transpose | 17.55 ms | 72.2 MB | ok |
| S6 transpose | 1000000 | hc | build | 60.77 ms | 56.3 MB | ok |
| S6 transpose | 1000000 | hc | transpose | 7.11 ms | 56.3 MB | ok |
| S8 alloc | 100000 | tree | build | 11.27 ms | 25.0 MB | ok |
| S8 alloc | 100000 | hc | build | 20.13 ms | 25.7 MB | ok |
| S8 alloc | 1000000 | tree | build | 125 ms | 241.6 MB | ok |
| S8 alloc | 1000000 | hc | build | 304 ms | 205.5 MB | ok |
| S8 alloc | 10000000 | tree | build | 1303 ms | 2616.9 MB | ok |
| S8 alloc | 10000000 | hc | build | 6580 ms | 2214.6 MB | ok |
| EQ same/hash | 10000 | tree | 100 x same | 6.40 ms | 1.8 MB | ok |
| EQ same/hash | 10000 | tree | 100 x hash | 4.32 ms | 1.8 MB | ok |
| EQ same/hash | 10000 | hc | 100 x same | 0.000 ms | 0.4 MB | ok |
| EQ same/hash | 10000 | hc | 100 x hash | 0.000 ms | 0.4 MB | ok |
| EQ same/hash | 100000 | tree | 100 x same | 159 ms | 17.6 MB | ok |
| EQ same/hash | 100000 | tree | 100 x hash | 88.20 ms | 17.6 MB | ok |
| EQ same/hash | 100000 | hc | 100 x same | 0.000 ms | 3.0 MB | ok |
| EQ same/hash | 100000 | hc | 100 x hash | 0.000 ms | 3.0 MB | ok |
| EQ same/hash | 1000000 | tree | 100 x same | 6869 ms | 175.4 MB | ok |
| EQ same/hash | 1000000 | tree | 100 x hash | 3786 ms | 175.4 MB | ok |
| EQ same/hash | 1000000 | hc | 100 x same | 0.000 ms | 23.6 MB | ok |
| EQ same/hash | 1000000 | hc | 100 x hash | 0.000 ms | 23.6 MB | ok |
