| case | size | variant | phase | median | peak heap | check | note |
|---|---|---|---|---|---|---|---|
| D4 define AC | 100 | scan | define | 0.231 ms | 0.1 MB | ok | 53 + 46 rules, 0 out of definition order |
| D4 define AC | 100 | index | define | 0.067 ms | 0.1 MB | ok | 53 + 46 rules, 0 out of definition order |
| D4 define AC | 1000 | scan | define | 17.35 ms | 0.6 MB | ok | 472 + 486 rules, 17 out of definition order |
| D4 define AC | 1000 | index | define | 0.755 ms | 0.6 MB | ok | 472 + 486 rules, 17 out of definition order |
| D4 define AC | 10000 | scan | define | 1278 ms | 4.7 MB | ok | 4049 + 4034 rules, 798 out of definition order |
| D4 define AC | 10000 | index | define | 18.64 ms | 4.7 MB | ok | 4049 + 4034 rules, 798 out of definition order |
