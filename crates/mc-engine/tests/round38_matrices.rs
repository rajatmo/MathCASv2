//! Matrix predicates and named matrices, bit operations, permutations of lists, distances and the characteristic
//! values of the odd Mathieu functions. Each result is checked by a second route: a definition evaluated directly,
//! an identity, or an eigenvalue of a truncated matrix.
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

fn ev(src: &str) -> String {
    let stop = StopFlag::new();
    let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
    match e.eval(src, &stop) {
        Ok(v) => v.printed,
        Err(err) => format!("error: {err}"),
    }
}

#[test]
fn hermitian_and_diagonalizable_matrices() {
    assert_eq!(ev("[hermitian_matrix_q(pauli_matrix(2)), hermitian_matrix_q([[1, 2], [3, 4]]), hermitian_matrix_q([[1., 0.3*im], [-0.3*im, 2.]])]"), "[true, false, true]");
    // a Jordan block is not diagonalizable; distinct eigenvalues and a multiple of the identity are
    assert_eq!(ev("[diagonalizable_matrix_q([[6, 0, 0], [0, 12, 1], [0, 0, 12]]), diagonalizable_matrix_q([[1, 2], [3, 4]]), diagonalizable_matrix_q(3*identity_matrix(3))]"), "[false, true, true]");
    // a conjugate of a Jordan block by an invertible matrix keeps the defect; of a diagonal matrix stays diagonalizable
    assert_eq!(ev("{ let p = [[1, 2, 0], [0, 1, 1], [1, 0, 1]]; [diagonalizable_matrix_q(dot(p, [[2, 1, 0], [0, 2, 0], [0, 0, 5]], inverse(p))), diagonalizable_matrix_q(dot(p, [[2, 0, 0], [0, 2, 0], [0, 0, 5]], inverse(p)))] }"), "[false, true]");
    // symbolic entries: a rotation generator over the complex numbers
    assert_eq!(ev("diagonalizable_matrix_q([[0, -a], [a, 0]])"), "true");
}

#[test]
fn pauli_and_fourier_matrices() {
    assert_eq!(ev("pauli_matrix(2)"), "[[0, -1im], [1im, 0]]");
    // the commutation relations [sigma_j, sigma_k] = 2 i eps_jkl sigma_l
    assert_eq!(ev("dot(pauli_matrix(2), pauli_matrix(3)) - dot(pauli_matrix(3), pauli_matrix(2)) == 2*im*pauli_matrix(1)"), "true");
    assert_eq!(ev("dot(pauli_matrix(3), pauli_matrix(1)) - dot(pauli_matrix(1), pauli_matrix(3)) == 2*im*pauli_matrix(2)"), "true");
    // the matrix of the transform agrees with fourier for other sizes and parameters
    assert_eq!(ev("chop(n(dot(fourier_matrix(4), [1, 2, 3, 4])) - fourier([1., 2, 3, 4]))"), "[0, 0, 0, 0]");
    assert_eq!(ev("chop(n(dot(fourier_matrix(6, fourier_parameters: [1, -1]), [1, -2, 0, 3, 5, 1])) - fourier([1., -2, 0, 3, 5, 1], fourier_parameters: [1, -1]))"), "[0, 0, 0, 0, 0, 0]");
    assert_eq!(ev("norm(n(dot(fourier_matrix(5), conjugate(transpose(fourier_matrix(5))))) - identity_matrix(5)) < 1e-14"), "true");
}

#[test]
fn bit_operations() {
    assert_eq!(ev("[bit_set(42, 9), bit_clear(63, 3), bit_shift_left(2^200 - 1, 7) == (2^200 - 1)*128]"), "[554, 55, true]");
    // against the bit lists of the numbers
    assert_eq!(ev("from_digits(reverse(replace_part(reverse(integer_digits(1000, 2, 16)), 6 -> 1)), 2) == bit_set(1000, 5)"), "true");
    assert_eq!(ev("[bit_clear(-1, 4), bit_and(-1, bit_not(16)), bit_set(-32, 0), -31]"), "[-17, -17, -31, -31]");
    assert_eq!(ev("[bit_shift_left([1, 2, 4, 16], 2), bit_shift_left(-5, 3), bit_shift_left(100, -2), bit_shift_right(100, 2)]"), "[[4, 8, 16, 64], -40, 25, 25]");
}

#[test]
fn perfect_numbers_and_palindromes() {
    assert_eq!(ev("perfect_number_q(496)"), "true");
    // the even perfect numbers 2^(p-1) (2^p - 1) of the Mersenne primes 2^p - 1, and none in between
    assert_eq!(ev("map(p => perfect_number_q(2^(p - 1)*(2^p - 1)), [2, 3, 5, 7, 13, 11])"), "[true, true, true, true, true, false]");
    assert_eq!(ev("select(range(10000), perfect_number_q) == select(range(10000), k => total(most(divisors(k))) == k)"), "true");
    assert_eq!(ev("[palindrome_q(12321), palindrome_q(\"abca\"), palindrome_q([1, 2, 1]), palindrome_q(10)]"), "[true, false, true, false]");
}

