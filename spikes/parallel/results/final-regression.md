| case | size | store | split | threads | median ms | speedup | efficiency | vs seq | splits | redos | check |
|---|---|---|---|---|---|---|---|---|---|---|---|

|---|---|---|---|---|---|---|
| case | size | store | phase | median ms | vs hc+imm | check |
| R10 replace | 10000 | hc+imm (spike 1) | build | 0.52 | 1.00 | ok |
| R10 replace | 10000 | A:locked | build | 0.52 | 1.00 | ok |
| R10 replace | 10000 | hc+imm (spike 1) | replace | 0.57 | 1.00 | ok |
| R10 replace | 10000 | A:locked | replace | 0.61 | 1.07 | ok |
| R10 replace | 10000 | B:sharded | build | 0.57 | 1.08 | ok |
| R10 replace | 10000 | B:sharded | replace | 0.60 | 1.06 | ok |
| R10 replace | 10000 | C:optimistic | build | 0.51 | 0.98 | ok |
| R10 replace | 10000 | C:optimistic | replace | 0.59 | 1.04 | ok |
| R10 replace | 100000 | hc+imm (spike 1) | build | 4.98 | 1.00 | ok |
| R10 replace | 100000 | hc+imm (spike 1) | replace | 7.58 | 1.00 | ok |
| R10 replace | 100000 | A:locked | replace | 5.99 | 0.79 | ok |
| R10 replace | 100000 | A:locked | build | 4.69 | 0.94 | ok |
| R10 replace | 100000 | B:sharded | replace | 6.16 | 0.81 | ok |
| R10 replace | 100000 | B:sharded | build | 4.87 | 0.98 | ok |
| R10 replace | 100000 | C:optimistic | build | 4.68 | 0.94 | ok |
| R10 replace | 100000 | C:optimistic | replace | 7.27 | 0.96 | ok |
| R10 replace | 1000000 | hc+imm (spike 1) | build | 54.35 | 1.00 | ok |
| R10 replace | 1000000 | A:locked | build | 48.39 | 0.89 | ok |
| R10 replace | 1000000 | hc+imm (spike 1) | replace | 82.49 | 1.00 | ok |
| R10 replace | 1000000 | B:sharded | build | 49.91 | 0.92 | ok |
| R10 replace | 1000000 | C:optimistic | replace | 92.04 | 1.12 | ok |
| R10 replace | 1000000 | A:locked | replace | 76.09 | 0.92 | ok |
| R10 replace | 1000000 | B:sharded | replace | 74.84 | 0.91 | ok |
| R10 replace | 1000000 | C:optimistic | build | 57.55 | 1.06 | ok |
| R11 eval | 10000 | hc+imm (spike 1) | build | 0.54 | 1.00 | ok |
| R11 eval | 10000 | hc+imm (spike 1) | eval | 0.64 | 1.00 | ok |
| R11 eval | 10000 | hc+imm (spike 1) | re-eval (unrelated change) | 0.61 | 1.00 | ok |
| R11 eval | 10000 | hc+imm (spike 1) | re-eval (s3 changed) | 0.61 | 1.00 | ok |
| R11 eval | 10000 | A:locked | eval | 0.63 | 0.99 | ok |
| R11 eval | 10000 | A:locked | build | 0.57 | 1.05 | ok |
| R11 eval | 10000 | B:sharded | build | 0.53 | 0.97 | ok |
| R11 eval | 10000 | B:sharded | re-eval (unrelated change) | 0.63 | 1.04 | ok |
| R11 eval | 10000 | A:locked | re-eval (s3 changed) | 0.60 | 0.99 | ok |
| R11 eval | 10000 | B:sharded | re-eval (s3 changed) | 0.63 | 1.03 | ok |
| R11 eval | 10000 | A:locked | re-eval (unrelated change) | 0.61 | 1.00 | ok |
| R11 eval | 10000 | C:optimistic | build | 0.51 | 0.93 | ok |
| R11 eval | 10000 | C:optimistic | eval | 0.67 | 1.05 | ok |
| R11 eval | 10000 | B:sharded | eval | 0.67 | 1.05 | ok |
| R11 eval | 10000 | C:optimistic | re-eval (unrelated change) | 0.61 | 1.01 | ok |
| R11 eval | 10000 | C:optimistic | re-eval (s3 changed) | 0.61 | 1.01 | ok |
| R11 eval | 100000 | hc+imm (spike 1) | build | 5.12 | 1.00 | ok |
| R11 eval | 100000 | hc+imm (spike 1) | eval | 7.59 | 1.00 | ok |
| R11 eval | 100000 | hc+imm (spike 1) | re-eval (s3 changed) | 7.06 | 1.00 | ok |
| R11 eval | 100000 | hc+imm (spike 1) | re-eval (unrelated change) | 7.12 | 1.00 | ok |
| R11 eval | 100000 | A:locked | eval | 6.81 | 0.90 | ok |
| R11 eval | 100000 | A:locked | build | 4.90 | 0.96 | ok |
| R11 eval | 100000 | A:locked | re-eval (s3 changed) | 6.62 | 0.94 | ok |
| R11 eval | 100000 | B:sharded | re-eval (s3 changed) | 6.72 | 0.95 | ok |
| R11 eval | 100000 | B:sharded | build | 4.86 | 0.95 | ok |
| R11 eval | 100000 | A:locked | re-eval (unrelated change) | 6.67 | 0.94 | ok |
| R11 eval | 100000 | C:optimistic | build | 4.67 | 0.91 | ok |
| R11 eval | 100000 | C:optimistic | eval | 6.91 | 0.91 | ok |
| R11 eval | 100000 | B:sharded | re-eval (unrelated change) | 6.71 | 0.94 | ok |
| R11 eval | 100000 | B:sharded | eval | 7.02 | 0.92 | ok |
| R11 eval | 100000 | C:optimistic | re-eval (unrelated change) | 6.58 | 0.92 | ok |
| R11 eval | 100000 | C:optimistic | re-eval (s3 changed) | 6.57 | 0.93 | ok |
| R11 eval | 1000000 | hc+imm (spike 1) | build | 55.02 | 1.00 | ok |
| R11 eval | 1000000 | hc+imm (spike 1) | re-eval (unrelated change) | 81.36 | 1.00 | ok |
| R11 eval | 1000000 | hc+imm (spike 1) | eval | 89.82 | 1.00 | ok |
| R11 eval | 1000000 | A:locked | eval | 88.97 | 0.99 | ok |
| R11 eval | 1000000 | hc+imm (spike 1) | re-eval (s3 changed) | 83.19 | 1.00 | ok |
| R11 eval | 1000000 | B:sharded | build | 50.92 | 0.93 | ok |
| R11 eval | 1000000 | A:locked | re-eval (unrelated change) | 81.02 | 1.00 | ok |
| R11 eval | 1000000 | B:sharded | re-eval (unrelated change) | 78.36 | 0.96 | ok |
| R11 eval | 1000000 | B:sharded | eval | 84.80 | 0.94 | ok |
| R11 eval | 1000000 | B:sharded | re-eval (s3 changed) | 84.58 | 1.02 | ok |
| R11 eval | 1000000 | C:optimistic | eval | 88.20 | 0.98 | ok |
| R11 eval | 1000000 | C:optimistic | build | 63.75 | 1.16 | ok |
| R11 eval | 1000000 | A:locked | build | 49.92 | 0.91 | ok |
| R11 eval | 1000000 | A:locked | re-eval (s3 changed) | 81.58 | 0.98 | ok |
| R11 eval | 1000000 | C:optimistic | re-eval (s3 changed) | 87.55 | 1.05 | ok |

| R11 eval | 1000000 | C:optimistic | re-eval (unrelated change) | 85.19 | 1.05 | ok |
all checks ok
