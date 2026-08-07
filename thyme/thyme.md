Obtained through trial and error using the `infix` statement.

| Precedence |  Associativity/form  | Operators               |   |   |
| ---------: | :------------------: | ----------------------- | - | - |
| infinity+1 | left | `a.b.c` |   |   |
| infinity | left | `f x`, `f(x)` |   |   |
|         14 |         right        | `^`                     |   |   |
|         13 |        prefix        | `!` unary `-` unary `+` |   |   |
|         12 |         left         | `*` `/` `%`             |   |   |
|         11 |         left         | binary `+` binary `-`   |   |   |
|         10 |         left         | `++`                    |   |   |
|          9 |         left         | `<` `<=` `>` `>=`       |   |   |
|          8 |         left         | `==` `!=`               |   |   |
|          5 |         left         | `..` (defined in  `thyme.cfg`)                   |   |   |
|          4 |         left         | `&&`                    |   |   |
|          3 |         left         | `\|\|`                    |   |   |
|          2 |         right        | `?:`                    |   |   |
|    below 0 | special/right-greedy | `->`                    |   |   |
| below `->` |         right        | `=` `:=`                |   |   |
