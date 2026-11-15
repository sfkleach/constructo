# Task: Add the query subcommand

## Overview

`docs/vision.md` lists, for later, selecting sub-values from a file or stdin. The design is settled in
[decision 44](../../../decisions/0044-queries-are-rust-like-expressions-over-a-stream-of-values.md): a query is a
Rust-like expression over `@values`, the lazy stream of the input values, which extracts data and transforms it with one
language. This task implements it, test first.

The working name of the command is `constructo query EXPR FILES`. The name and the options are among the questions to
settle below.

The decision record is the specification. This file adds the examples that drive the implementation, the steps, the
questions that are left open, and the notes from the exploration that led to the design.

## TDD examples

Each example is an input, an expression and the expected output. They become functional tests, and the smaller ones
unit tests too. The values are written as a values file would have them.

### Extraction (the user's list)

| # | Purpose | Input | Expression | Output |
|---|---|---|---|---|
| 1 | Pass the input through | `1; "a"; Quit;` | `@values` | `1; "a"; Quit;` |
| 2 | Delete the whole input | `1; 2; 3;` | `@values.take(0)` | nothing |
| 3 | The first item | `10; 20; 30;` | `@values.take(1)` | `10;` |
| 3a | The first item as an `Option` | `10; 20; 30;` | `@values.next()` | `Some(10);` |
| 3b | No first item | (empty) | `@values.take(1)` / `@values.next()` | nothing / `None;` |
| 4 | The 3rd item, if it exists | `10; 20; 30;` | `@values.skip(2).take(1)` | `30;` |
| 4a | The 3rd item as an `Option` | `10; 20; 30;` | `@values.nth(2)` | `Some(30);` |
| 4b | No 3rd item | `10; 20;` | `@values.skip(2).take(1)` / `@values.nth(2)` | nothing / `None;` |
| 5 | A field of records | `{ x: 1, y: 2 }; { x: 3, y: 4 };` | `@values.map(\|r\| r.x)` | `1; 3;` |
| 6 | A pair of fields | `{ x: 1, y: 2, z: 9 }; { x: 3, y: 4, z: 9 };` | `@values.map(\|r\| { x: r.x, y: r.y })` | `{ x: 1, y: 2 }; { x: 3, y: 4 };` |
| 7 | Lengths of arrays | `[1, 2]; []; [5, 6, 7];` | `@values.map(\|a\| a.len())` | `2; 0; 3;` |
| 8 | First elements, an error when empty | `[1, 2]; []; [5, 6, 7];` | `@values.map(\|a\| a[0])` | `1;` then an error at the empty array |
| 8a | First elements, skipping empty arrays | `[1, 2]; []; [5, 6, 7];` | `@values.filter_map(\|a\| a.first())` | `1; 5;` |
| 9 | Third elements | `[1, 2]; []; [5, 6, 7];` | `@values.filter_map(\|a\| a.get(2))` | `7;` |
| 10 | Fields from arrays of records, as one stream | `[Pair { left: true, right: false }, Pair { left: false, right: true }]; [];` | `@values.flat_map(\|ps\| ps.iter().map(\|p\| p.right))` | `false; true;` |
| 10a | The same, one array per value | as 10 | `@values.map(\|ps\| ps.iter().map(\|p\| p.right).collect())` | `[false, true]; [];` |
| 11 | Positional arguments | `[Record(true, right: false), Record(false, false)];` | `@values.flat_map(\|rs\| rs.iter().map(\|r\| r.0))` | `true; false;` |
| 11a | A named argument, untyped | as 11 | `@values.flat_map(\|rs\| rs.iter().map(\|r\| r.right))` | `false;` then an error at `Record(false, false)` saying to supply the types |
| 11b | A named argument, typed | as 11, with `enum R = Record(Bool, right: Bool);` | as 11a | `false; false;` |

### Patterns and constructors

| # | Purpose | Input | Expression | Output |
|---|---|---|---|---|
| 12 | `filter_match` skips other constructors | `Move { x: 1, y: 2 }; Quit; Move { x: 3, y: 4 };` | `@values.filter_match(\|Move { x, .. }\| x)` | `1; 3;` |
| 13 | Field shorthand in the result | as 12 | `@values.filter_match(\|Move { x, y, .. }\| { x, y })` | `{ x: 1, y: 2 }; { x: 3, y: 4 };` |
| 14 | `matches!` | as 12 | `@values.filter(\|v\| matches!(v, Quit)).count()` | `1;` |
| 15 | A refutable pattern in `map` is refused | as 12 | `@values.map(\|Move { x, .. }\| x)` | an error before any value: the parameter must always match |
| 16 | Untyped names match by exact text | `shapes::Circle { r: 1.0 }; Circle { r: 2.0 };` | `@values.filter_match(\|Circle { r }\| r)` | `2.0;` |

### Numbers

