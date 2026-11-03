| case | size | store | split | threads | median ms | speedup | efficiency | vs seq | splits | redos | check |
|---|---|---|---|---|---|---|---|---|---|---|---|
| M8 intern | 1000000 | A:locked | - | 1 | 41.66 | 1.00 | 1.00 | 0.86 | 0 | 0 | ok 27 MB reserved |
| M8 intern | 1000000 | A:locked | - | 2 | 56.03 | 0.74 | 0.37 | 1.16 | 0 | 0 | ok 27 MB reserved |
| M8 intern | 1000000 | A:locked | - | 4 | 98.61 | 0.42 | 0.11 | 2.04 | 0 | 0 | ok 27 MB reserved |
| M8 intern | 1000000 | A:locked | - | 8 | 254.15 | 0.16 | 0.02 | 5.27 | 0 | 0 | ok 28 MB reserved |
| M8 intern | 1000000 | A:locked | - | 16 | 439.80 | 0.09 | 0.01 | 9.12 | 0 | 0 | ok 28 MB reserved |
| M8 intern | 1000000 | B:sharded | - | 1 | 40.33 | 1.00 | 1.00 | 0.84 | 0 | 0 | ok 27 MB reserved |
| M8 intern | 1000000 | B:sharded | - | 2 | 32.12 | 1.26 | 0.63 | 0.67 | 0 | 0 | ok 27 MB reserved |
| M8 intern | 1000000 | B:sharded | - | 4 | 21.68 | 1.86 | 0.47 | 0.45 | 0 | 0 | ok 27 MB reserved |
| M8 intern | 1000000 | B:sharded | - | 8 | 15.58 | 2.59 | 0.32 | 0.32 | 0 | 0 | ok 28 MB reserved |
| M8 intern | 1000000 | B:sharded | - | 16 | 12.81 | 3.15 | 0.20 | 0.27 | 0 | 0 | ok 28 MB reserved |
| M8 intern | 1000000 | C:optimistic | - | 1 | 48.10 | 1.00 | 1.00 | 1.00 | 0 | 0 | ok 27 MB reserved |
| M8 intern | 1000000 | C:optimistic | - | 2 | 34.91 | 1.38 | 0.69 | 0.72 | 0 | 0 | ok 27 MB reserved |
| M8 intern | 1000000 | C:optimistic | - | 4 | 23.74 | 2.03 | 0.51 | 0.49 | 0 | 0 | ok 28 MB reserved |
| M8 intern | 1000000 | C:optimistic | - | 8 | 15.62 | 3.08 | 0.38 | 0.32 | 0 | 0 | ok 28 MB reserved |
| M8 intern | 1000000 | C:optimistic | - | 16 | 11.38 | 4.23 | 0.26 | 0.24 | 0 | 0 | ok 28 MB reserved |
| M8 intern | 10000000 | A:locked | - | 1 | 810.42 | 1.00 | 1.00 | 0.90 | 0 | 0 | ok 259 MB reserved |
| M8 intern | 10000000 | A:locked | - | 2 | 1019.40 | 0.80 | 0.40 | 1.14 | 0 | 0 | ok 260 MB reserved |
| M8 intern | 10000000 | A:locked | - | 4 | 1413.94 | 0.57 | 0.14 | 1.58 | 0 | 0 | ok 260 MB reserved |
| M8 intern | 10000000 | A:locked | - | 8 | 2849.60 | 0.28 | 0.04 | 3.18 | 0 | 0 | ok 260 MB reserved |
| M8 intern | 10000000 | A:locked | - | 16 | 5338.02 | 0.15 | 0.01 | 5.96 | 0 | 0 | ok 262 MB reserved |
| M8 intern | 10000000 | B:sharded | - | 1 | 802.04 | 1.00 | 1.00 | 0.89 | 0 | 0 | ok 259 MB reserved |
| M8 intern | 10000000 | B:sharded | - | 2 | 573.67 | 1.40 | 0.70 | 0.64 | 0 | 0 | ok 260 MB reserved |
| M8 intern | 10000000 | B:sharded | - | 4 | 373.88 | 2.15 | 0.54 | 0.42 | 0 | 0 | ok 260 MB reserved |
| M8 intern | 10000000 | B:sharded | - | 8 | 242.01 | 3.31 | 0.41 | 0.27 | 0 | 0 | ok 260 MB reserved |
| M8 intern | 10000000 | B:sharded | - | 16 | 195.57 | 4.10 | 0.26 | 0.22 | 0 | 0 | ok 261 MB reserved |
| M8 intern | 10000000 | C:optimistic | - | 1 | 883.47 | 1.00 | 1.00 | 0.99 | 0 | 0 | ok 259 MB reserved |
| M8 intern | 10000000 | C:optimistic | - | 2 | 550.87 | 1.60 | 0.80 | 0.61 | 0 | 0 | ok 260 MB reserved |
| M8 intern | 10000000 | C:optimistic | - | 4 | 412.42 | 2.14 | 0.54 | 0.46 | 0 | 0 | ok 260 MB reserved |
| M8 intern | 10000000 | C:optimistic | - | 8 | 233.83 | 3.78 | 0.47 | 0.26 | 0 | 0 | ok 260 MB reserved |
| M8 intern | 10000000 | C:optimistic | - | 16 | 139.68 | 6.32 | 0.40 | 0.16 | 0 | 0 | ok 261 MB reserved |
| M8 intern | 100000000 | B:sharded | - | 1 | 16356.45 | 1.00 | 1.00 | 1.42 | 0 | 0 | ok 2589 MB reserved |
| M8 intern | 100000000 | B:sharded | - | 2 | 9550.74 | 1.71 | 0.86 | 0.83 | 0 | 0 | ok 2590 MB reserved |
| M8 intern | 100000000 | B:sharded | - | 4 | 5757.21 | 2.84 | 0.71 | 0.50 | 0 | 0 | ok 2590 MB reserved |
| M8 intern | 100000000 | B:sharded | - | 8 | 3701.30 | 4.42 | 0.55 | 0.32 | 0 | 0 | ok 2591 MB reserved |
| M8 intern | 100000000 | B:sharded | - | 16 | 2961.19 | 5.52 | 0.35 | 0.26 | 0 | 0 | ok 2591 MB reserved |
| M8 intern | 100000000 | C:optimistic | - | 1 | 11340.26 | 1.00 | 1.00 | 0.99 | 0 | 0 | ok 2589 MB reserved |
| M8 intern | 100000000 | C:optimistic | - | 2 | 6624.68 | 1.71 | 0.86 | 0.58 | 0 | 0 | ok 2589 MB reserved |
| M8 intern | 100000000 | C:optimistic | - | 4 | 4366.03 | 2.60 | 0.65 | 0.38 | 0 | 0 | ok 2590 MB reserved |
| M8 intern | 100000000 | C:optimistic | - | 8 | 2480.71 | 4.57 | 0.57 | 0.22 | 0 | 0 | ok 2590 MB reserved |
| M8 intern | 100000000 | C:optimistic | - | 16 | 1652.99 | 6.86 | 0.43 | 0.14 | 0 | 0 | ok 2591 MB reserved |

all checks ok
