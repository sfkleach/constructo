# Task: Implement the `convert` subcommand

## Overview

The idea is to be able to convert to-or-from JSON/JSONL and potentially other
formats. e.g.

```
constructo convert --to=json-exact shapes.values > shapes.json
constructo convert --from=json-exact shapes.json > shapes.values
```

There are two JSON strategies, and a format names both the syntax and the
strategy:

- `json-exact` is explicit and needs no types. Every value is written with its
  kind, and each number with the text that it was written with, so the values can be rebuilt from it. Comments, the
  layout and the spelling of the escapes in a string are not kept.
- `json-typed` is natural JSON, the shape that other systems expect. It needs the
  Constructo data-types, because the reverse direction cannot be told what a
  JSON `5` or `"Dot"` is without a target type, and the forward direction is
  only exact with the types (a bare constructor is not a string, `Some(x)` can
  be flattened to `x`, omitted arguments take defaults, consts are inlined).

The same two strategies exist for JSONL, as `jsonl-exact` and `jsonl-typed`.

## Step 1: Convert to JSON, exactly

Implement the subcommand `convert` so that it takes an option `--to=FORMAT`. The
only format recognised at this step is `json-exact`. Example:

```
constructo convert --to=json-exact shapes.values > shapes.json
```

### The names of the formats

A format is matched exactly, so `json-exact` is the only value of `--to` at this
step, and `JSON-Exact` is not accepted. There is deliberately no bare `json`: a
user has to choose between the exact and the typed strategy and think about
what they give up. So `--to=json` is an error that says why and lists the
choices, for example

```text
ERROR: json is not a format; choose json-exact (every kind kept, no types) or json-typed (natural, needs types)
```

and so is any other value, with the same list. The list names only the formats
that exist, so `json-typed` is not offered until it is implemented, and the
message for `json` says that the typed one is not available yet.

### Inputs

- The inputs are `FILES...` and `--stdin`, read in that order, as for `check`.
  With neither, it is a usage error (exit code 2). A file that cannot be read is
  exit code 2.
- Only values files are converted, and a file is classified by its first token,
  as `check` does. A types file (one that starts with `enum` or `const`) is an
  error with exit code 1, for example
  `ERROR: types.types is a types file; convert reads values files (types.types, line 1, col 1)`.
  A file with no declarations and no values, which is empty or has only comments,
  is ignored: it has nothing to convert, and which kind it is makes no difference.
- No types are read and no type check is made, so the options of `check` for
  projects, the prelude and consts (`--project`, `--no-prelude`,
  `--allow-refs`) are not options of `convert`. `--config` and `--cache` are
  global options, so they are accepted and not used.
- A syntax error stops the run at the first one, as for `check`, with exit code 1.
  A record or a set of named arguments that has a name twice is an error with
  exit code 1.
- Nothing is written to the standard output if there is an error. This is for the
  formats that write one JSON document, which would not be valid if it were cut
  short. The lines formats stream (Steps 2 and 4).

### One value, or all of them

- By default a single value is converted: the first value that there is, across
  the inputs in order, so an input with no values is passed over. If any other
  value follows, in that input or another, one warning is written
  to the standard error, in the form of an error diagnostic:

  ```text
  WARNING: More values follow; only the first was converted, so use --to-array for all (shapes.values, line 1, col 20)
  ```

  A warning does not change the exit code, which is 0.
- With `--to-array` the output is a JSON array of every value of every input, in
  the order that they were read. A single value is an array of one, and no value
  gives `[]`.
- Without `--to-array`, no value at all in any input, which includes inputs that
  are all empty, is an error with exit code 1: `No value to convert`.

### Layout

The JSON is pretty-printed by default, with two spaces of indentation. `--compact`
writes it on one line. The JSONL formats always write one value on each line, so
`--compact` has no effect on them.

### The encoding of `json-exact`

Every value is an object. A number keeps its text as a string, so that its size
and precision never matter and the sign and the point tell `nat`, `int` and
`float` apart. A string is the decoded string. A name is written as it appears,
so a qualified `group::Name` stays qualified. A record or a set of named
arguments is an object, in which the order is not significant. Comments, layout
and positions are not kept.

| Constructo | `json-exact` |
|---|---|
| `5` | `{"nat": "5"}` |
| `+5` | `{"int": "+5"}` |
| `1.5e3` | `{"float": "1.5e3"}` |
| `"hi"` | `{"string": "hi"}` |
| `[1, 2]` | `{"array": [{"nat": "1"}, {"nat": "2"}]}` |
| `{ a: 1 }` | `{"record": {"a": {"nat": "1"}}}` |
| `limit`, `null` | `{"const": "limit"}` |
| `Dot` | `{"constructor": "Dot"}` |
| `Circle { r: 1.0 }` | `{"constructor": "Circle", "record": {"r": {"float": "1.0"}}}` |
| `Pair(1, 2)` | `{"constructor": "Pair", "tuple": [{"nat": "1"}, {"nat": "2"}]}` |
| `Image("a", hOffset: 3)` | see below |
| `Items[1, 2]` | `{"constructor": "Items", "array": [{"nat": "1"}, {"nat": "2"}]}` |

