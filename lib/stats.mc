#! mathcas 2
# The standard module `stats`: descriptive statistics of a list of numbers (exact for exact input).
# Use `import stats` (or `import stats.{mean, median}`), or the qualified names `stats.mean`, ... after an import.

module stats {
  # mean(xs): the arithmetic mean
  pub fn mean(xs) = sum(xs)/length(xs)

  # the squared deviations of xs from their mean (private helper)
  fn squared_deviations(xs) = { let m = mean(xs); map(x => (x - m)^2, xs) }

  # variance(xs): the sample variance (divisor n - 1)
  pub fn variance(xs) = sum(squared_deviations(xs))/(length(xs) - 1)

  # std_dev(xs): the sample standard deviation
  pub fn std_dev(xs) = sqrt(variance(xs))

  # median(xs): the middle element of the sorted list, or the mean of the two middle ones
  pub fn median(xs) = {
    let s = sort(xs)
    let n = length(s)
    if mod(n, 2) == 1 then s[(n + 1)/2] else (s[n/2] + s[n/2 + 1])/2
  }

  # geometric_mean(xs): the n-th root of the product of n positive numbers
  pub fn geometric_mean(xs) = product(xs)^(1/length(xs))
}
