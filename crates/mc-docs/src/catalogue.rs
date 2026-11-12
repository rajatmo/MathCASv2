//! The catalogue: every library function with its one-line description (the `FUNCTIONS` lists the engine installs),
//! sorted into the five areas of the system and a topic per library.
use mc_libraries::{mc_algebra, mc_assoc, mc_cad, mc_calculus, mc_combinat, mc_decide, mc_diffalg, mc_dist, mc_fields, mc_further_functions, mc_geom, mc_graphics_kernel, mc_groups, mc_io, mc_jit, mc_langlib, mc_linalg, mc_lists, mc_mechanics, mc_numeric, mc_numtheory, mc_ode, mc_poly, mc_quad, mc_random, mc_roots, mc_simplify, mc_solve, mc_special, mc_strings, mc_sums};

/// The five areas of the system, the tiles of the home page.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Area {
    /// the language and its data structures: lists, associations, strings, the session
    Core,
    /// reading and writing files, random numbers, statistics
    Data,
    /// plots and images
    Visualisation,
    /// calculus, algebra, equations, numerical methods
    SymbolicNumeric,
    /// number theory, special functions, linear algebra
    Higher,
}

impl Area {
    /// Every area, in the home page's order.
    pub const ALL: [Area; 5] = [Area::Core, Area::Data, Area::Visualisation, Area::SymbolicNumeric, Area::Higher];

    /// The tile's title.
    pub fn title(self) -> &'static str {
        match self {
            Area::Core => "Core language",
            Area::Data => "Data",
            Area::Visualisation => "Visualisation",
            Area::SymbolicNumeric => "Symbolic and numeric computation",
            Area::Higher => "Higher mathematics",
        }
    }

    /// A sentence under the title.
    pub fn blurb(self) -> &'static str {
        match self {
            Area::Core => "Lists and how to build, reshape and search them; associations and sets; text; the session.",
            Area::Data => "Files in and out, random numbers, probability distributions and statistics.",
            Area::Visualisation => "Curves and point sets drawn as images.",
            Area::SymbolicNumeric => {
                "Derivatives, integrals, equations, simplification, polynomials, and numerical integration, \
                 root finding and differential equations."
            }
            Area::Higher => "Number theory, special functions and linear algebra.",
        }
    }

    /// A short word for the self-test and links (`guide:core`).
    pub fn key(self) -> &'static str {
        match self {
            Area::Core => "core",
            Area::Data => "data",
            Area::Visualisation => "visualisation",
            Area::SymbolicNumeric => "symbolic",
            Area::Higher => "higher",
        }
    }

    /// The area of a key.
    pub fn from_key(key: &str) -> Option<Area> {
        Area::ALL.into_iter().find(|a| a.key() == key)
    }
}

/// One function.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// the name
    pub name: &'static str,
    /// the one-line description (it starts with the signature)
    pub summary: &'static str,
    /// the area
    pub area: Area,
    /// the topic within the area
    pub topic: &'static str,
}