A tuple with named arguments has a `"named"` object next to its `"tuple"` array:

```json
{"constructor": "Image", "tuple": [{"string": "a"}], "named": {"hOffset": {"nat": "3"}}}
```

`Ping()` is `"tuple": []` and `Pong {}` is `"record": {}`, so the form survives even
when it is empty. The postfix `?`, as in `0?`, is written as the `Some(0)` that it
stands for. A const reference stays a reference, so `true` and `null` are
`{"const": "true"}` and `{"const": "null"}`, and the conversion does not depend on
whether the prelude is loaded.

This depends on the rule that a const name starts with a lower-case letter
(decision 35), which makes the kind of a bare name plain from its form.

### Tasks

- [x] Add the subcommand with `--to`, `--to-array` and `--compact`, and the inputs above.
- [x] Write the encoder, with unit tests for every row of the table, and the
      functional tests.
- [x] Add the `WARNING:` form of a diagnostic.
- [x] A spec of the convert command, and a decision for the two JSON strategies.
- [x] Update `docs/specs/exit-codes.md`, which says that there are no warnings, so
      that a run with only warnings exits with 0, and the CHANGELOG.

## Step 2: Convert to JSONL (exact)

Implement the next format `--to=jsonl-exact`, which takes a sequence of values
and simply streams them in compact JSONL format using the same conversion 
rules as for JSON-exact.

```
constructo convert --to=jsonl-exact shapes.values > shapes.jsonl
```


## Step 3: Convert to JSON, typed

`--to=json-typed` writes natural JSON, in the shape that other systems expect. The
shapes follow the convention of serde, which is what Rust writes for enums like
these, so that the output is the JSON that such a program already reads and
writes. Where Constructo has no counterpart in that convention, the choice is
made here and recorded.

### Inputs and options

- The inputs are those of `check`: types files first and then values files, or
  `--project=FOLDER`, and `--stdin`. The values are type-checked first, and a
  type error is an error with exit code 1 and no output.
- `convert` takes the options of `check` that decide how the checking is done,
  under the same names, defaults and `[check]` keys of a settings file:
  `--prelude`, `--widening`, `--allow-refs`, `--infer-limit`, `--default-limit`
  and `-x`. It does not take `--no-type-check` (the values have to be checked),
  `--continue`, `--dev` and `--verbose`.
- A value is converted against its own type. The first value, the warning, `--to-array`,
  the layout and `--compact` are as for `json-exact` (Step 1).
- Bool and Option are those of the prelude. With `--no-prelude`, or if a project
  declares its own types of those names, they are ordinary enums.

### The mapping

| Constructo type or value | JSON |
|---|---|
| `Nat`, `Int`, `Float` | a number, as written (see below) |
| `String` | a string |
| `[T]` | an array |
| an untagged record `{ a: T }` | an object, in the order of the declaration |
| `Bool` | `true` or `false` |
| `Option<T>`: `None` | `null` |
| `Option<T>`: `Some(x)` | `x` |
| a const reference | the value of the const |
| a bare constructor `Dot` | the string `"Dot"` |
| a record constructor `Circle { r: 1.0 }` | `{"Circle": {"r": 1.0}}` |
| a tuple constructor `Pair(1, 2)` | `{"Pair": [1, 2]}` |
| a tuple with one parameter `Wrapper(5)` | `{"Wrapper": 5}` |
| a tuple with none, `Ping()` | `{"Ping": []}` |
| an array constructor `Items[1, 2]` | `{"Items": [1, 2]}` |

- **Enums.** A value of an enum is the externally tagged form above: the name of
  the constructor is the key, or the whole value for a bare constructor. So an
  enum whose constructors are all bare is a string, as in `"Fast"` and `"Slow"`.
  The name is the one that the enum declares, whatever group or rename the
  project uses for it.
- **Tuples.** The parameters are written in the order of the declaration, with
  no names, and an omitted argument is written as its default (decision 18). A
  tuple with one parameter is unwrapped, as serde does for a newtype variant.
- **Records.** The fields are written in the order of the declaration, so that
  one value always converts to the same text. An untagged record at the top level has
  no declaration, and keeps the order it was written in.
- **Options.** A field of type `T?` that is `None` is written `null`, and is
  not left out. `Some(None)`, of an `Option<Option<T>>`, would read back as
  `None`, so it is an error: `ERROR: Some(None) cannot be written as JSON, since it would read back as None`, exit
  code 1.
