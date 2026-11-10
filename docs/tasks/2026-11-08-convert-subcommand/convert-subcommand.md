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

```
constructo convert --from=json-exact shapes.json > shapes.values
```


## Step 6: Convert from JSONL (exact)

```
constructo convert --from=jsonl-exact shapes.jsonl > shapes.values
```

## Step 7: Convert from JSON (typed)

```
constructo convert --from=json-typed --project=. shapes.json > shapes.values
```

`json-typed` needs a target type to read, which is a data-type in the project.

## Step 8: Convert from JSONL (typed)

```
constructo convert --from=jsonl-typed shapes.jsonl > shapes.values
```