/// The evaluator's own functions that a program calls by name (no library lists them): the operators' functions
/// and the core tools, described here.
const LANGUAGE: &[(&str, &str)] = &[
    ("add", "add(a, b, ...): the sum a + b + ..."),
    ("mul", "mul(a, b, ...): the product a*b*..."),
    ("pow", "pow(a, b): the power a^b"),
    ("equal", "equal(a, b): a == b, true or false when it can be decided, else left as it is"),
    ("unequal", "unequal(a, b): a != b"),
    ("less", "less(a, b): a < b"),
    ("less_equal", "less_equal(a, b): a <= b"),
    ("greater", "greater(a, b): a > b"),
    ("greater_equal", "greater_equal(a, b): a >= b"),
    ("same", "same(a, b): a === b, whether the two terms are identical (1 and 1.0 are not)"),
    ("not_same", "not_same(a, b): a !== b"),
    ("and", "and(a, b, ...): a and b, evaluated left to right, stopping at the first false"),
    ("or", "or(a, b, ...): a or b, evaluated left to right, stopping at the first true"),
    ("not", "not(a): the negation of a truth value"),
    ("map", "map(f, xs): [f(x1), f(x2), ...] for the list xs"),
    ("print", "print(x): write x as a line of output (a string as its text); the value is x"),
    ("rewrite", "rewrite(e, rule) or rewrite(e, [rules]): e with every part a rule matches replaced"),
    ("is_ground", "is_ground(e): whether e holds no unknown symbol"),
    ("complex", "complex(re, im): the complex number re + im*im"),
    ("expm1", "expm1(x): exp(x) - 1, accurate for small x"),
    ("log1p", "log1p(x): log(1 + x), accurate for small x"),
    ("throw", "throw(v), throw(v, tag): failure(thrown, v[, tag]), the value a surrounding catch returns"),
    ("catch", "catch(e), catch(e, form), catch(e, form, f): the value of the first throw reached in e (tag matching form; f(v, tag))"),
    ("sow", "sow(e), sow(e, tag): e, collected by the innermost reap whose pattern matches the tag"),
    ("reap", "reap(e), reap(e, p), reap(e, p, f): [value of e, the values sown in it, grouped by tag]"),
    ("repeated", "repeated(p), repeated(p, n), repeated(p, [m, n]): the pattern of a run of arguments each matching p"),
    ("key_value_pattern", "key_value_pattern([p1, ...]): the pattern of an association (or list of rules) with entries matching p1, ..."),
    ("replace_list", "replace_list(e, rules), replace_list(e, rules, n): the results of every way the rules match the whole of e"),
];