#[test]
fn permutations_move_elements() {
    assert_eq!(ev("fold(permute, [a, b, c, d], [[1, 4, 3, 2], [3, 2, 4, 1]])"), "[b, d, a, c]");
    // permuting by p, then by the inverse of p, gives the list back
    assert_eq!(ev("{ let p = [4, 1, 5, 2, 3]; permute(permute([a, b, c, d, e], p), ordering(p)) }"), "[a, b, c, d, e]");
    // the cycles form gives the result of its permutation list; positions moved by the cycle only
    assert_eq!(ev("permute(range(26), cycles([[1, 13, 25]]))[[1, 13, 25]]"), "[25, 1, 13]");
    assert_eq!(ev("permute([a, b, c, d], cycles([[1, 2], [3, 4]])) == permute([a, b, c, d], [2, 1, 4, 3])"), "true");
}

#[test]
fn distances() {
    assert_eq!(ev("[edit_distance([0, 1, 1, 0, 0, 1, 0], [1, 1, 0, 0, 1, 1, 0]), edit_distance(\"bccbbcb\", \"ccbcccb\")]"), "[2, 3]");
    // insertions and deletions only: the lengths minus twice the longest common subsequence bound the distance
    assert_eq!(ev("[edit_distance(\"kitten\", \"sitting\"), edit_distance(\"\", \"abc\"), edit_distance(\"abc\", \"abc\"), edit_distance(\"flaw\", \"lawn\")]"), "[3, 3, 0, 2]");
    assert_eq!(ev("[chessboard_distance([1.5, 0.6], [0, 0]), chessboard_distance([1, 5, 2], [4, 1, 0]), squared_euclidean_distance([1, 5, 2], [4, 1, 0]) == euclidean_distance([1, 5, 2], [4, 1, 0])^2]"), "[1.5, 4, true]");
}

#[test]
fn polar_coordinates_invert_the_cartesian_map() {
    assert_eq!(ev("to_polar_coordinates([1, 2])"), "[5^(1/2), atan(2)]");
    assert_eq!(ev("chop(n(from_polar_coordinates(to_polar_coordinates([3, -4, 12]))) - [3, -4, 12])"), "[0, 0, 0]");
    assert_eq!(ev("chop(n(from_polar_coordinates(to_polar_coordinates([-1, 2, -3, 0.5]))) - [-1, 2, -3, 0.5])"), "[0, 0, 0, 0]");
}

#[test]
fn mathieu_characteristic_values_of_the_odd_functions() {
    // b_2(1.5): the eigenvalue of the truncated tridiagonal matrix of the coefficients B_(2m+2) (DLMF 28.4.8)
    assert_eq!(ev("{ let q = 1.5; let m = table(if i == j then (2*i)^2 else if abs(i - j) == 1 then q else 0, [i, 1, 30], [j, 1, 30]); abs(min(eigenvalues(m)) - mathieu_characteristic_b(2, q)) < 1e-12 }"), "true");
    // b_4(7.25), the second eigenvalue of that matrix
    assert_eq!(ev("{ let q = 7.25; let m = table(if i == j then (2*i)^2 else if abs(i - j) == 1 then q else 0, [i, 1, 40], [j, 1, 40]); abs(sort(eigenvalues(m))[2] - mathieu_characteristic_b(4, q)) < 1e-10 }"), "true");
    // b_3(-2) through DLMF 28.2.26 b_(2n+1)(q) = a_(2n+1)(-q): the matrix of DLMF 28.4.7 with first entry 1 - q
    assert_eq!(ev("{ let q = -2.; let m = table(if i == j then (2*i - 1)^2 - (if i == 1 then q else 0) else if abs(i - j) == 1 then q else 0, [i, 1, 30], [j, 1, 30]); abs(sort(eigenvalues(m))[2] - mathieu_characteristic_b(3, q)) < 1e-11 }"), "true");
    // the small-q series b_2 = 4 - q^2/12 + 5 q^4/13824 (DLMF 28.6.18) and the period of the odd function
    assert_eq!(ev("abs(mathieu_characteristic_b(2, 0.01) - (4 - 0.01^2/12 + 5*0.01^4/13824)) < 1e-15"), "true");
    assert_eq!(ev("{ let b = mathieu_characteristic_b(2, 3.); abs(mathieu_s(b, 3., 1.2) - mathieu_s(b, 3., 1.2 + pi)) < 1e-10 }"), "true");
    assert_eq!(ev("[mathieu_characteristic_b(3, 0), mathieu_characteristic_b(2.5, 1.5) == mathieu_characteristic_a(2.5, 1.5)]"), "[9, true]");
    assert_eq!(ev("mathieu_characteristic_b(0, 1.5)"), "mathieu_characteristic_b(0, 1.5)");
}
