#! mathcas 2
#@ title: Flatten and transpose of nested lists
#@ stresses: structural rebuilding
#@ sizes: r = 100, 316, 1000
#@ check: property
#@ setup
mat = range(r) |> map(i => range(r) |> map(j => i*r + j, _), _)
#@ measure
flatten(transpose(mat))
#@ property
length(result) == r^2 and result[2] == mat[2, 1] and result[r + 1] == mat[1, 2]