/// The prelude's functions (`lib/prelude.mc`, written in the language itself and defined in every session): no
/// library lists them, so they are described here. A test checks each is defined by the engine.
const PRELUDE: &[(&str, &str)] = &[
    ("say", "say(x, ...): print the arguments on one line, strings as their text; the value is null"),
    ("is_even", "is_even(n): whether the integer n is even"),
    ("is_odd", "is_odd(n): whether the integer n is odd"),
    ("clamp", "clamp(x, lo, hi): x limited to the interval [lo, hi]"),
    ("log_base", "log_base(b, x): the logarithm of x to base b, exact when x is a power of the integer b"),
    ("count_if", "count_if(xs, p): how many elements x of xs have p(x) true"),
    ("every", "every(xs, p): whether p(x) is true for every element x of xs"),
    ("some", "some(xs, p): whether p(x) is true for at least one element x of xs"),
    ("member", "member(xs, x): whether x is an element of xs, compared by identity"),
    ("zip", "zip(xs, ys): the pairs [xs[i], ys[i]], as long as the shorter list"),
    ("flat_map", "flat_map(f, xs): the lists f(x) for each element x of xs, joined into one list"),
    ("accumulate", "accumulate(xs): the running sums [xs[1], xs[1] + xs[2], ...]"),
    ("fix_point", "fix_point(f, x): f applied repeatedly from x until the value no longer changes"),
    ("is_ordered", "is_ordered(xs), is_ordered(xs, p): whether the elements of xs are in order"),
    ("replace_repeated", "replace_repeated(e, rules): the rules applied until e no longer changes"),
    ("reverse_sort_by", "reverse_sort_by(xs, f): the elements in decreasing order of f(x)"),
    ("block_map", "block_map(f, xs, n), block_map(f, xs, n, d): f applied to the blocks of n elements"),
    ("length_while", "length_while(xs, f): how many elements from the start have f(x) true"),
    ("take_while", "take_while(xs, f): the elements from the start while f(x) is true"),
    ("fold_while", "fold_while(f, x, xs, test): fold while test of the latest value is true, the first value failing it"),
    ("fold_while_list", "fold_while_list(f, x, xs, test): the values fold_while goes through, x first"),
    ("commonest", "commonest(xs), commonest(xs, n): the most frequent elements (the n most), in order of first occurrence"),
    ("boole", "boole(b): 1 for true, 0 for false, element-wise on lists"),
    ("is_numeric", "is_numeric(x): whether x is a number or evaluates numerically to one"),
    ("digit_count", "digit_count(n, b, d): how many digits d n has in base b (all digits 1, ..., b - 1, 0 without d)"),
    ("digit_sum", "digit_sum(n), digit_sum(n, b), digit_sum(n, b, k): the sum of the digits of n in base b"),
    ("key_value_map", "key_value_map(f, a): [f(k, v), ...] for the entries k -> v of the association a"),
    ("key_map", "key_map(f, a): the association with each key k replaced by f(k)"),
    ("association_map", "association_map(f, xs): the association of x -> f(x) for the elements of xs"),
    ("association_thread", "association_thread(ks, vs): the association of ks[i] -> vs[i]"),
    ("delete_missing", "delete_missing(xs): xs without its missing(...) elements (or entries)"),
    ("covariance", "covariance(xs, ys), covariance(m): the sample covariance of two lists, or of the columns of a matrix"),
    ("correlation", "correlation(xs, ys), correlation(m): Pearson's correlation of two lists, or of the columns of a matrix"),
    ("bin_counts", "bin_counts(xs, w), bin_counts(xs, [a, b, dx]), bin_counts(xs, [[edges]]): the counts of xs in bins"),
    ("distribute", "distribute(e), distribute(e, g), distribute(e, g, f): f(..., a + b, ...) spread over the sums (calls of g) among its arguments"),
    ("sequence_fold_list", "sequence_fold_list(f, [x1, ..., xk], xs): the values, each f of the last k values and the next element"),
    ("sequence_fold", "sequence_fold(f, init, xs): the last value of sequence_fold_list"),
    ("curry_applied", "curry_applied(f, spec): f taking its arguments over several calls (spec n, [i1, ...] or n -> [i1, ...])"),
    ("operator_applied", "operator_applied(f): operator_applied(f)(x)(y) is f(y, x); operator_applied(f, spec): as curry_applied"),
    ("reverse_applied", "reverse_applied(f): reverse_applied(f)(a, b, ...) is f(..., b, a)"),
    ("take_list", "take_list(xs, [n1, n2, ...]): the successive pieces of n1, n2, ... elements of xs"),
    ("array_reshape", "array_reshape(xs, dims), array_reshape(xs, dims, pad): the elements of xs arranged with dimensions dims"),
    ("flatten_at", "flatten_at(xs, pos): xs with the part at pos (or at each of a list of positions) spliced into its parent"),
    ("key_complement", "key_complement([a1, a2, ...]): the entries of a1 whose keys are in none of the others"),
    ("operate", "operate(p, e), operate(p, e, n): e with p applied to its head (at depth n of heads)"),
    ("numerical_sort", "numerical_sort(xs): the elements in increasing numerical value, the non-numerical ones after"),
    ("design_matrix", "design_matrix(data, [f1, ...], x): the values of the basis functions at the points of data"),
    ("fit", "fit(data, [f1, ...], x): the least-squares combination c1 f1 + ... of the basis functions"),
    ("coordinate_bounds", "coordinate_bounds(pts), coordinate_bounds(pts, d): [[min, max], ...] of each coordinate of the points, padded by d"),
    ("coordinate_bounding_box", "coordinate_bounding_box(pts), coordinate_bounding_box(pts, d): the corners [mins, maxs] of the points"),
    ("coordinate_bounds_array", "coordinate_bounds_array(bounds, d, offset, pad): the array of the grid points in the box with steps d"),
    ("coordinate_bounding_box_array", "coordinate_bounding_box_array(corners, ...): coordinate_bounds_array of the box with those corners"),
    ("nand", "nand(a, b, ...): not (a and b and ...)"),
    ("nor", "nor(a, b, ...): not (a or b or ...)"),
    ("divide", "divide(a, b): a/b"),
    ("echo", "echo(x), echo(x, label), echo(x, label, f): print x (or f(x)) after the label and give x"),
    ("echo_label", "echo_label(label): the function printing its argument after the label and giving it"),
    ("echo_function", "echo_function(f), echo_function(label, f): the function printing f(x) and giving x"),
    ("echo_evaluation", "echo_evaluation(e), echo_evaluation(e, label): print e unevaluated and its value, and give the value"),
    ("echo_timing", "echo_timing(e), echo_timing(e, label): print the seconds e took (after the label) and give the value of e"),
    ("maximal_by", "maximal_by(xs, f), maximal_by(xs, f, n): every element with the largest f(x) (the n largest)"),
    ("minimal_by", "minimal_by(xs, f), minimal_by(xs, f, n): every element with the smallest f(x) (the n smallest)"),
    ("bin_lists", "bin_lists(xs, w), bin_lists(xs, [a, b, dx]), bin_lists(xs, [[edges]]): the elements of xs in each bin"),
    ("subtract", "subtract(x, y): x - y"),
    ("minus", "minus(x): -x, the negation as a function"),
    ("greater_than", "greater_than(y): the function x => x > y"),
    ("less_than", "less_than(y): the function x => x < y"),
    ("greater_equal_than", "greater_equal_than(y): the function x => x >= y"),
    ("less_equal_than", "less_equal_than(y): the function x => x <= y"),
    ("equal_to", "equal_to(y): the function x => x == y"),
    ("unequal_to", "unequal_to(y): the function x => x != y"),
    ("order", "order(a, b): 1, 0 or -1 as a comes before b in the canonical order, is b, or comes after"),
    ("numerical_order", "numerical_order(a, b): 1, 0 or -1 as a is numerically smaller than b, equal to it, or larger"),
    ("lexicographic_order", "lexicographic_order(a, b), lexicographic_order(a, b, p): the order of the first differing parts, a prefix first"),
    ("lexicographic_sort", "lexicographic_sort(xs), lexicographic_sort(xs, p): the elements in lexicographic order of their parts"),
    ("alphabetic_order", "alphabetic_order(a, b): 1, 0 or -1 as the strings are in alphabetic order; alphabetic_order() the ordering function"),
    ("alphabetic_sort", "alphabetic_sort(xs): the strings of xs in alphabetic order"),
    ("alphabet", "alphabet(): the letters a to z"),
    ("ordering_by", "ordering_by(xs, f), ordering_by(xs, f, n): the positions of the elements in the canonical order of f(x)"),
    ("split_by", "split_by(xs, f), split_by(xs, [f1, f2, ...]): the runs of neighbours with identical f(x)"),
    ("count_distinct_by", "count_distinct_by(xs, f): the number of distinct values f(x)"),
    ("delete_adjacent_duplicates", "delete_adjacent_duplicates(xs), delete_adjacent_duplicates(xs, test): xs without elements repeating the one before"),
    ("key_sort_by", "key_sort_by(a, f): the association with its entries in the canonical order of f(key)"),
    ("hamming_distance", "hamming_distance(a, b): the number of places where the strings or lists a and b differ"),
    ("unique_elements", "unique_elements([xs1, xs2, ...]): for each list, its elements in none of the others"),
    ("subset_map", "subset_map(f, xs, positions): xs with the parts at the positions replaced by the elements of f(those parts)"),
    ("boolean_table", "boolean_table(e, [x1, ...]): the values of e for every assignment of true and false to the variables"),
    ("is_tautology", "is_tautology(e), is_tautology(e, vars): whether the Boolean expression e is true for every assignment"),
    ("is_satisfiable", "is_satisfiable(e), is_satisfiable(e, vars): whether the Boolean expression e is true for some assignment"),
    ("equivalent", "equivalent(a, b, ...): whether the arguments are all true or all false"),
    ("probability", "probability(c, x, d): the probability that the condition c on x holds for x with the distribution d"),
    ("hazard_function", "hazard_function(d, x): the hazard rate of the distribution d at x"),
    ("confirm", "confirm(x), confirm(x, info, tag): x, or a confirmation failure thrown to the enclosing enclose"),
    ("enclose", "enclose(e), enclose(e, f, tag): the value of e, or the failure a confirm inside it threw"),
    ("array_pad", "array_pad(a, m), array_pad(a, m, pad): a with m entries added on every side (pad, or \"Fixed\", \"Periodic\", \"Reversed\", \"Reflected\")"),
    ("hankel_matrix", "hankel_matrix(n), hankel_matrix(c), hankel_matrix(c, r): the Hankel matrix with first column c and last row r"),
    ("toeplitz_matrix", "toeplitz_matrix(n), toeplitz_matrix(c), toeplitz_matrix(c, r): the Toeplitz matrix with first column c and first row r"),
    ("center_array", "center_array(a, n), center_array(a, n, x): the list a centred in n entries of padding"),
    ("diagonal", "diagonal(m), diagonal(m, k): the entries of the k-th diagonal of the matrix m"),
    ("upper_triangularize", "upper_triangularize(m), upper_triangularize(m, k): m with the entries below the k-th diagonal replaced by zero"),
    ("lower_triangularize", "lower_triangularize(m), lower_triangularize(m, k): m with the entries above the k-th diagonal replaced by zero"),
    ("upper_triangular_matrix_q", "upper_triangular_matrix_q(m), upper_triangular_matrix_q(m, k): whether every entry below the k-th diagonal is zero"),
    ("lower_triangular_matrix_q", "lower_triangular_matrix_q(m), lower_triangular_matrix_q(m, k): whether every entry above the k-th diagonal is zero"),
    ("diagonal_matrix_q", "diagonal_matrix_q(m), diagonal_matrix_q(m, k): whether every entry off the k-th diagonal is zero"),
    ("minors", "minors(m), minors(m, k), minors(m, k, f): the matrix of the k x k minors of m"),
    ("adjugate", "adjugate(m): the transpose of the cofactor matrix of m"),
    ("array_flatten", "array_flatten(a): the matrix built from the block matrix a"),
    ("projection", "projection(u, v), projection(u, v, f): the projection of the vector u onto v"),
    ("orthogonalize", "orthogonalize(vs), orthogonalize(vs, f): an orthonormal basis of the span of the vectors vs (Gram-Schmidt)"),
    ("euclidean_distance", "euclidean_distance(u, v): the Euclidean distance of two vectors"),
    ("range_space", "range_space(m): a basis of the span of the columns of the exact matrix m"),
    ("integer_partitions", "integer_partitions(n), integer_partitions(n, k), integer_partitions(n, k, ps, m): the partitions of n"),
    ("cellular_automaton", "cellular_automaton(rule, init, t): the evolution of a one-dimensional cellular automaton"),
    ("bell_y", "bell_y(n, k, xs), bell_y(n, xs): the partial (complete) Bell polynomial in the xs"),
    ("cube_root", "cube_root(x): the real cube root of the real number x"),
    ("real_exponent", "real_exponent(x), real_exponent(x, b): the logarithm of |x| to base b (10) as a machine number"),
    ("number_decompose", "number_decompose(x, [u1, ..., un]): x written greedily as counts of the units u1, ..., plus a remainder"),
    ("integer_string", "integer_string(n), integer_string(n, b), integer_string(n, b, len): the digits of |n| in base b as a string"),
    ("indexed", "indexed(x, i), indexed(x, [i1, i2, ...]): the part of the list x at the index (path); unevaluated for a symbol x"),
    ("integer_name", "integer_name(n), integer_name(n, \"Words\"): the English name of the integer n"),
    ("find_sequence_function", "find_sequence_function(xs, n): a closed form in n (polynomial, rational, factorial, Fibonacci or geometric factor) that gives the terms xs"),
    ("linear_optimization", "linear_optimization(f, cons, vars): the point [x -> ..] minimizing the linear f under the constraints"),
    ("linear_fractional_optimization", "linear_fractional_optimization(f, cons, vars): the point minimizing a ratio of linear functions under the constraints"),
    ("convex_optimization", "convex_optimization(f, cons, vars): the point minimizing the convex f under the constraints"),
    ("prime_zeta_p", "prime_zeta_p(s): the sum of p^-s over the primes p, numerically for inexact s with re(s) > 1"),
    ("unit_vector", "unit_vector(n, k), unit_vector(k): the vector of length n with a 1 at position k (the 2-vector for k = 1 or 2)"),
    ("angle_vector", "angle_vector(phi), angle_vector([r, phi]), angle_vector([r, theta, phi]), angle_vector(c, p): the unit vector (polar or spherical point) at the angles, relative to c"),
    ("vector_angle", "vector_angle(u, v): the angle acos(u . v/(|u| |v|)) between two real vectors"),
    ("manhattan_distance", "manhattan_distance(u, v): the sum of the absolute differences of the entries of u and v"),
    ("from_polar_coordinates", "from_polar_coordinates([r, t1, ..., tk]): the Cartesian point of hyperspherical coordinates; a list of points maps"),
    ("symmetric_matrix_q", "symmetric_matrix_q(m): whether the square matrix m equals its transpose (inexact entries up to rounding)"),
    ("positive_definite_matrix_q", "positive_definite_matrix_q(m): whether the Hermitian matrix m is positive definite (Sylvester's criterion)"),
    ("permanent", "permanent(m): the permanent of the matrix m (Ryser's formula)"),
    ("shearing_matrix", "shearing_matrix(theta, v, w): the matrix of the shear by the angle theta along the direction v normal to w"),
    ("scaling_matrix", "scaling_matrix([s1, ..., sn]), scaling_matrix(s, v): the diagonal scaling matrix, or the scaling by s along the direction v"),
    ("distance_matrix", "distance_matrix(points), distance_matrix(a, b): the matrix of Euclidean distances"),
    ("least_squares", "least_squares(m, b): the vector x minimizing |m . x - b| (of least norm when m is rank deficient)"),
    ("drazin_inverse", "drazin_inverse(m): the Drazin inverse of the square matrix m"),
    ("disk_matrix", "disk_matrix(r), disk_matrix(r, d): the array of ones inside the disk (ball, ellipsoid) of radius r, zeros outside"),
    ("diamond_matrix", "diamond_matrix(r), diamond_matrix(r, d): the array of ones inside the diamond of radius r, zeros outside"),
    ("box_matrix", "box_matrix(r), box_matrix(r, d): the array of ones of size 2 r + 1 per dimension"),
    ("cross_matrix", "cross_matrix(r), cross_matrix(r, d): the array of ones along the axes through the centre"),
    ("gaussian_matrix", "gaussian_matrix(r), gaussian_matrix([r, s]): the (2 r + 1) x (2 r + 1) Gaussian kernel of standard deviation r/2 (s), total 1"),
    ("mixed_fraction_parts", "mixed_fraction_parts(x): [ip, fp], the integer and fractional parts of x with the sign of x (real and imaginary parts apart for complex x)"),
    ("perfect_number", "perfect_number(k): the k-th perfect number 2^(p - 1) (2^p - 1), p the k-th Mersenne prime exponent (k up to 51)"),
    ("rudin_shapiro", "rudin_shapiro(n): (-1) to the number of occurrences of the binary digits 11 in n; maps over lists"),
    ("quadratic_irrational_q", "quadratic_irrational_q(x): whether x is an irrational root of a quadratic polynomial with integer coefficients"),
    ("discrete_indicator", "discrete_indicator(x, x0, domain): 1 for x = x0, 0 for another element of the finite domain"),
    ("list_convolve", "list_convolve(ker, list), list_convolve(ker, list, q): the convolution of the list with the kernel (valid part, or cyclic with the kernel entry q on the first entry)"),
    ("cumulant", "cumulant(data, r): the r-th cumulant of the data from its raw moments"),
    ("array_rules", "array_rules(a), array_rules(a, v): the rules [i, j, ...] -> x of the entries different from v (0), then the rule for all other positions"),
    ("matrix_minimal_polynomial", "matrix_minimal_polynomial(m, x): the monic polynomial in x of least degree that vanishes at the square matrix m"),
    ("coefficient_arrays", "coefficient_arrays(polys, vars): the list of the arrays of the coefficients of degree 0, 1, 2, ... of the polynomial(s) in the variables"),
    ("pfaffian_det", "pfaffian_det(m): the Pfaffian of the antisymmetric matrix m"),
    ("principal_components", "principal_components(data): the rows of the data matrix in the coordinates of its principal axes (mean subtracted)"),
    ("karhunen_loeve_decomposition", "karhunen_loeve_decomposition(data): [transformed, basis] of the Karhunen-Loeve transform of the rows of the data matrix"),
    ("set_accuracy", "set_accuracy(x, a): x with its precision set so that its accuracy is a digits"),
    ("interval_member_q", "interval_member_q(i, x), interval_member_q(i)(x): whether the number (interval) x lies in the interval i"),
    ("interval_union", "interval_union(i1, i2, ...): the union of the intervals as one interval"),
    ("knapsack_solve", "knapsack_solve(items, cap): the counts of each item (a cost, [payoff, cost] or [payoff, cost, max count]) that maximize the payoff within the cost cap"),
    ("quadratic_optimization", "quadratic_optimization(f, cons, vars), quadratic_optimization([q, c], [a, b]): the point minimizing the convex quadratic f under the constraints"),
    ("conic_optimization", "conic_optimization(f, cons, vars): the point minimizing f under linear, quadratic-bound and cone constraints (norm bounds, NormCone, SemidefiniteCone)"),
    ("second_order_cone_optimization", "second_order_cone_optimization(f, cons, vars): the point minimizing f under linear and norm(...) <= t constraints"),
    ("semidefinite_optimization", "semidefinite_optimization(f, cons, vars), semidefinite_optimization(c, [a0, ..., an]): the point minimizing f with a matrix constrained positive semidefinite"),
    ("robust_convex_optimization", "robust_convex_optimization(f, for_all(params, region, cons), vars): the minimizer for which cons holds for all parameters in a box region"),
    ("parametric_convex_optimization", "parametric_convex_optimization(f, cons, vars, params): the function of the parameters giving the minimizer; a fifth argument selects the property"),
    ("champernowne_number", "champernowne_number(b), champernowne_number(): the base-b number 0.123456789(10)(11)... whose digits are the integers one after another (10 by default); numerical through n"),
];

