# Morty

Morty reads SystemVerilog files and pickles them into a single file for easier handling. Optionally it allows to re-name modules with a common prefix or suffix. This allows for easier management of larger projects (they just become a single file). By making them unique they can also depend on different versions of the same dependency without namespace clashes.
