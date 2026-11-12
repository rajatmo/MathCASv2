#! mathcas 2
# The standard module `stats`: descriptive statistics of a list of numbers (exact for exact input).
# Use `import stats` (or `import stats.{mean, median}`), or the qualified names `stats.mean`, ... after an import.
# The members also accept a distribution (`mean(normal_distribution(mu, sigma))`), handing it to the library's
# functions of distributions; `quantile`, `skewness`, `kurtosis`, `moment` and `histogram_list` need no import.

module stats {
  # mean(xs): the arithmetic mean
  pub fn mean(xs) = if is_list(xs) then sum(xs)/length(xs) else moment(xs, 1)

  # the squared deviations of xs from their mean (private helper)
  fn squared_deviations(xs) = { let m = mean(xs); map(x => (x - m)^2, xs) }

  # variance(xs): the sample variance (divisor n - 1)
  pub fn variance(xs) = if is_list(xs) then sum(squared_deviations(xs))/(length(xs) - 1) else central_moment(xs, 2)

  # std_dev(xs): the sample standard deviation
  pub fn std_dev(xs) = sqrt(variance(xs))

  # median(xs): the middle element of the sorted list, or the mean of the two middle ones
  pub fn median(xs) = if is_list(xs) then {
    let s = sort(xs)
    let n = length(s)
    if mod(n, 2) == 1 then s[(n + 1)/2] else (s[n/2] + s[n/2 + 1])/2
  } else quantile(xs, 1/2)

  # geometric_mean(xs): the n-th root of the product of n positive numbers
  pub fn geometric_mean(xs) = product(xs)^(1/length(xs))

  # covariance(xs, ys): the sample covariance of two lists of equal length (divisor n - 1)
  pub fn covariance(xs, ys) = sum((xs - mean(xs))*(ys - mean(ys)))/(length(xs) - 1)

  # correlation(xs, ys): Pearson's correlation coefficient of two lists of equal length
  pub fn correlation(xs, ys) = covariance(xs, ys)/(std_dev(xs)*std_dev(ys))
}