/// Natives that the syntax lowers to (definitions, blocks, control, bindings) or that serve other natives: not
/// functions a program calls by name, so the documentation leaves them out.
pub const INTERNAL: &[&str] = &[
    "assign",
    "c30_fox_machine", "c30_julia_counts", "nl_fft_machine", "nl_total_compensated",
    "ball_literal",
    "block",
    "check_options",
    "dataset_rows",
    "define",
    "define_fn",
    "define_rule",
    "define_sym",
    "fn",
    "for",
    "iadd",
    "if",
    "imod",
    "imul",
    "interpolated",
    "iquo",
    "isub",
    "let",
    "local_cell",
    "match",
    "option_value",
    "set",
    "set_delayed",
    "var",
    "while",
];

/// A `FUNCTIONS` list: names and one-line descriptions.
type Functions = &'static [(&'static str, &'static str)];

/// The libraries: topic, area and `FUNCTIONS` list, in the order the guides show the topics.
fn sources() -> Vec<(&'static str, Area, Functions)> {
    vec![
        ("Language", Area::Core, LANGUAGE),
        ("Language", Area::Core, PRELUDE),
        ("Lists", Area::Core, mc_lists::FUNCTIONS),
        ("Associations and sets", Area::Core, mc_assoc::FUNCTIONS),
        ("Language library", Area::Core, mc_langlib::FUNCTIONS),
        ("Number sequences", Area::SymbolicNumeric, mc_combinat::FUNCTIONS),
        ("Further functions", Area::SymbolicNumeric, mc_further_functions::FUNCTIONS),
        ("Differential algebra", Area::Higher, mc_diffalg::FUNCTIONS),
        ("Strings", Area::Core, mc_strings::FUNCTIONS),
        ("Session", Area::Core, mc_engine::FUNCTIONS),
        ("Files", Area::Data, mc_io::FUNCTIONS),
        ("Random numbers", Area::Data, mc_random::FUNCTIONS),
        ("Distributions and statistics", Area::Data, mc_dist::FUNCTIONS),
        ("Graphics", Area::Visualisation, mc_graphics_kernel::FUNCTIONS),
        ("Plots", Area::Visualisation, mc_plot::FUNCTIONS),
        ("Plots", Area::Visualisation, mc_kernel::front_end::FUNCTIONS),
        ("Calculus", Area::SymbolicNumeric, mc_calculus::FUNCTIONS),
        ("Sums and recurrences", Area::SymbolicNumeric, mc_sums::FUNCTIONS),
        ("Polynomials", Area::SymbolicNumeric, mc_poly::FUNCTIONS),
        ("Algebraic numbers", Area::SymbolicNumeric, mc_algebra::FUNCTIONS),
        ("Simplification", Area::SymbolicNumeric, mc_simplify::FUNCTIONS),
        ("Equations", Area::SymbolicNumeric, mc_solve::FUNCTIONS),
        ("Simplification", Area::SymbolicNumeric, mc_decide::FUNCTIONS),
        ("Quantifiers and real algebraic sets", Area::SymbolicNumeric, mc_cad::FUNCTIONS),
        ("Numbers and precision", Area::SymbolicNumeric, mc_numeric::FUNCTIONS),
        ("Root finding", Area::SymbolicNumeric, mc_roots::FUNCTIONS),
        ("Numerical integration", Area::SymbolicNumeric, mc_quad::FUNCTIONS),
        ("Numerics library", Area::SymbolicNumeric, mc_quad::LIBRARY_FUNCTIONS),
        ("Differential equations", Area::SymbolicNumeric, mc_ode::FUNCTIONS),
        ("Compiled functions", Area::SymbolicNumeric, mc_jit::FUNCTIONS),
        ("Number theory", Area::Higher, mc_numtheory::FUNCTIONS),
        ("Finite fields", Area::Higher, mc_fields::FUNCTIONS),
        ("Groups and combinatorics", Area::Higher, mc_groups::FUNCTIONS),
        ("Special functions", Area::Higher, mc_special::FUNCTIONS),
        ("Linear algebra", Area::Higher, mc_linalg::FUNCTIONS),
        ("Regions", Area::Higher, mc_geom::FUNCTIONS),
        ("Analytical mechanics", Area::Higher, mc_mechanics::FUNCTIONS),
    ]
}