| # | Purpose | Input | Expression | Output |
|---|---|---|---|---|
| 17 | A literal adapts | `{ y: 1.5 }; { y: -2.0 };` | `@values.filter(\|m\| m.y > 0.0).count()` | `1;` |
| 18 | Mixed kinds need a cast, untyped | `{ y: 1.5 }; { y: 5 };` | `@values.filter(\|m\| m.y > 0.0).count()` | an error at `5`, suggesting `m.y as Float` |
| 19 | The cast works on every spelling | `{ y: 1.5 }; { y: 5 };` | `@values.filter(\|m\| m.y as Float > 0.0).count()` | `2;` |
| 20 | Typed, `5` in a `Float` field is a `Float` | as 18, with `y: Float` | as 18 | `2;` |
| 21 | A narrowing cast that changes the number | `-1;` | `@values.map(\|n\| n as Nat)` | an error at `-1` |
| 22 | `Nat` arithmetic | `{ port: 8080 };` | `@values.map(\|c\| c.port + 1)` | `8081;` |
| 23 | `Nat` below zero | `3;` | `@values.map(\|n\| n - 5)` | an error at `3` |
| 24 | `Int` results are signed | `3;` | `@values.map(\|n\| n as Int - 5)` | `-2;` |
| 25 | Integers are unbounded | `18446744073709551615;` | `@values.map(\|n\| n + 1)` | `18446744073709551616;` |
| 26 | Integer division truncates | `7;` | `@values.map(\|n\| n / 2)` / `@values.map(\|n\| n % 2)` | `3;` / `1;` |
| 27 | `Float` arithmetic is `f64` | `0.1;` | `@values.map(\|x\| x + 0.2)` | `0.30000000000000004;` |
| 28 | A new `Float` always reads back as a `Float` | `1.5;` | `@values.map(\|x\| x * 2.0)` | `3.0;` |
| 29 | Untouched literals keep their text | `{ a: 1.50, b: +5 };` | `@values.map(\|r\| r)` | `{ a: 1.50, b: +5 };` |

### Transformation

| # | Purpose | Input | Expression | Output |
|---|---|---|---|---|
| 30 | Struct update | `Config { name: "h", port: 80 };` | `@values.map(\|c\| Config { port: 8080, ..c })` | `Config { name: "h", port: 8080 };` |
| 31 | Struct update of a bare record | `{ a: 1, b: 2 };` | `@values.map(\|r\| { b: 3, ..r })` | `{ a: 1, b: 3 };` |
| 32 | A deep `update` | `{ server: { tls: { port: 1 }, host: "x" } };` | `@values.map(\|c\| c.update(\|c\| c.server.tls.port = 443))` | `{ server: { tls: { port: 443 }, host: "x" } };` |
| 33 | Chained updates | `{ a: 1, b: { c: 2 } };` | `@values.map(\|v\| v.update(\|v\| v.a = 10).update(\|v\| v.b.c = 20))` | `{ a: 10, b: { c: 20 } };` |
| 34 | Compound assignment | `{ count: 4 };` | `@values.map(\|v\| v.update(\|v\| v.count += 1))` | `{ count: 5 };` |
| 35 | Updating an array element | `{ items: [1, 2, 3] };` | `@values.map(\|v\| v.update(\|v\| v.items[1] = 9))` | `{ items: [1, 9, 3] };` |
| 36 | Updating a missing field | `{ a: 1 };` | `@values.map(\|v\| v.update(\|v\| v.z = 1))` | an error at the value |
| 37 | Typed, a value of the wrong type | `Config { name: "h", port: 80 };`, typed | `@values.map(\|c\| c.update(\|c\| c.port = "x"))` | an error before any value is read |
| 38 | Removing array elements | `[1, 5, 2, 8];` | `@values.map(\|a\| a.iter().filter(\|n\| n > 3).collect())` | `[5, 8];` |
| 39 | The whole stream as one value | `1; 2; 3;` | `@values.collect()` | `[1, 2, 3];` |

## Steps

1. Settle the questions below that the first examples need: the command's name and options.
2. Parse expressions: closures, method calls, field and index access, patterns, `as`, the operators, record and array
   literals with struct update, and `@values`. Reuse the lexer and the value grammar where they apply, and keep the
   distinction that `{` always starts a record.
3. Evaluate untyped: `@values` as a lazy iterator over the inputs, the adaptors, access, `filter_match`, numbers and
   `update`, writing the results with the existing rendering of values. Drive it with the examples, one at a time.
4. Add the typed mode: check the expression against the types before reading, then supply defaults, resolve consts
   and names, and widen numbers, as the principle in the decision requires.
5. Errors at the position of the value, or of the expression, and exit codes as in `docs/specs/exit-codes.md`.
6. Write the specification in `docs/specs/`, update the CHANGELOG, and move this task to `docs/tasks/done/`.

## Questions to settle

