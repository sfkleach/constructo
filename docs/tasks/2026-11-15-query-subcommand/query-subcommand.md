# Task: Add the query subcommand

## Overview

`docs/vision.md` lists, for later, selecting sub-values from a file or stdin. The design is settled in
[decision 44](../../../decisions/0044-queries-are-rust-like-expressions-over-a-stream-of-values.md): a query is a
Rust-like expression over `#values`, the lazy stream of the input values, which extracts data and transforms it with one
language. This task implements it, test first.

The command is `constructo query EXPR FILES`, as settled below with its options.

The decision record is the specification. This file adds the examples that drive the implementation, the steps, the
questions that are left open, and the notes from the exploration that led to the design.

## TDD examples

Each example is an input, an expression and the expected output. They become functional tests, and the smaller ones
unit tests too. The values are written as a values file would have them.

### Extraction (the user's list)

| # | Purpose | Input | Expression | Output |
|---|---|---|---|---|
| 1 | Pass the input through | `1; "a"; Quit;` | `#values` | `1; "a"; Quit;` |
| 2 | Delete the whole input | `1; 2; 3;` | `#values.take(0)` | nothing |
| 3 | The first item | `10; 20; 30;` | `#values.take(1)` | `10;` |
| 3a | The first item as an `Option` | `10; 20; 30;` | `#values.next()` | `Some(10);` |
| 3b | No first item | (empty) | `#values.take(1)` / `#values.next()` | nothing / `None;` |
| 4 | The 3rd item, if it exists | `10; 20; 30;` | `#values.skip(2).take(1)` | `30;` |
| 4a | The 3rd item as an `Option` | `10; 20; 30;` | `#values.nth(2)` | `Some(30);` |
| 4b | No 3rd item | `10; 20;` | `#values.skip(2).take(1)` / `#values.nth(2)` | nothing / `None;` |
| 5 | A field of records | `{ x: 1, y: 2 }; { x: 3, y: 4 };` | `#values.map(\|r\| r.x)` | `1; 3;` |
| 6 | A pair of fields | `{ x: 1, y: 2, z: 9 }; { x: 3, y: 4, z: 9 };` | `#values.map(\|r\| { x: r.x, y: r.y })` | `{ x: 1, y: 2 }; { x: 3, y: 4 };` |
| 7 | Lengths of arrays | `[1, 2]; []; [5, 6, 7];` | `#values.map(\|a\| a.len())` | `2; 0; 3;` |
| 8 | First elements, an error when empty | `[1, 2]; []; [5, 6, 7];` | `#values.map(\|a\| a[0])` | `1;` then an error at the empty array |
| 8a | First elements, skipping empty arrays | `[1, 2]; []; [5, 6, 7];` | `#values.filter_map(\|a\| a.first())` | `1; 5;` |
| 9 | Third elements | `[1, 2]; []; [5, 6, 7];` | `#values.filter_map(\|a\| a.get(2))` | `7;` |
| 10 | Fields from arrays of records, as one stream | `[Pair { left: true, right: false }, Pair { left: false, right: true }]; [];` | `#values.flat_map(\|ps\| ps.iter().map(\|p\| p.right))` | `false; true;` |
| 10a | The same, one array per value | as 10 | `#values.map(\|ps\| ps.iter().map(\|p\| p.right).collect())` | `[false, true]; [];` |
| 11 | Positional arguments | `[Record(true, right: false), Record(false, false)];` | `#values.flat_map(\|rs\| rs.iter().map(\|r\| r.0))` | `true; false;` |
| 11a | A named argument, untyped | as 11 | `#values.flat_map(\|rs\| rs.iter().map(\|r\| r.right))` | `false;` then an error at `Record(false, false)` saying to supply the types |
| 11b | A named argument, typed | as 11, with `enum R = Record(Bool, right: Bool);` | as 11a | `false; false;` |

### Patterns and constructors

