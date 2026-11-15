# The query command

`constructo query` extracts data from a stream of values and transforms it, with a Rust-like expression over
`#values`, the stream of the input values. This document says what it reads, what the expression can say, what it
writes, and what its exit code means. The design is [decision 44][decision], and the details were settled in
[the task][task].

[decision]: ../decisions/0044-queries-are-rust-like-expressions-over-a-stream-of-values.md
[task]: ../tasks/2026-11-15-query-subcommand/query-subcommand.md

```text
constructo query [--compact] [-x] [--stdin] EXPR [FILES]...
constructo query [--compact] [--type=TYPE] [--project=FOLDER] [OPTIONS] EXPR [FILES]...
```

For example:

```text
constructo query '#values.map(|m| m.x)' measurements.values
constructo query '#values.filter_match(|Move { x, .. }| x)' moves.values
constructo query '#values.map(|c| c.update(|c| c.server.port = 8080))' config.values
constructo query --type=Config '#values.map(|c| Config { port: 8080, ..c })' config.types config.values
```

## Inputs

`EXPR` is the query. The inputs are the `FILES`, in order, and then the standard input if `--stdin` is given, named
`<stdin>`. A run needs at least one of `FILES`, `--stdin` and `--project`.

A query is **typed** when it has types: a `--project`, a `--type`, or a types file among the `FILES`, which is told by
the first token of each file. Otherwise it is **untyped**. The options for the types (`--project`, `--no-prelude`,
`--no-widening` and the others of `check`) are refused, with exit code 2, by an untyped query, since they would do
nothing.

- **Untyped,** the inputs are read lazily: a file is only read, and parsed, when `#values` reaches it, so that
  `#values.take(1)` stops reading early, and a later file that cannot be read is never noticed.
- **Typed,** the inputs are those of [the check command](check-command.md), read in the same way. The types and every
  value are checked, as `check` checks them, before the query runs, and the first error ends the run. With
  `--type=TYPE`, every value is checked against the type, which is any type of the root project, such as `Shape`,
  `[Shape]` or `Option<Shape>`, as for `convert --from=json-typed`. Without it, a value has the type that the check
  infers for it, so that `Pair(1)` of `enum Pair<T> = Pair(T, other: T);` is a `Pair<Nat>`, whose omitted `other` is
  `0`, and `[1, 2.5]` is a `[Float]`. A type that the value leaves open, such as the `T` of `None`, stays unknown.

The query is read and checked before any input.

## The language

