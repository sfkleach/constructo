# Task: Create Initial Documents for Constructo

`Constructo` is a typed, simple-object notation, inspired by JSON and the 
compact algebraic data type syntax of Rust. See the [overview](overview.md) 
document to get an understanding of what we are hoping to make.

This task aims to produce three key documents: the vision document of the
`constructo` tool, the specification of contructo data-types (aka schema),
and the specification of the constructo data-value -expressions. 

Outputs:
- The syntax of the data-values, expressed as a "tabadkins" grammar.
  - A template file `syntax/dt_grammar_tabatkins.txt` is provided for this
  - It contains a previous (irrelevant) example.
  - See `https://github.com/tabatkins/railroad-diagrams/blob/gh-pages/README-py.md`
    for the specific notation details.
- The syntax of the data-types, expressed the same way.
  - A template file `syntax/dv_grammar_tabatkins.txt` is provided for this
  - It contains a previous (irrelevant) example.
- A vision document 
  - it is a description of an imaginary scenario in which one or more people use
    the tool to perform tasks. The scenario typically runs through a
    user-journey and establishes the key capabilities of the tool.
  - See `the-constructo-tool.md` file.
- A set of examples of valid and invalid syntax.