| # | Purpose | Input | Expression | Output |
|---|---|---|---|---|
| 12 | `filter_match` skips other constructors | `Move { x: 1, y: 2 }; Quit; Move { x: 3, y: 4 };` | `#values.filter_match(\|Move { x, .. }\| x)` | `1; 3;` |
| 13 | Field shorthand after a constructor name | as 12 | `#values.filter_match(\|Move { x, y, .. }\| Point { x, y })` | `Point { x: 1, y: 2 }; Point { x: 3, y: 4 };` |
| 13a | A bare record names its fields | as 12 | `#values.filter_match(\|Move { x, .. }\| { x })` | an error before any value: write `x` for the value, or `{ x: x }` for a record |
| 14 | `matches!` | as 12 | `#values.filter(\|v\| matches!(v, Quit)).count()` | `1;` |
| 15 | A refutable pattern in `map` is refused | as 12 | `#values.map(\|Move { x, .. }\| x)` | an error before any value: the parameter must always match |
| 16 | Untyped names match by exact text | `shapes::Circle { r: 1.0 }; Circle { r: 2.0 };` | `#values.filter_match(\|Circle { r }\| r)` | `2.0;` |

### Numbers

| # | Purpose | Input | Expression | Output |
|---|---|---|---|---|
| 17 | A literal adapts | `{ y: 1.5 }; { y: -2.0 };` | `#values.filter(\|m\| m.y > 0.0).count()` | `1;` |
| 18 | Mixed kinds need a cast, untyped | `{ y: 1.5 }; { y: 5 };` | `#values.filter(\|m\| m.y > 0.0).count()` | an error at `5`, suggesting `m.y as Float` |
| 19 | The cast works on every spelling | `{ y: 1.5 }; { y: 5 };` | `#values.filter(\|m\| m.y as Float > 0.0).count()` | `2;` |
| 20 | Typed, `5` in a `Float` field is a `Float` | as 18, with `y: Float` | as 18 | `2;` |
| 21 | A narrowing cast that changes the number | `-1;` | `#values.map(\|n\| n as Nat)` | an error at `-1` |
| 22 | `Nat` arithmetic | `{ port: 8080 };` | `#values.map(\|c\| c.port + 1)` | `8081;` |
| 23 | `Nat` below zero | `3;` | `#values.map(\|n\| n - 5)` | an error at `3` |
| 24 | `Int` results are signed | `3;` | `#values.map(\|n\| n as Int - 5)` | `-2;` |
| 25 | Integers are unbounded | `18446744073709551615;` | `#values.map(\|n\| n + 1)` | `18446744073709551616;` |
| 26 | Integer division truncates | `7;` | `#values.map(\|n\| n / 2)` / `#values.map(\|n\| n % 2)` | `3;` / `1;` |
| 27 | `Float` arithmetic is `f64` | `0.1;` | `#values.map(\|x\| x + 0.2)` | `0.30000000000000004;` |
| 28 | A new `Float` always reads back as a `Float` | `1.5;` | `#values.map(\|x\| x * 2.0)` | `3.0;` |
| 29 | Untouched literals keep their text | `{ a: 1.50, b: +5 };` | `#values.map(\|r\| r)` | `{ a: 1.50, b: +5 };` |

### Transformation

| # | Purpose | Input | Expression | Output |
|---|---|---|---|---|
| 30 | Struct update | `Config { name: "h", port: 80 };` | `#values.map(\|c\| Config { port: 8080, ..c })` | `Config { name: "h", port: 8080 };` |
| 31 | Struct update of a bare record | `{ a: 1, b: 2 };` | `#values.map(\|r\| { b: 3, ..r })` | `{ a: 1, b: 3 };` |
| 32 | A deep `update` | `{ server: { tls: { port: 1 }, host: "x" } };` | `#values.map(\|c\| c.update(\|c\| c.server.tls.port = 443))` | `{ server: { tls: { port: 443 }, host: "x" } };` |
| 33 | Chained updates | `{ a: 1, b: { c: 2 } };` | `#values.map(\|v\| v.update(\|v\| v.a = 10).update(\|v\| v.b.c = 20))` | `{ a: 10, b: { c: 20 } };` |
| 34 | Compound assignment | `{ count: 4 };` | `#values.map(\|v\| v.update(\|v\| v.count += 1))` | `{ count: 5 };` |
| 35 | Updating an array element | `{ items: [1, 2, 3] };` | `#values.map(\|v\| v.update(\|v\| v.items[1] = 9))` | `{ items: [1, 9, 3] };` |
| 36 | Updating a missing field | `{ a: 1 };` | `#values.map(\|v\| v.update(\|v\| v.z = 1))` | an error at the value |
| 37 | Typed, a value of the wrong type | `Config { name: "h", port: 80 };`, typed | `#values.map(\|c\| c.update(\|c\| c.port = "x"))` | an error before any value is read |
| 38 | Removing array elements | `[1, 5, 2, 8];` | `#values.map(\|a\| a.iter().filter(\|n\| n > 3).collect())` | `[5, 8];` |
| 39 | The whole stream as one value | `1; 2; 3;` | `#values.collect()` | `[1, 2, 3];` |

