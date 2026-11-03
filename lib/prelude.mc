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
fn accumulate(xs) = rest(fold_list((a, b) => a + b, 0, xs))

# dot(u, v): the scalar product of two vectors of equal length
fn dot(u, v) = sum(map(i => u[i]*v[i], range(length(u))))

# norm(v): the Euclidean length of a vector
fn norm(v) = sqrt(dot(v, v))

# fix_point(f, x): f applied repeatedly from x until the value no longer changes
fn fix_point(f, x) = {
  var a = x
  var b = f(x)
  while a !== b {
    a = b
    b = f(b)
  }
  b
}

# say(x, ...): print the arguments on one line (strings as their text, other values printed); the value is null
fn say(parts...) = { print(string_join(map(to_string, parts))); null }