/// Every function, in topic order, each name once (the first library to list a name keeps it).
#[derive(Clone, Debug)]
pub struct Catalogue {
    /// the functions
    pub entries: Vec<Entry>,
}

impl Catalogue {
    /// The catalogue of the libraries this program was built with.
    pub fn new() -> Catalogue {
        let mut seen = std::collections::HashSet::new();
        let mut entries = vec![];
        for (topic, area, list) in sources() {
            for &(name, summary) in list {
                if seen.insert(name) {
                    entries.push(Entry { name, summary, area, topic });
                }
            }
        }
        Catalogue { entries }
    }

    /// The function `name`.
    pub fn get(&self, name: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.name == name)
    }

    /// The topics of an area, in order, with their functions sorted by name.
    pub fn topics(&self, area: Area) -> Vec<(&'static str, Vec<&Entry>)> {
        let mut out: Vec<(&'static str, Vec<&Entry>)> = vec![];
        for e in self.entries.iter().filter(|e| e.area == area) {
            match out.iter_mut().find(|(t, _)| *t == e.topic) {
                Some((_, v)) => v.push(e),
                None => out.push((e.topic, vec![e])),
            }
        }
        for (_, v) in &mut out {
            v.sort_by_key(|e| e.name);
        }
        out
    }
}