## Steps

1. Settle the questions that the first examples need. Done: see the settled questions below.
2. Parse expressions: closures, method calls, field and index access, patterns, `as`, the operators, `if` and `match`
   in both forms, record and array literals with struct update, and `#values`. Reuse the lexer and the value grammar
   where they apply. Outside the braced forms of `if` and `match`, `{` always starts a record.
3. Type-check the expression, and evaluate untyped: `#values` as a lazy iterator over the inputs, the adaptors, access, `filter_match`, numbers and
   `update`, writing the results with the existing rendering of values. Drive it with the examples, one at a time.
4. Add the typed mode: check the expression against the types before reading, then supply defaults, resolve consts
   and names, and widen numbers, as the principle in the decision requires.
5. Errors at the position of the value, or of the expression, and exit codes as in `docs/specs/exit-codes.md`.
6. Write the specification in `docs/specs/`, update the CHANGELOG, and move this task to `docs/tasks/done/`.

## Outcome

Done, in the stages of the steps above: the untyped language, then the typed mode, then the check of a typed query
against the types before any value is read. The command is specified in `docs/specs/query-command.md`. Three choices
were made while implementing it:

- **A typed query reads every input before it runs,** since every value is checked as `check` checks it first; an
  untyped one reads them lazily.
- **Typed, a name is written as the source of its value writes it,** after resolution, so a name written with a
  qualifier that it does not need may be written without one. Numbers and the rest of a value keep their text.
- **Without `--type`, the type of a value that the query is given is not known up front,** so example 37 needs
  `--type=Config` for its error to be found before any value is read; without it, the error is found at the value.

## Questions to settle

- A way to update every location that a predicate selects, as jq's `(.. | numbers) |= . * 10` does, if real use shows
  the need.

## Settled questions

### The command line (2026-11-15)

`constructo query [OPTIONS] <EXPR> [FILES]...`

- **The name is `query`.** Like SQL, the word is already read loosely enough to include changes.
- **The expression is the first positional argument,** as for `jq` and `grep`. Reading it from a file (`-f`) waits
  until it is needed.
- **The inputs are as for `convert --to=json-typed`:** types files may come among `FILES`, each file classified by its
  content; `--project` and `--stdin` mean what they mean for `check` and `convert`; and the settings options
  (`--prelude`, `--widening`, `--allow-refs`, the limits, `--config`, `--cache`, `--dev`) are shared.
- **Types and `--type`.** Having types, from a project or from types files, makes the run typed: each value is typed by
  its constructor, as `check` does, and errors that the types can find are found value by value. `--type T` adds that
  every value is a `T`, so `#values` is an iterator of `T` and the expression is checked before any data is read.
- **The output** is values text, with `--compact` as for `convert`. Other formats wait, since piping into
  `convert --from` covers them.

### No leading `.` for `#values.` (2026-11-15)

A leading `.` is not shorthand for `#values.`. It makes the easiest queries trivial without generalising or fitting
the design: `jq` users would read `.x` as the field of each value, and get an error about an iterator instead. The
Rust-like principle already accepts Rust's level of verbosity, and a shorthand can be added later but never removed.

### Exact casts, and casts to `Float` (2026-11-15)

- **A narrowing cast that does not change the number is allowed:** `5.0 as Nat` gives `5`, `+5 as Nat` gives `5`, and
  `1.0e3 as Int` gives `+1000`. An untouched literal converts by the exact value of its text, and a computed `Float` by
  the exact value of its `f64`. A cast that would change the number, such as `1.5 as Int`, stays an error.
- **`as Float` rounds to the nearest `f64`,** as `Float` arithmetic does, so `12345678901234567890123 as Float` and
  `0.1 as Float` are allowed although neither is exact. A `Float` is exact only for integers in a limited range and
  for some integers divided by a power of 2, and programmers accept the approximation elsewhere; there is no practical
  alternative. A value beyond the range of `f64`, such as `1e400`, is an error, as it is when a literal is converted.

### The folder a query runs in is immaterial (2026-11-15)

A query in a folder that holds a project does not use its types unless `--project` names it, as for `check` and
`convert`. Where a command is run never changes what it does.

### `if` has an outfix form and Rust's form (2026-11-15)

