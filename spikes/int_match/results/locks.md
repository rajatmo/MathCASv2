|---|---|---|---|---|---|---|---|---|---|---|
| case | n | config | threads | ms | speedup vs seq | vs spike 2 | splits | redos | check | note |
| R4 AC rules | 10000 | spike 2 indexed/smart | 1 | 120.08 | | 1.00 | | | | spike 2's sequential evaluator |
| R4 AC rules | 10000 | ref | 1 | 124.16 |  | 1.03 (s2 check ok) | 0 | 0 | ok | 9609 of 10000 subjects rewritten |
| R4 AC rules | 10000 | seq | 1 | 124.13 | 1.00 | 1.03 (s2 check ok) | 0 | 0 | ok | 9609 of 10000 subjects rewritten |
| R4 AC rules | 10000 | lazy | 1 | 124.38 | 1.00 |  | 0 | 0 | ok | 9609 of 10000 subjects rewritten |
| R4 AC rules | 10000 | lazy | 2 | 63.31 | 1.96 |  | 15 | 0 | ok | 9609 of 10000 subjects rewritten |
| R4 AC rules | 10000 | lazy | 4 | 32.16 | 3.86 |  | 29 | 0 | ok | 9609 of 10000 subjects rewritten |
| R4 AC rules | 10000 | always | 4 | 35.18 | 3.53 |  | 9999 | 0 | ok | 9609 of 10000 subjects rewritten |
| R4 AC rules | 10000 | seq hashed-memo | 1 | 130.54 | 0.95 |  | 0 | 0 | ok | 9609 of 10000 subjects rewritten |
| R4 AC rules | 10000 | lazy hashed-memo | 4 | 33.99 | 3.65 |  | 30 | 0 | ok | 9609 of 10000 subjects rewritten |
| R4 AC rules | 10000 | lazy tiny-memo | 4 | 32.30 | 3.84 |  | 29 | 0 | ok | 9609 of 10000 subjects rewritten |
| R4 AC rules | 10000 | lazy locked-index | 4 | 32.45 | 3.83 |  | 29 | 0 | ok | 9609 of 10000 subjects rewritten |
| R5 literal rules | 50000 | spike 2 indexed | 1 | 235.11 | | 1.00 | | | | spike 2's sequential evaluator |
| R5 literal rules | 50000 | ref | 1 | 204.67 |  | 0.87 (s2 check ok) | 0 | 0 | ok | 499681 of 1000000 calls hit a literal rule |
| R5 literal rules | 50000 | seq | 1 | 226.13 | 1.00 | 0.96 (s2 check ok) | 0 | 0 | ok | 499681 of 1000000 calls hit a literal rule |
| R5 literal rules | 50000 | lazy | 1 | 229.15 | 0.99 |  | 0 | 0 | ok | 499681 of 1000000 calls hit a literal rule |
| R5 literal rules | 50000 | lazy | 2 | 105.37 | 2.15 |  | 21 | 0 | ok | 499681 of 1000000 calls hit a literal rule |
| R5 literal rules | 50000 | lazy | 4 | 50.18 | 4.51 |  | 77 | 0 | ok | 499681 of 1000000 calls hit a literal rule |
| R5 literal rules | 50000 | always | 4 | 149.31 | 1.51 |  | 999999 | 0 | ok | 499681 of 1000000 calls hit a literal rule |
| R5 literal rules | 50000 | lazy locked-index | 4 | 65.01 | 3.48 |  | 56 | 0 | ok | 499681 of 1000000 calls hit a literal rule |
| R13 arg-head rules | 1000 | spike 2 arg-head | 1 | 230.13 | | 1.00 | | | | spike 2's sequential evaluator |
| R13 arg-head rules | 1000 | ref | 1 | 224.62 |  | 0.98 (s2 check ok) | 0 | 0 | ok | 99866 calls fell through to the generic rule |
| R13 arg-head rules | 1000 | seq | 1 | 224.08 | 1.00 | 0.97 (s2 check ok) | 0 | 0 | ok | 99866 calls fell through to the generic rule |
| R13 arg-head rules | 1000 | lazy | 1 | 228.41 | 0.98 |  | 0 | 0 | ok | 99866 calls fell through to the generic rule |
| R13 arg-head rules | 1000 | lazy | 2 | 114.22 | 1.96 |  | 32 | 0 | ok | 99866 calls fell through to the generic rule |
| R13 arg-head rules | 1000 | lazy | 4 | 59.33 | 3.78 |  | 125 | 0 | ok | 99866 calls fell through to the generic rule |
| R13 arg-head rules | 1000 | always | 4 | 148.62 | 1.51 |  | 999999 | 0 | ok | 99866 calls fell through to the generic rule |
| R13 arg-head rules | 1000 | lazy locked-index | 4 | 60.63 | 3.70 |  | 176 | 0 | ok | 99866 calls fell through to the generic rule |
[1/27 4%] R4 AC rules n=10000 spike 2
[2/27 7%] R4 AC rules n=10000 ref t=1
[3/27 11%] R4 AC rules n=10000 seq t=1
[4/27 15%] R4 AC rules n=10000 lazy t=1
[5/27 19%] R4 AC rules n=10000 lazy t=2
[6/27 22%] R4 AC rules n=10000 lazy t=4
[7/27 26%] R4 AC rules n=10000 always t=4
[8/27 30%] R4 AC rules n=10000 seq hashed-memo t=1
[9/27 33%] R4 AC rules n=10000 lazy hashed-memo t=4
[10/27 37%] R4 AC rules n=10000 lazy tiny-memo t=4
[11/27 41%] R4 AC rules n=10000 lazy locked-index t=4
[12/27 44%] R5 literal rules n=50000 spike 2
[13/27 48%] R5 literal rules n=50000 ref t=1
[14/27 52%] R5 literal rules n=50000 seq t=1
[15/27 56%] R5 literal rules n=50000 lazy t=1
[16/27 59%] R5 literal rules n=50000 lazy t=2
[17/27 63%] R5 literal rules n=50000 lazy t=4
[18/27 67%] R5 literal rules n=50000 always t=4
[19/27 70%] R5 literal rules n=50000 lazy locked-index t=4
[20/27 74%] R13 arg-head rules n=1000 spike 2
[21/27 78%] R13 arg-head rules n=1000 ref t=1
[22/27 81%] R13 arg-head rules n=1000 seq t=1
[23/27 85%] R13 arg-head rules n=1000 lazy t=1
[24/27 89%] R13 arg-head rules n=1000 lazy t=2
[25/27 93%] R13 arg-head rules n=1000 lazy t=4
[26/27 96%] R13 arg-head rules n=1000 always t=4
[27/27 100%] R13 arg-head rules n=1000 lazy locked-index t=4