- **Numbers.** A number is copied as it was written, so a large `Nat` is exact
  here, and what a reader can hold is the reader's limit. A leading `+` is
  dropped, as JSON has none. A number that fills a `Float` slot shows a point, so
  `5` for a `Float` is `5.0` and a typed reader sees a float.

### Tasks

- [x] Share the type-check of `check` with `convert`, with its options.
- [x] Write the typed encoder, with a unit test for every row above, and the functional tests.
- [x] Add the spec section and a decision that the typed shape follows serde.

## Step 4: Convert to JSONL, typed

Implement the next format `--to=jsonl-typed`, which takes a sequence of values and
streams them in compact JSONL format using the same conversion rules as for
JSON-typed (Step 3), as `jsonl-exact` does for `json-exact` (Step 2).

```
constructo convert --to=jsonl-typed shapes.types shapes.values > shapes.jsonl
```

The behaviour is that of `jsonl-exact`, with the inputs and options of `json-typed`:

- Every value of every values file is converted, each as the `json-typed` form of it
  on one line with no white space, and a new line after each. There is no first
  value to choose, so no warning.
- Every value is therefore type-checked, since every value is converted. The first
  error ends the run, as for `json-typed`. The lines are streamed: each is written
  as soon as its value is converted, so an error in a late value leaves the lines
  before it written, and then the error is reported with exit code 1. Every line
  that is written is complete, and an error in the types or in the syntax, which is
  found before any value is converted, leaves the standard output empty.
- No values at all, which includes inputs that are only types files, give no output,
  with exit code 0.
- `--compact` makes no difference and is accepted. `--to-array` is a usage error
  (exit code 2), as for `jsonl-exact`, since a lines format has no array.
- The inputs, the options for types (`--project`, `--prelude`, `--widening`,
  `--allow-refs`, `--infer-limit`, `--default-limit`, `--dev`), the `[check]` keys of
  a settings file and the exit codes are those of `json-typed`. The options for
  types are refused for the formats that need none.
- Each line is exactly what `--compact` gives for that value with `json-typed`, so
  the lines are the elements of the array that `--to-array` gives. The functional
  tests check that.

Streaming is not for `json-exact` and `json-typed`, which write one document. A
document that was cut short by an error would not be valid JSON, so for those the
output is only written if the conversion is whole (Step 1). `jsonl-exact` (Step 2)
is changed to stream as well, so that the two lines formats behave alike.

### Tasks

- [x] Add the format, sharing the typed encoder, with unit tests.
- [x] Stream the lines of both lines formats, so that the library writes to a sink
      and not to a string, and change `jsonl-exact` and its spec text and functest
      to match.
- [x] Functional tests with the examples of Step 3, and the spec section.
- [x] Update `docs/specs/convert-command.md`, `exit-codes.md` and the CHANGELOG.


## Step 5: Convert from JSON (exact)

Given a JSON file in the JSON-exact format, translate it back into a data-values
file.

```
constructo convert --from=json-exact shapes.json > shapes.values
```

Steps 5 and 6 are taken together, because the reading of a value is the same and
only the framing differs. This section is about what they share.

### The direction

`--from=FORMAT` and `--to=FORMAT` say which way the conversion runs, and not which
is the input and which the output. Exactly one of them is required, and giving both
is a usage error (exit code 2). The data-values side is implicit: `--from` always
writes data-values and `--to` always reads them, so there is no `--to=values`. The
output always goes to the standard output, and there is no option for a file.

### Inputs

- The inputs are `FILES...` and then `--stdin`, as for `--to`. Each is a document
  of the format that `--from` names, and the values of all of them are written in
  order.
- A document in `json-exact` is one JSON value. An object is one value. An array is
  a list of values, one for each element, which is the form that `--to-array`
  writes, so that `--to=json-exact --to-array` followed by `--from=json-exact` is
  a round trip. Each element of the array must be an object, and anything else at
  the top, such as a string or a number, is an error.
- An empty file, or one with only white space, is not a JSON document, and is a
  syntax error. (A Constructo document can be empty and a JSON one cannot.) `[]` is
  a document with no values.
- The options for types, `--to-array` and `-x` are refused with `--from`, as the
  options that do not apply are with `--to`: they are usage errors (exit code 2).
  `--compact` is accepted, and `--stdin`, `--config` and `--cache` as before.

### Reading a value

The reader is strict. A value is the shape that `json-exact` writes, and anything
else is an error at the place that is wrong, in the usual form, for example
`ERROR: Expected a string for "nat" (shapes.json, line 7, col 14)`.