- **Outfix:** `if TEST then THEN elseif TEST then THEN ... else ELSE endif`. Every `{` in it is a record, so a record
  is written plainly in a branch: `if c then { x: 1 } else { x: 0 } endif`. This is the more natural form, which Rust
  walked away from.
- **Rust's form:** `if TEST { THEN } else if TEST { THEN } ... else { ELSE }`, so a Rust programmer's first guess
  works. Its braces hold one expression, not statements, so there are still no blocks, and a record in a branch has
  braces of its own: `if c { { x: 1 } } else { { x: 0 } }`.
- **`else` is required** in both, since there is no `()` to fall back to. Each `if` keeps to one form.
- **Parsing.** The condition is read as an ordinary expression, records and constructor bodies included, and the next
  token decides the form: `then` for outfix, `{` for Rust's. So the outfix form has no restriction on its condition,
  and neither form needs Rust's rule that a struct literal in a condition must be in parentheses.
- **Backtracking.** Read in full, the condition of `if v == None { x } else { y }` swallows the branch as `None`'s
  record body (`{ x }` in shorthand), and is followed by `else`. So when a condition ends in a constructor with a
  record body and the next token is neither `then` nor `{`, the condition is read again without that body, which
  becomes the Rust branch. This is one step, at one place, and the two readings cannot both succeed, because the
  wrong one always fails at the next token. `match EXPR { ... }` is parsed the same way.

### Field shorthand only after a constructor name (2026-11-15)

As in Rust, `Move { x, y }` means `Move { x: x, y: y }`, and a bare record names its fields: `{ x: x, y: y }`. Rust
has no anonymous records, so a Rust programmer reads a bare `{ x }` as a block that gives `x`, and writes `|m| { m }` or
`Some(x) => { x }` out of habit. Had the shorthand applied to bare records, those would quietly give the record
`{ m: m }`; instead they are errors that say to write `m` for the value or `{ m: m }` for a record. Patterns keep
Rust's shorthand, as in `Move { x, .. }`, since a pattern only binds. This replaces the example in decision 44,
`|Move { x, y, .. }| { x, y }`.

### Braces need only ever denote records (2026-11-15)

**The principle:** it is always possible to write Constructo in which braces denote only records, bare or after a
constructor, and never blocks or branches. Rust's braced forms are accepted for familiarity, but no one is forced to
use them. Any new construct with a braced Rust form needs an outfix form too. With the outfix `if` and `match`,
nothing else needs braces: closures have none, `update` takes a closure, and `matches!` uses parentheses.

### `match` (2026-11-15)

```
#values.map(|s| match s with
    Circle { r } => 3.14159 * r * r,
    Rect { w, h } if w == h => w * w,
    Rect { w, h } => w * h,
    Quit | Pause => 0.0,
endmatch)
```

- **Two forms,** as for `if`: outfix, `match EXPR with ARMS endmatch`, and Rust's, `match EXPR { ARMS }`. `with` is
  OCaml's and F#'s word for this and reads as "match against these cases"; `do` was considered and not taken, since
  it suggests steps to be executed. In the outfix form `with` ends the scrutinee, so no backtracking is needed; Rust's
  form is parsed as `if` is.
- **Arms** are `pattern => expression`, separated by commas, with an optional trailing comma, `if` guards and `|`
  alternatives. Patterns have `_`, `..`, bindings and `name @ pattern`.
- **An arm's body is an expression,** so a record there names its fields, and Rust's habit of `=> { x }` is an error.
- **Exhaustiveness.** Untyped, the enum cannot be known, so a value that no arm matches is an error at the value.
  Typed, a `match` that does not cover every constructor is refused before any data is read, as in Rust.

### Boolean and comparison operators (2026-11-15)

Rust's operators, with Rust's precedence.

- **`&&`, `||` and `!`** short-circuit, and their operands must be booleans. There is no truthiness, so
  `if v.name then ... endif` is a type error, not a test for emptiness.
- **`==` and `!=`** compare whole values structurally, and only values of the same type: checked up front where the
  expression fixes the type, and at the value otherwise. Numbers must be of the same kind; untouched literals compare
  by exact value (`1.50 == 1.5`) and computed `Float`s as `f64`. Strings compare by their content once escapes are
  read (`"\u{41}" == "A"`). Records compare by their fields, in any order. Constructors compare by name (by text when
  untyped) and then by body.
