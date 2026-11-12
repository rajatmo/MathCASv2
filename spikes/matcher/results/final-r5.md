|---|---|---|---|---|---|---|---|
| case | size | variant | phase | median | peak heap | check | note |
| R5 literal rules | 500 | linear | define | 0.296 ms | 8.2 MB | ok |  |
| R5 literal rules | 500 | linear | calls | 1250 ms | 8.2 MB | ok | 499115 of 1000000 calls hit a literal rule |
| R5 literal rules | 500 | indexed | define | 0.308 ms | 8.2 MB | ok |  |
| R5 literal rules | 500 | indexed | calls | 120 ms | 8.2 MB | ok | 499115 of 1000000 calls hit a literal rule |
| R5 literal rules | 5000 | linear | define | 2.84 ms | 10.5 MB | ok |  |
| R5 literal rules | 5000 | indexed | define | 4.43 ms | 10.5 MB | ok |  |
| R5 literal rules | 5000 | linear | calls | 18827 ms | 10.5 MB | ok | 499026 of 1000000 calls hit a literal rule |
| R5 literal rules | 5000 | indexed | calls | 231 ms | 10.5 MB | ok | 499026 of 1000000 calls hit a literal rule |
| R5 literal rules | 50000 | linear | define | 60.92 ms | 32.1 MB | limit |  |
| R5 literal rules | 50000 | linear | calls | 59893 ms | 32.1 MB | limit | limit: 339452 of 1000000 calls in 59.9 s (176.44 us each) |
| R5 literal rules | 50000 | indexed | define | 67.81 ms | 32.1 MB | ok |  |
| R5 literal rules | 50000 | indexed | calls | 279 ms | 32.1 MB | ok | 499681 of 1000000 calls hit a literal rule |
