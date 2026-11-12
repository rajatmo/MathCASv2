#! mathcas 2
# Round-trip corpus of whole programs: statements over several lines, new lines as separators, the tour of
# 04-syntax.md section 1. The file is read as one program.

let f = x => x^3 - 2x                        # lambda; a numeric literal before a name multiplies
diff(f(x), x)
range(5) |> map(k => k^2, _) |> sum

{ let y = 2; let z = y + 1; y*z }

fn fact(0) = 1
fn fact(n: Int) when n > 0 = n*fact(n - 1)
fact(-1)

rule fib(0) = 0
rule fib(1) = 1
rule fib(n_: Int) := fib(n) = fib(n - 1) + fib(n - 2)

@associative @commutative @identity(0)
sym oplus

fn plot_points(held e, held v: Sym, a: Real, b: Real) = e

n(pi, 50)
1.25~40
2 + 3im

assuming(x > 0, simplify(sqrt(x^2)))
rewrite(e, [sin(a_)^2 + cos(a_)^2 -> 1])

module Geo {
  fn sq(x) = x*x
  pub fn area(r: Real): Real = pi*sq(r)
}
import Geo
area(1)

# multi-line blocks, control flow and pipes
{
  var total = 0
  for k in range(10) {
    total = total + k
  }
  total
}

let result = range(100)
  |> select(&(&1 > 3), _)
  |> map(x => x^2, _)

match shape {
  circle(r_) => pi*r^2,
  square(a_) when a > 0 => a^2
  _ => 0
}

if x > 0 then
  1
else
  -1

f(a,
  b,
  c)

@on(square)
rule area(square(a_)) := a^2

var i = 0
while i < 10 { i = i + 1 }
i = 0