- An object has exactly the keys of one kind of value, in any order, and an unknown
  key, a missing key, two kinds in one object or a repeated key is an error at the
  key. The members of a `record` keep the order in which they were written.
- `nat`, `int` and `float` hold a JSON string whose text reads back as a literal of
  that kind: digits for a `nat`, a leading sign for an `int`, and a fraction for a
  `float`. Otherwise `{"int": "5"}` would be written as `5` and read back as a
  `Nat`. A JSON number, as in `{"nat": 5}`, is an error that says to write
  `{"nat": "5"}`.
- A `const` is a name that starts with a lower-case letter or `_`, a `constructor`
  one that starts with a capital letter, and a qualifier such as `shapes::Circle`
  is allowed. A field or argument name is a lower-case name. Anything else is an
  error at the string.
- A constructor has at most one body, `record`, `tuple` or `array`, and `named`
  only with `tuple`. An empty body is kept.
- An object that is the shape of `json-typed`, such as `{"Circle": {"r": 1.0}}`,
  gets an error that says that it looks like `json-typed`. A bare `--from=json` is
  not a format, as it is not for `--to`.

### The data-values that are written

- Each value ends with `;` and a new line, so the output is a values file.
- The layout is that of the JSON output: a value goes on one line if its text on
  one line is at most 80 characters wide, not counting the indentation, and
  otherwise it is spread, with one field, element or argument on each line, and
  the parts apply the rule to themselves. `--compact` writes every value on one
  line, with no layout. For example:

  ```
  Move { x: 1, y: 2 };
  Config {
      name: Some("n"),
      on: false,
      size: { w: 2.0, h: 1.0 }
  };
  ```

- A number is written as its text. A string is written with the JSON escapes, and
  any character that the values grammar does not allow raw, such as a control
  character, is written as `\uXXXX`. The reader of the output is `check` and
  `convert --to`, so what is written has to parse as the value that was read.
- `Some(5)` is written as `Some(5)`, and never with the postfix `?`.

### Errors, exit codes and streaming

- The output is written as the values are converted, and the run stops at the first
  problem with exit code 1, whatever has been written. The output that comes before
  a problem is not made tidy.
- A syntax error in a JSON document is found before any value of that document is
  written, since a document is read whole. An earlier document in the inputs may
  have been written already.
- An input that cannot be read is exit code 2, and so is an output that cannot be
  written, as for `--to`.

### The parser

A JSON parser of our own, in a new module. It records the line and column of every
node, so that an error in a value is reported where it is, which a parser that does
not keep positions cannot do. It keeps a number as its text, keeps a repeated key
(the reader of `json-exact` is what refuses it, at the key), limits the nesting as
the other parsers do, and accepts exactly RFC 8259: no comments, no trailing
commas, a leading `+` or zeros not allowed.

### Tasks

- [x] A JSON parser with positions, and a writer of data-values with the layout above.
- [x] The reader of `json-exact`, with unit tests for every kind of value and for
      every error, and a round trip over `kinds.values`.
- [x] `--from` in the command, with `--to` exclusive of it and the refusals above.
- [x] Functional tests, the spec section, and the exit codes and CHANGELOG.

## Step 6: Convert from JSONL (exact)

Given a JSONL file in the JSONL-exact format, translate it back into a
data-values file.

```
constructo convert --from=jsonl-exact shapes.jsonl > shapes.values
```

The direction, the options, the reading of a value, the output and the exit codes
are those of Step 5. What differs is the framing:

- Each line is one JSON value, which is an exact value, so an object. A line that is
  an array is an error, since the array form belongs to a `json-exact` document.
  A value that is spread over several lines is not allowed, and a line that stops
  part way through an object is a syntax error at that line.
- A line with only white space is skipped, and `\r\n` is accepted as well as `\n`.
  A last line with no new line is fine.
- An empty file is a file of no lines, so it is valid and has no values, unlike an
  empty `json-exact` file.
- A position is in the file: the line is the line of the file, and the column is
  within the line.
- The lines are converted one at a time. A problem on a line ends the run with
  exit code 1 at that line, and the values of the lines before it have been written.
  There is no reading of the whole file first.

### Tasks

- [x] The reader of `jsonl-exact`, sharing the reading of a value with Step 5, with
      unit tests for the lines, the blank lines, the line endings and the errors.
- [x] Functional tests with the files that `--to=jsonl-exact` writes, including a
      round trip, the spec section, and the CHANGELOG.

## Step 7: Convert from JSON (typed)

```
constructo convert --from=json-typed --project=. shapes.json > shapes.values
```

`json-typed` needs a target type to read, which is a data-type in the project.

## Step 8: Convert from JSONL (typed)

```
constructo convert --from=jsonl-typed --project=. shapes.jsonl > shapes.values
```


