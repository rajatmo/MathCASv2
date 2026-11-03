#! mathcas 2
# Modules: `vectors` is the file vectors.mc next to this script; `stats` is a standard module built in.

import vectors
import stats.{mean, median}

say("e1 + 2 e2 = ", plus(e1, times(2, e2)))
say("The angle between e1 and [1, 1]: ", angle(e1, [1, 1]))

# a qualified name works too, for public members
say("Also qualified: ", vectors.times(3, [1, 2]))

# a module written in the script itself; `scale` is private to it
module temperature {
  pub fn to_kelvin(c) = c + scale
  scale = 27315/100
}

let readings = [21, 19, 24, 22, 23]
say("Mean and median: ", [mean(readings), median(readings)])
map(temperature.to_kelvin, readings)
