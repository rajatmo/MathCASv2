#! mathcas 2
# The prelude: small general-purpose functions written in MathCAS itself, evaluated when the engine starts.
# Everything here is in the bare namespace (the prelude is not a module). Rules: never define a symbol that has a
# native implementation, and never reuse a name the benchmark cases define.

# is_even(n), is_odd(n): parity of an integer
fn is_even(n: Int) = mod(n, 2) == 0
fn is_odd(n: Int) = mod(n, 2) == 1

# clamp(x, lo, hi): x limited to the interval [lo, hi]
fn clamp(x, lo, hi) = min(max(x, lo), hi)

# log_base(b, x): the logarithm of x to base b (an exact integer when x is a power of the integer b)
fn log_base(b: Int, x: Int) when b > 1 and x > 0 and b^integer_exponent(x, b) == x = integer_exponent(x, b)
fn log_base(b, x) = log(x)/log(b)

# count_if(xs, p): how many elements x of xs have p(x) true
fn count_if(xs, p) = length(select(xs, p))

# every(xs, p), some(xs, p): whether p(x) is true for all elements, for at least one element
fn every(xs, p) = count_if(xs, p) == length(xs)
fn some(xs, p) = count_if(xs, p) > 0

# member(xs, x): whether x is an element of xs (compared by identity)
fn member(xs, x) = some(xs, y => y === x)

# zip(xs, ys): the list of pairs [xs[i], ys[i]], as long as the shorter list
fn zip(xs, ys) = range(min(length(xs), length(ys))) |> map(i => [xs[i], ys[i]], _)

# flat_map(f, xs): the lists f(x) for each element x, joined into one list
fn flat_map(f, xs) = flatten(map(f, xs), 1)

# accumulate(xs): the running sums [xs[1], xs[1] + xs[2], ...]
fn accumulate(xs) = if length(xs) == 0 then [] else fold_list(add, xs)

# fix_point(f, x): f applied repeatedly from x until the value no longer changes; numbers that agree (balls that
# overlap, equal machine reals) count as no change; fix_point(f, x, n): at most n applications; fix_point(f, x,
# same_test: g): until g(previous, next) is true. The last value of fixed_point_list.
fn fix_point(f, x, rest...) = last(apply(fixed_point_list, join([f, x], rest)))

# say(x, ...): print the arguments on one line (strings as their text, other values printed); the value is null
fn say(parts...) = { print(string_join(map(to_string, parts))); null }

# is_ordered(xs), is_ordered(xs, p): whether the elements of xs are in order (canonical, or by the predicate p)
fn is_ordered(xs, rest...) = apply(ordered, join([xs], rest))

# replace_repeated(e, rules): the rules applied over and over until e no longer changes
fn replace_repeated(e, rules) = rewrite(e, rules, strategy: fixed_point)

# reverse_sort_by(xs, f): the elements of xs in decreasing order of f(x)
fn reverse_sort_by(xs, f) = reverse(sort_by(xs, f))

# block_map(f, xs, n), block_map(f, xs, n, d): f applied to the blocks partition(xs, n) (offset d)
fn block_map(f, xs, n) = map(f, partition(xs, n))
fn block_map(f, xs, n, d) = map(f, partition(xs, n, d))