impl Default for Catalogue {
    fn default() -> Catalogue {
        Catalogue::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_area_has_functions() {
        let c = Catalogue::new();
        for a in Area::ALL {
            assert!(!c.topics(a).is_empty(), "{a:?}");
            assert_eq!(Area::from_key(a.key()), Some(a));
        }
        assert_eq!(c.get("n_integrate").map(|e| e.area), Some(Area::SymbolicNumeric));
        assert_eq!(c.get("table").map(|e| e.topic), Some("Lists"));
        assert_eq!(c.get("plot").map(|e| e.area), Some(Area::Visualisation));
        let mut names: Vec<_> = c.entries.iter().map(|e| e.name).collect();
        let n = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), n);
    }

    /// Every function the engine knows from a library is in the catalogue (a library added to the engine but not
    /// here would make its functions invisible in the documentation).
    #[test]
    fn the_catalogue_covers_the_engine() {
        let c = Catalogue::new();
        let mut engine = mc_engine::CoreEngine::new();
        let missing: Vec<String> = engine
            .name_kinds()
            .into_iter()
            .filter(|(n, k)| *k == mc_engine::NameKind::Function && c.get(n).is_none() && !n.contains('.'))
            .map(|(n, _)| n)
            .filter(|n| engine.has_native(n))
            .collect();
        let unexpected: Vec<_> = missing.iter().filter(|n| !INTERNAL.contains(&n.as_str())).collect();
        assert!(unexpected.is_empty(), "functions the catalogue does not list: {unexpected:?}");
        // and the language list names only functions that exist
        for (n, _) in LANGUAGE {
            assert!(engine.has_native(n), "{n}");
        }
        // and the prelude list names only functions the prelude defines
        let kinds = engine.name_kinds();
        for (n, _) in PRELUDE {
            assert!(
                kinds.iter().any(|(k, kind)| k == n && *kind == mc_engine::NameKind::Function),
                "prelude function {n} is not defined"
            );
        }
    }
}