- **`<`, `>`, `<=` and `>=`** work on numbers and strings only, with strings ordered by code point, as in Rust. There
  is no ordering of constructors, records or arrays, as Rust's `#[derive(PartialOrd)]` gives: declaration order needs
  the types, it is rarely what a query means, and Constructo already gives declaration order one meaning, since the
  first constructor of an enum is its default. Overloading it with a second meaning would be unwise.

### String methods (2026-11-15)

A small set to start with, each with Rust's meaning:

| Method | Gives |
|---|---|
| `len()` | the length in bytes of the UTF-8, as in Rust |
| `is_empty()` | a boolean |
| `contains(s)`, `starts_with(s)`, `ends_with(s)` | a boolean |
| `trim()`, `to_lowercase()`, `to_uppercase()` | a string |
| `char_strings()` | an iterator of one-character strings |
| `split(s)` | an iterator of strings |
| `s + t` | the concatenation |

- **`len()` counts bytes,** since a familiar name keeps its meaning. Characters are counted with
  `s.char_strings().count()`.
- **`char_strings()`, not `chars()`.** Constructo has no character type yet, so the characters are given as strings,
  under a name of our own. `chars()` is reserved for when there is a character type, and will then have Rust's exact
  meaning; until then it is an error that suggests `char_strings()`.
- **Deferred** until there is a need: `format!`, regular expressions, `replace`, slicing by range, and parsing
  numbers (`s.parse()`).

### One assignment per `update` (2026-11-15)

Several changes are made by chaining, `c.update(|c| c.a = 1).update(|c| c.b.c = 2)`, and an `update` closure makes one
assignment. A separator such as `;` or `,` would begin to look like statements and invite a braced block, against the
principle that braces need only denote records. If a need appears, an outfix form can be designed then.

### Untyped still means type-checked (2026-11-15)

"Untyped" means only that there are no types files and no project. The expression is always type-checked; what is
missing is knowledge of the data.

- **What the expression fixes is checked before any data is read.** `if v.ok then 1 else "failed" endif` is refused
  up front, as Rust would refuse it, because `Nat` and `String` have no common type. So are `1 + "a"` and
  `"a".len() > 0.0`. An ill-typed expression may be fine in Python, but it is antithetical to Constructo.
- **What comes from the data has a type that is not known until a value arrives.** `if c then v.x else "failed" endif`
  passes the check, and each value's `x` is then checked to be a `String`; a mismatch is an error at the value, as
  strict numbers already are.
- **The check at a value sees its kind:** `Nat`, `Int`, `Float`, `String`, record, array or constructor. It cannot
  tell whether two constructors belong to the same enum, so untyped that is not checked.
- **`.collect()` needs a common type,** so collecting values of different kinds is an error at the value where the
  kinds part. This replaces the earlier note that untyped `.collect()` may make a mixed array. A stream is not an
  array, so `#values` on `1; "a"; Quit;` still writes all three.

**The principle, restated:** checked and unchecked processing give the same result whenever both succeed. Either may
fail where the other succeeds: checked finds errors that unchecked cannot see, such as mixing constructors of
different enums, and supplies what the text leaves out, such as defaults, consts and widening.

**Unchecked processing assumes the type distinctions that the text shows** (2026-11-15). Without the types it is
obliged to: constructors with the same text are the same constructor, and constructors with different text are
different, so `v == Square` is `false` on a `Circle` and `filter_match(|Circle { r }| r)` drops a `Square`. The
principle is strictly true only when the types used by checked processing agree with those distinctions. They may
not: a qualified name (`shapes::Circle`) or a renaming import can give one constructor two spellings, and the same
text in two files with different imports can name two constructors. Then the results may differ, and supplying the
types gives the right one.

## Dependencies

None. How maps are modelled (`docs/tasks/todo/2026-11-14-model-maps/model-maps.md`) will add map access and updates
to the language later.

A character type, and `chars()` with it, will come from a future design that unifies all literals as typed values
with a format, as the Monogram project does, which will also bring dates and similar types.

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
  (`#values.map(|m| m.x)`), so that the first item or the whole stream can be processed.
- **The name `#values`** was chosen from `#values`, `$values`, `@input`, `$input`, a leading `.`, a keyword, `self`
  and `_`. Lower-case names are consts and capitalised ones constructors, so a plain word could clash; `$` is expanded
  inside a shell's double quotes; `values` is Constructo's own word and plural, like a stream, where jq's `input` is
  one value. `@` is also Rust's pattern binding (`m @ Measurement { .. }`); that is infix where `#values` is prefix,
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