- The command's name, and its options for the expression, the inputs, `--type`, `--project` and `--stdin`. `query` is
  preferred: like SQL, the word is already read loosely enough to include changes.
- Whether a leading `.` is shorthand for `@values.`, as in `.take(1)`.
- Whether a narrowing cast that is exact, such as `5.0 as Nat` giving `5`, is allowed.
- Whether a query inside a project uses its types without being told.
- The rest of the expression language: `if`/`else`, the details of `match`, the boolean operators, and any string
  methods.
- Whether one `update` closure may make several assignments, or chaining is enough.
- A way to update every location that a predicate selects, as jq's `(.. | numbers) |= . * 10` does, if real use shows
  the need.

## Dependencies

None. How maps are modelled (`docs/tasks/todo/2026-11-14-model-maps/model-maps.md`) will add map access and updates
to the language later.

## Notes from the exploration

### jq as a transformation tool

The design started from a review of jq 1.7, checked locally, and its rejection.

- **The model.** A program is a filter: one value in, a stream of zero or more values out. `|` passes each output on,
  `,` joins streams and `empty` is the empty stream, so selecting, iterating and fanning out are one mechanism. The data
  is plain JSON, with no tags or types. Numbers are doubles once touched: 1.7 keeps an untouched literal's text, but
  `100000000000000000000000001 + 0` is `1e+26`.
- **Paths are first-class.** `.a`, `.[0]`, `.[2:4]`, `.[]`, `..` and `?`. Any filter built from these and `select`,
  `if` and `,` is a path expression, which selects by predicate: `path(.. | numbers | select(. > 1))`. `getpath`,
  `setpath`, `delpaths` and `tostream` work on paths as data.
- **Transformation.** `pathexp |= f` applies `f` at every selected location and keeps the rest of the document;
  `(.. | numbers) |= . * 10`. `=` and `+=` and the rest assign. `del(pathexp)` deletes, and since 1.7 so does
  `pathexp |= empty`. `{name, port: .cfg.port, (.k): .v}` and `[f]` build values, and `pick` keeps only the selected
  paths. `walk(f)` rewrites a whole tree, `with_entries(f)` the keys of an object. There are `map`, `sort_by`,
  `group_by`, `unique_by`, `add`, `INDEX` and `JOIN`, `reduce` and `foreach`, `if`, `//`, `try`/`catch`, destructuring
  with `?//`, functions that take filters as arguments, modules, regular expressions and output formats such as `@csv`.
- **The command line.** Several values per input, `-n`, `-s` (slurp), `-R`, `inputs`, `--stream`, `--arg`,
  `--argjson`, `-r`, `-c`, `-S` and `-e`.
- **Weak points.** No types, so mistakes appear downstream; errors about data have no position; sharp edges, such as
  `|=` using only the first output of its right-hand side, `=` against `|=`, and `pick` padding arrays with `null`.

The user's verdict: the point-free style, in which nothing is named, is as unpleasant as `awk`, and few people have jq
skills to carry over. Two ideas were kept: paths as values, and changing one thing while keeping the rest. Constructo
adds what jq lacks: types.

### How the design was reached

- **Directions considered:** fixed verbs on paths, patterns and templates in Constructo's syntax, a small query
  language with comprehensions, and a library only. The decision record compares them.
- **The first experiment** was extracting the `x` of a stream of `Measurement { x: Float, y: Float }`, written
  `|m| m.x`. It raised whether the closure applies to each value or to the stream, which led to the stream as a value
  (`@values.map(|m| m.x)`), so that the first item or the whole stream can be processed.
- **The name `@values`** was chosen from `@values`, `$values`, `@input`, `$input`, a leading `.`, a keyword, `self`
  and `_`. Lower-case names are consts and capitalised ones constructors, so a plain word could clash; `$` is expanded
  inside a shell's double quotes; `values` is Constructo's own word and plural, like a stream, where jq's `input` is
  one value. `@` is also Rust's pattern binding (`m @ Measurement { .. }`); that is infix where `@values` is prefix,
  but worth watching if bindings are adopted.
- **Types optional** was chosen over requiring them, because requiring them is too big a price in usefulness.
- **Numbers.** Comparing by value with a warning in typed mode was suggested and not taken: the user preferred Rust's
  strictness, since a cast does useful work when a source writes `5` for a `Float`. `as Float` was preferred to
  `as f64`, since the expressions are Rust-like, not Rust.
- **`filter_match`** replaced an earlier idea of letting `filter_map` take a refutable pattern, which would have changed
  the meaning of a Rust-named method depending on how its parameter was written.
- **`update`** came from seeing that a deep update is "copy and assign". Rust's idiom needs blocks, and reopening
  blocks would have collided with `{` for records (`{ x }` is either), so the copy-and-assign is a method instead.
- **Deleting a field** was dropped: it is needed in JSON only because objects are used as maps, which led to the task
  of modelling maps.
