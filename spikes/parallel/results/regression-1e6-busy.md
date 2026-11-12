| R10 replace | 1000000 | hc+imm (spike 1) | build | 53.05 | 1.00 | ok |
| R10 replace | 1000000 | hc+imm (spike 1) | replace | 79.52 | 1.00 | ok |
| R10 replace | 1000000 | A:locked | build | 49.26 | 0.93 | ok |
| R10 replace | 1000000 | A:locked | replace | 77.11 | 0.97 | ok |
| R10 replace | 1000000 | B:sharded | build | 50.18 | 0.95 | ok |
| R10 replace | 1000000 | B:sharded | replace | 74.92 | 0.94 | ok |
| R10 replace | 1000000 | C:optimistic | build | 60.15 | 1.13 | ok |
| R10 replace | 1000000 | C:optimistic | replace | 91.21 | 1.15 | ok |
| R11 eval | 1000000 | hc+imm (spike 1) | build | 53.23 | 1.00 | ok |
| R11 eval | 1000000 | hc+imm (spike 1) | eval | 92.84 | 1.00 | ok |
| R11 eval | 1000000 | hc+imm (spike 1) | re-eval (unrelated change) | 79.85 | 1.00 | ok |
| R11 eval | 1000000 | hc+imm (spike 1) | re-eval (s3 changed) | 80.15 | 1.00 | ok |
| R11 eval | 1000000 | A:locked | build | 53.40 | 1.00 | ok |
| R11 eval | 1000000 | A:locked | eval | 101.90 | 1.10 | ok |
| R11 eval | 1000000 | A:locked | re-eval (unrelated change) | 92.14 | 1.15 | ok |
| R11 eval | 1000000 | A:locked | re-eval (s3 changed) | 89.15 | 1.11 | ok |
| R11 eval | 1000000 | B:sharded | build | 53.59 | 1.01 | ok |
| R11 eval | 1000000 | B:sharded | eval | 92.94 | 1.00 | ok |
| R11 eval | 1000000 | B:sharded | re-eval (unrelated change) | 83.44 | 1.05 | ok |
| R11 eval | 1000000 | B:sharded | re-eval (s3 changed) | 83.39 | 1.04 | ok |
| R11 eval | 1000000 | C:optimistic | build | 61.91 | 1.16 | ok |
| R11 eval | 1000000 | C:optimistic | eval | 89.14 | 0.96 | ok |
| R11 eval | 1000000 | C:optimistic | re-eval (unrelated change) | 83.42 | 1.04 | ok |
| R11 eval | 1000000 | C:optimistic | re-eval (s3 changed) | 84.70 | 1.06 | ok |
