| case | size | store | total ms | phases (ms) | GCs | pause total | pause max | pause mean | live nodes after last GC | reserved after last GC MB | max reserved after GC MB | peak heap MB | overhead | check | notes |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| R10 replace | 10000 | hc+imm+stamps | 1.16 | build 0.524, safe point (tree live) 0.000, replace 0.640, safe point (tree dead) 0.000 | 0 | 0.000 | 0.000 | 0.000 | - | - | - | 0.7 | +0% | ok |  |
| R10 replace | 10000 | none | 1.16 | build 0.519, safe point (tree live) 0.000, replace 0.642, safe point (tree dead) 0.000 | 0 | 0.000 | 0.000 | 0.000 | - | - | - | 0.7 | +0% | ok |  |
| R10 replace | 10000 | A | 1.33 | build 0.521, safe point (tree live) 0.057, replace 0.644, safe point (tree dead) 0.100 | 2 | 0.156 | 0.100 | 0.078 | 4502 | 0.7 | 0.7 | 0.7 | +15% | ok |  |
| R10 replace | 10000 | B | 1.38 | build 0.524, safe point (tree live) 0.104, replace 0.606, safe point (tree dead) 0.142 | 2 | 0.244 | 0.142 | 0.122 | 4502 | 0.7 | 0.7 | 0.7 | +19% | ok |  |
| R10 replace | 10000 | C1 | 1.36 | build 0.510, safe point (tree live) 0.148, replace 0.568, safe point (tree dead) 0.127 | 2 (2 minor) | 0.275 | 0.148 | 0.137 | 8500 | 0.7 | 0.7 | 0.8 | +17% | ok |  |
| R10 replace | 10000 | C0 | 1.35 | build 0.509, safe point (tree live) 0.148, replace 0.566, safe point (tree dead) 0.126 | 2 (2 minor) | 0.274 | 0.148 | 0.137 | 8500 | 0.7 | 0.7 | 0.8 | +17% | ok |  |
| R10 replace | 10000 | C | 1.37 | build 0.522, safe point (tree live) 0.150, replace 0.568, safe point (tree dead) 0.128 | 2 (2 minor) | 0.280 | 0.153 | 0.140 | 8500 | 0.7 | 0.7 | 0.8 | +18% | ok |  |
| R10 replace | 10000 | A-fl | 1.30 | build 0.532, safe point (tree live) 0.054, replace 0.606, safe point (tree dead) 0.107 | 2 | 0.158 | 0.106 | 0.079 | 4502 | 0.7 | 0.7 | 0.7 | +12% | ok |  |
| R10 replace | 100000 | hc+imm+stamps | 13.70 | build 5.71, safe point (tree live) 0.000, replace 8.02, safe point (tree dead) 0.000 | 0 | 0.000 | 0.000 | 0.000 | - | - | - | 5.9 | -1% | ok |  |
| R10 replace | 100000 | none | 13.90 | build 5.78, safe point (tree live) 0.000, replace 8.10, safe point (tree dead) 0.000 | 0 | 0.000 | 0.000 | 0.000 | - | - | - | 5.9 | +0% | ok |  |
| R10 replace | 100000 | C | 15.98 | build 5.82, safe point (tree live) 1.37, replace 7.01, safe point (tree dead) 1.76 | 2 (2 minor) | 3.19 | 1.80 | 1.59 | 77603 | 6.1 | 6.1 | 6.6 | +15% | ok |  |
| R10 replace | 100000 | A-fl | 15.15 | build 5.79, safe point (tree live) 0.620, replace 7.42, safe point (tree dead) 1.44 | 2 | 2.05 | 1.44 | 1.03 | 40391 | 6.3 | 6.3 | 6.4 | +9% | ok |  |
| R10 replace | 100000 | B | 15.41 | build 5.73, safe point (tree live) 0.934, replace 7.33, safe point (tree dead) 1.42 | 2 | 2.31 | 1.38 | 1.15 | 40391 | 5.8 | 5.8 | 5.9 | +11% | ok |  |
| R10 replace | 100000 | A | 15.00 | build 5.72, safe point (tree live) 0.617, replace 7.32, safe point (tree dead) 1.30 | 2 | 1.89 | 1.27 | 0.945 | 40391 | 6.1 | 6.1 | 6.2 | +8% | ok |  |
| R10 replace | 100000 | C0 | 15.86 | build 5.74, safe point (tree live) 1.36, replace 7.00, safe point (tree dead) 1.65 | 2 (2 minor) | 3.00 | 1.63 | 1.50 | 77603 | 6.1 | 6.1 | 6.6 | +14% | ok |  |
| R10 replace | 100000 | C1 | 15.70 | build 5.76, safe point (tree live) 1.40, replace 6.99, safe point (tree dead) 1.67 | 2 (2 minor) | 3.00 | 1.67 | 1.50 | 77603 | 6.1 | 6.1 | 6.6 | +13% | ok |  |
| R10 replace | 1000000 | hc+imm+stamps | 154 | build 65.23, safe point (tree live) 0.000, replace 88.13, safe point (tree dead) 0.000 | 0 | 0.000 | 0.000 | 0.000 | - | - | - | 47.2 | -7% | ok |  |
| R10 replace | 1000000 | none | 166 | build 66.81, safe point (tree live) 0.000, replace 99.02, safe point (tree dead) 0.000 | 0 | 0.000 | 0.000 | 0.000 | - | - | - | 47.2 | +0% | ok |  |
| R10 replace | 1000000 | A | 171 | build 66.59, safe point (tree live) 5.44, replace 86.24, safe point (tree dead) 11.55 | 2 | 16.70 | 11.26 | 8.35 | 362857 | 48.2 | 48.2 | 49.4 | +3% | ok |  |
| R10 replace | 1000000 | A-fl | 182 | build 67.37, safe point (tree live) 5.86, replace 89.87, safe point (tree dead) 13.11 | 2 | 18.03 | 12.40 | 9.01 | 362857 | 49.8 | 49.8 | 50.9 | +10% | ok |  |
| R10 replace | 1000000 | C | 195 | build 64.50, safe point (tree live) 21.28, replace 90.22, safe point (tree dead) 19.00 | 3 (2 minor) | 39.92 | 18.71 | 13.31 | 707932 | 48.8 | 48.8 | 52.5 | +18% | ok |  |
| R10 replace | 1000000 | B | 189 | build 68.18, safe point (tree live) 8.48, replace 99.28, safe point (tree dead) 18.39 | 2 | 21.91 | 13.44 | 10.95 | 362857 | 46.1 | 46.1 | 47.4 | +14% | ok |  |
| R10 replace | 1000000 | C1 | 196 | build 65.70, safe point (tree live) 21.55, replace 89.33, safe point (tree dead) 19.00 | 3 (2 minor) | 39.51 | 18.28 | 13.17 | 707932 | 48.8 | 48.8 | 52.5 | +18% | ok |  |
| R10 replace | 1000000 | C0 | 191 | build 65.56, safe point (tree live) 21.15, replace 86.03, safe point (tree dead) 18.05 | 3 (2 minor) | 40.38 | 19.33 | 13.46 | 707932 | 48.8 | 48.8 | 52.5 | +15% | ok |  |
