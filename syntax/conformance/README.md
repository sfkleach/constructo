# The conformance suite for runtimes

A runtime is the part of Constructo that a program in some language uses to read and write values: a lexer, a parser
and a printer of data-values. The Rust runtime is the crate `constructo-runtime`, which `constructo check` uses, and a
runtime for another language is written in that language. This folder says what every runtime has to give for each
input, so that they all read the same syntax.

A runtime conforms when it gives the expected result for every case.

## A case

A case is a values file, `NAME.values`, and the result that is expected for it, `NAME.expected`, beside it. The input
is read as a whole, as UTF-8, and parsed as a values file. Types files are not part of the suite.

The expected result is one of two things:

- **The values that the file holds**, each written on a line of its own, followed by `;` and a new line, in the
  compact form that `constructo convert --from` writes with `--compact`: a space after `:` and `,`, inside the braces
  of a record that has members, and between a name and the `{` of its record, and nowhere else, as in
  `Circle { radius: 3.0 }`, `{}`, `Items[1, 2]` and `Pair(1, b: 2)`. The text of each number and name is kept as it
  was written, so `007`, `1.50` and `-0.0` are written as they are, and a string is written with the escapes that the
  printer uses. An input with no values gives an empty result.
- **The first syntax error**, as one line:

  ```text
  ERROR: MESSAGE (NAME.values, line LINE, col COL)
  ```

  `NAME.values` is the name of the case's file, without its folder. Lines and columns count from 1, and a column
  counts characters, which are Unicode scalar values, and not bytes or UTF-16 units. A new line is `\n`, and a `\r`
  before it is a character of the line it ends.

## The folders

- `examples/` holds a copy of every values file of `syntax/examples/values`, in the same folders. A test checks that
  the copies are the same as the examples, so that the suite stays whole in itself and can be taken to another
  repository as it is.
- `limits/` holds the depth limit. A value may be nested 200 levels deep, counting each value, a scalar included, so
  `[[1]]` is three levels; a value nested 201 levels deep is refused with `Nesting is too deep`. There is a case of
  each for every way that one value can hold another.
- `lexer/` holds the cases that a runtime written by hand most often gets wrong: escapes, surrogates, signs and
  exponents, comments, line endings, and how columns are counted.

A case in a folder called `experimental`, at any depth, is read with every experimental feature of the syntax
switched on. A runtime that has no experimental features skips those cases.

## Making and changing the expected results

The expected results are written by the Rust runtime, which is the one that the others are held to:

```sh
CONSTRUCTO_BLESS=1 cargo test -p constructo-runtime --test conformance
```

Without `CONSTRUCTO_BLESS`, the same test compares the Rust runtime with them. A change to the syntax is a change to
the expected results, and shows in review as one.
