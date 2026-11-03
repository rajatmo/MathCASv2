| case | size | store | split | threads | median ms | speedup | efficiency | vs seq | splits | redos | check |
|---|---|---|---|---|---|---|---|---|---|---|---|
| LIMIT | fuel 10^5, interrupt | C:optimistic | never | 1 | fuel used by the root task 102400 | ok |
| LIMIT | fuel 10^5, interrupt | C:optimistic | lazy | 1 | fuel used by the root task 102400 | ok |
| LIMIT | fuel 10^5, interrupt | C:optimistic | always | 1 | fuel used by the root task 353955 | ok |
| LIMIT | fuel 10^5, interrupt | C:optimistic | never | 2 | fuel used by the root task 102400 | ok |
| LIMIT | fuel 10^5, interrupt | C:optimistic | lazy | 2 | fuel used by the root task 126976 | ok |
| LIMIT | fuel 10^5, interrupt | C:optimistic | always | 2 | fuel used by the root task 353955 | ok |
| LIMIT | fuel 10^5, interrupt | C:optimistic | never | 4 | fuel used by the root task 102400 | ok |
| LIMIT | fuel 10^5, interrupt | C:optimistic | lazy | 4 | fuel used by the root task 126976 | ok |
| LIMIT | fuel 10^5, interrupt | C:optimistic | always | 4 | fuel used by the root task 360448 | ok |
| LIMIT | fuel 10^5, interrupt | C:optimistic | never | 8 | fuel used by the root task 102400 | ok |
| LIMIT | fuel 10^5, interrupt | C:optimistic | lazy | 8 | fuel used by the root task 126976 | ok |
| LIMIT | fuel 10^5, interrupt | C:optimistic | always | 8 | fuel used by the root task 360448 | ok |
| LIMIT | fuel 10^5, interrupt | C:optimistic | never | 16 | fuel used by the root task 102400 | ok |
| LIMIT | fuel 10^5, interrupt | C:optimistic | lazy | 16 | fuel used by the root task 126976 | ok |
| LIMIT | fuel 10^5, interrupt | C:optimistic | always | 16 | fuel used by the root task 360448 | ok |

all checks ok
