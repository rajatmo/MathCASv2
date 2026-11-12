| case | size | variant | phase | median | peak heap | check | note |
|---|---|---|---|---|---|---|---|
| D5 define literal | 500 | scan | define | 1.48 ms | 0.1 MB | ok | 476 rules after replacements |
| D5 define literal | 500 | index | define | 0.284 ms | 0.1 MB | ok | 476 rules after replacements |
| D5 define literal | 5000 | scan | define | 132 ms | 1.9 MB | ok | 4751 rules after replacements |
| D5 define literal | 5000 | index | define | 2.99 ms | 1.9 MB | ok | 4751 rules after replacements |
| D5 define literal | 50000 | scan | define | 13358 ms | 15.9 MB | ok | 47501 rules after replacements |
| D5 define literal | 50000 | index | define | 41.47 ms | 15.9 MB | ok | 47501 rules after replacements |
