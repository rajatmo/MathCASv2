#! mathcas 2
# A script: `mc-repl examples/hello.mc` runs it, shows what it prints and the value of its last statement.

say("Exact arithmetic: ", 1/3 + 1/6)
say("A big number: 2^100 = ", 2^100)

# functions are rules; `fn` clauses are tried in order
fn collatz(1) = [1]
fn collatz(n: Int) = prepend(collatz(if is_even(n) then n/2 else 3n + 1), n)

say("The Collatz path of 6: ", collatz(6))

# functions from the prelude: unique, accumulate, count_if
say("Running sums: ", accumulate(range(5)))
say("Odd numbers below 20: ", count_if(range(20), is_odd))

# the last value is printed (a null value would not be)
unique([3, 1, 4, 1, 5, 9, 2, 6, 5, 3, 5])