A query is an expression. It is Rust's expressions with the changes that decision 44 makes; where it deviates from
Rust, the deviation has a name of its own. How a query is written is given by the
[query grammar](../../syntax/grammar.md#query-grammar), whose examples are in `syntax/examples/query/`.

- **`#values`** is a lazy iterator over the values of every input, in order. It may be used once, and not inside a
  closure.
- **Iterators** have Rust's adaptors, with Rust's meanings: `map`, `filter`, `filter_map`, `flat_map`, `take`,
  `skip`, `nth`, `next`, `last`, `count`, `collect`, `any`, `all` and `find`. `next`, `nth`, `last` and `find` give an
  `Option`, `Some(v)` or `None`. `collect` always makes an array.
- **`filter_match`** is our own: its closure's parameter is a pattern that may not match, and a value that does not
  match is dropped. In every other method, a closure's parameter must always match, as in Rust.
- **Values.** `v.x` is a field of a record, bare or as a constructor's body, or a named argument of a tuple. `v.0` is
  an argument by position. `v[i]` is an element of an array, and an error when out of range. Arrays have `len`,
  `is_empty`, `first`, `last` and `get(i)`, which give an `Option`, and `iter`, as an array is not an iterator.
  Options have `is_some`, `is_none` and `unwrap_or`.
- **Strings** have `len` (in bytes, as in Rust), `is_empty`, `contains`, `starts_with`, `ends_with`, `trim`,
  `to_lowercase`, `to_uppercase`, `split`, and `char_strings`, which gives the characters as strings, since
  Constructo has no character type yet. `chars` is reserved for that type. `+` joins strings.
- **Records and constructors** are written as values are: `{ x: 1, y: 2 }`, `Point { x: 1, y: 2 }`, `Write("hi")`,
  `Users[u1, u2]`. A bare record names its fields; field shorthand, `Point { x, y }`, is allowed only after a
  constructor name, as in Rust. Struct update keeps the rest: `Config { port: 8080, ..c }`, `{ b: 3, ..r }`.
- **`update`** is copy and assign: `c.update(|c| c.server.tls.port = 443)` is `c` with one place changed. The place is
  anything that the access syntax reaches from the parameter. `+=` and the other compound assignments follow the rules
  for arithmetic. Several changes chain: `c.update(|c| c.a = 1).update(|c| c.b = 2)`. A field is never deleted.
- **`if`** has an outfix form, `if c then a elseif d then b else e endif`, and Rust's form,
  `if c { a } else if d { b } else { e }`. `else` is required. In the outfix form every brace is a record.
- **`match`** has an outfix form, `match v with ARMS endmatch`, and Rust's form, `match v { ARMS }`. An arm is
  `pattern => expression`, with an optional `if` guard, separated by commas. Patterns have `_`, bindings, `name @ p`,
  `p | q`, literals, constructors with bodies and `..`, records and arrays. The `..` that ends an array, bare or as a
  constructor's body, may bind the rest as an array, as a slice pattern does in Rust: `[first, rest @ ..]`.
  `matches!(v, pattern)` is a Bool.
- **Operators** are Rust's, with Rust's precedence: `||`, `&&`, `!`, the comparisons, which do not chain, `+`, `-`,
  `*`, `/`, `%`, and `as Nat`, `as Int` or `as Float`. `&&`, `||` and `!` need Bools: there is no truthiness. `==`
  compares whole values; `<` and the others compare numbers and strings only.
- **Braces** are never blocks. It is always possible to write a query in which braces only denote records.

### Numbers

- The operands of arithmetic and of a comparison must be numbers of one kind. A literal adapts to the kind it meets,
  so `m.count + 1` and `m.y > 0.0` work, but values do not: a `Nat` compared with a `Float` needs `as Float`.
- `as` to the same kind does nothing. A narrowing cast that would change the number, such as `-1 as Nat` or
  `1.5 as Int`, is an error; one that would not, such as `5.0 as Nat`, is allowed. `as Float` rounds to the nearest
  `f64`, and is an error only beyond its range.
- `Nat` and `Int` are unbounded. A `Nat` below zero is an error. `/` truncates, `%` is the remainder, and division by
  zero is an error. `Float` arithmetic is `f64`, and a literal too large for one is an error.
- Comparison is exact on the text of the numbers, so `1.50 == 1.5`.
- A new `Nat` is plain digits, a new `Int` always has a sign, and a new `Float` is the shortest text that reads back
  as the same `f64`, always with a `.`. A value that is not changed keeps its text.

### Types

Untyped means only that there are no types: the expression is still type-checked. What the expression fixes is
checked before any value is read, so `if v.ok then 1 else "failed" endif` is refused. What comes from the data is
checked as each value arrives, by its kind, and a mismatch is an error at that value. Without the types, constructor
names match by their exact text, a named argument that is not written by name, a const, and the default of an omitted
argument cannot be known, and asking for one is an error that says to supply the types.

Typed, a named argument is found however it was written, an omitted one takes its default, a const is seen into, a
number takes the kind of its place, so a `5` in a `Float` field is a `Float`, and the constructor names of the query
are resolved as a values file of the root project would resolve them, so an unknown one is an error. `update` checks
that the new value fits, and an omitted argument that it changes is written by name.

A typed query may also name a const of the types in an expression, as `origin` or `shapes::origin`: a name that no
pattern binds is a const, and a binding hides a const of the same name, so `#values.map(|origin| origin)` gives each
value. A name that is neither is an error, `oops is not bound by any pattern, nor a const of the types`. A pattern
never compares with a const: a lower-case name in a pattern always binds, so that a pattern means the same typed and
untyped, and a const is compared in a guard, `match p { q if q == origin => ..., _ => ... }`. Untyped, a name that no
pattern binds is an error that says to supply the types. `true`, `false` and `null` are values of the query in both.

With `--type`, more is checked
before the query runs on any value: a field or parameter that no constructor of the type has, a pattern whose constructor is of
another type, a new value that cannot fit where `update` puts it, and a `match` that does not cover every constructor
of its enum.

Checked and unchecked processing give the same result whenever both succeed, given types that agree with the
distinctions that the text shows; either may fail where the other succeeds.

## Output

If the result is an iterator, each item is written as a value, as it is made; otherwise the one result is written. A
value is written as `convert --from` writes values: on one line if that is at most 80 characters wide, and spread over
lines otherwise, or all on one line with `--compact`. Typed, a name is written as the source of its value writes it,
and a name that the query makes as a values file of the root project would write it.

## Errors and exit codes

A diagnostic is written as `ERROR: <message> (<file>, line L, col C)`. A mistake in the query is in `<query>`, at its
position in the expression.

- **Exit code 0:** the query ran to the end. No output is not an error.
- **Exit code 1:** an error in the inputs: a syntax error, a types file among the values of an untyped query, and,
  typed, the errors of `check`; and an error at a value as the query runs, such as a missing field, an index out of
  range, a number of the wrong kind, a `Nat` below zero, or a value that no arm of a `match` matches. The values
  before an error at a value have been written; typed, an error in the inputs is found before anything is written.
- **Exit code 2:** a mistake in the query. A syntax error, a name that no pattern binds, a closure's parameter that
  may not match and branches with no common type are found before any input is read. Typed, an unknown constructor, a
  `--type` that is not a type, and what `--type` lets the types find are found once the inputs have been read and
  checked, before the query runs on any value. Also an option or an argument that is not
  understood, an option for the types in an untyped query, an input that cannot be read, and an output that cannot be
  written, as for `convert`.

See [the exit codes](exit-codes.md).
