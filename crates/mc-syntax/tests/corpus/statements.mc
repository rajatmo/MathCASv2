#! mathcas 2
# Round-trip corpus: every non-empty line that is not a comment is one statement. Each must parse, lower, print,
# and read back to the identical term (tests/corpus.rs).

# numbers
42
-7
1_000_000
0x1F
0b1011
0xFFFF_FFFF_FFFF_FFFF_FFFF
123456789012345678901234567890123456789
-123456789012345678901234567890123456789
-9223372036854775808
9223372036854775807
1.5
1.
2e-10
-0.0
1e300
6.02214076e23
1.25~40
2~100
-1.25~40
1e-5~30
3im
1.5im
-3im
2 + 3im
2 - 3im
1.5~20im
3/4
-3/4
6/4
4/2
0/5
5/0
3/-4
1/2/3
(1/2)/3
x/4
3*4^-1
-(3/4)
-(-7)

# strings and names
"hello"
"tab\tnewline\nquote\"backslash\\"
"unicode \u{3b8} and θ"
""
r"$\alpha_1 \le \beta$"
r#"say "hi" \o/"#
r##"a "# b\"##
text(r"$\theta$", [0, 1])
x
α
θ1
_tmp
pi
π
∞
e
im
inf
true
false
null
Geo.area
`if`
`a b`
`x_`

# arithmetic and precedence
a + b
a - b
a + b - c + d
a*b
a/b
a*b/c*d
2x
2x^2
3(a + b)
2pi
-x
-x^2
(-x)^2
-a*b
a*-b
a^b^c
(a^b)^c
a^-1
a^-b
a^(1/2)
x^-2
(a + b)*(c - d)
(a + b) + c
a + (b + c)
a*(b*c)
(a*b)*c
a - (b + c)
a - (b - c)
a - 7
a + -7
a - -7
a - 3/4
a - b*c
a - 2y
a/(b*c)
a/b/c
(a/b)/c
1/x
-1*x*y
-1*7
x^2 + 2x*y + y^2
3x^2 - 2x + 1
(x + 1)^10
2^-3

# comparisons and logic
a == b
a != b
a < b
a <= b
a > b
a >= b
a === b
a !== b
a < b and b < c
a or b
a or b or c
a and b and c
a and (b or c)
(a and b) or c
not a
not a and b
not (a and b)
not a == b
not not a
(a == b) == c
x > 0 or x < -1

# rules, conditions, alternatives, patterns
a -> b
a :> b
a -> b -> c
(a -> b) -> c
x_ -> x^2
f(x_) :> g(x)
_
x_
xs__
xs___
__
___
x_: Int
xs__: Real
_: Str
x_: Int | Real
x_: List[Int]
x_: Array[Real, 2]
x_: Complex[Float]
x_: Head[f]
x_: (Int, Real) -> Bool
x_: Int where x > 0
x @ f(y_)
p @ (a_ | b_)
x_ when x > 0
f(x_) when x > 0
a_ | b_
0 | 1 | 2
f(x_ = 1)
f(x_: Int = 1, y_)
rewrite(e, [sin(a_)^2 + cos(a_)^2 -> 1])
cases(xs, _: Int)

# calls, lists, indexing, options
f()
f(x)
f(x, y, z)
f(a)(b)
(f + g)(x)
(x => x^2)(3)
[]
[1, 2, 3]
[[1, 2], [3, 4]]
xs[1]
xs[i, j]
xs[-1]
xs[2:5]
xs[i:j, 1]
m[1][2]
f(x)[1]
n(x, digits: 50)
n(pi, 50)
plot(f, x, from: 0, to: 1)
f(options(a -> 1))
f'
f''
f'''
f''''
y''(t)
derivative(5)(f)
derivative(1)(g(x))
f'(x) + g''(x)

# pipes
xs |> f
range(5) |> map(k => k^2, _) |> sum
a |> f(b, _)
x |> g(1)

# lambdas and slot functions
x => x^3 - 2x
(x, y) => x + y
() => 42
(x: Real) => x^2
(x, y: Int = 2) => x*y
(xs...) => length(xs)
(held e) => e
&(&1 + 1)
&(&1 + &2)
&(&2)
&(f(&1, &(&1*2)))
map(&(&1^2), xs)
x => y => x + y
f(x => x, y)

# blocks, let, var, control
{ let y = 2; let z = y + 1; y*z }
{ var i = 0; i = i + 1; i }
{ let x: Int = 3; x }
{ }
{ a; b; c }
if c then a else b
if c then a
if a then if b then x else y else z
if c then (if d then x) else y
if c then a else if d then b else e
(if c then a else b) + 1
while i < 10 { i = i + 1 }
for x in xs { print(x) }
for k in range(10) { s = s + k }
match e { 0 => 1, n_ => n*2 }
match e { x_ when x > 0 => 1, _ => 0 }
match e { 0 | 1 => true, _ => false }
match e { }
match p { [a_, b_] => a + b, f(x_) => x }

# definitions
fn fact(0) = 1
fn fact(n: Int) when n > 0 = n*fact(n - 1)
fn sq(x) = x*x
fn area(r: Real): Real = pi*sq(r)
fn n(x, d: Int = 16; guard: Int = 32, max_precision: Int = 100_000) = x
fn plot_points(held e, held v: Sym, a: Real, b: Real) = e
fn total(first, rest...) = first + sum(rest)
fn pick(x_: Int, [a_, b_]) = a
fn g(x) = y when y > 0
@listable fn h(x) = x^2
@associative @commutative @identity(0) sym oplus
@commutative sym otimes
@uses_assumptions @handles_failures fn refine2(x) = x
sym mysym
rule fib(0) = 0
rule fib(1) = 1
rule fib(n_: Int) := fib(n) = fib(n - 1) + fib(n - 2)
rule f(x_) := x^2 when x > 0
rule f(x_) when x < 0 := -x
@on(square) rule area(square(a_)) := a^2
@effect rule log_it(x_) := print(x)
x = 3
x := random()
f(x_) := x^2
let f = x => x^3 - 2x
var counter = 0
unset(x)

# modules and imports
module Geo { fn sq(x) = x*x; pub fn area(r: Real): Real = pi*sq(r) }
module M { pub sym s; pub x = 1; let y = 2 }
import Geo
import Geo.{area, sq}
import core except e
import core except e, im
assuming(x > 0, simplify(sqrt(x^2)))
