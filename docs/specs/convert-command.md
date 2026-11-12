# The convert command

`constructo convert` reads data-values and writes them in another notation. This document says what it reads, what it
writes, what it prints when something is wrong, and what its exit code means. Four formats exist so far: `json-exact`,
which writes every kind of value in an explicit form and needs no types, `jsonl-exact`, which writes the same encoding
on a line for each value, `json-typed`, which writes natural JSON in the shapes that serde uses, and needs the types,
and `jsonl-typed`, which writes the same encoding on a line for each value. With `--from` the exact formats are read
back and the values are written as text. The conversions back from the typed formats will be added here.

```text
constructo convert --to=json-exact|jsonl-exact [--to-array] [--compact] [-x] [--stdin] [FILES]...
constructo convert --to=json-typed|jsonl-typed [--to-array] [--compact] [--project=FOLDER] [OPTIONS] [FILES]...
constructo convert --from=json-exact|jsonl-exact [--compact] [--stdin] [FILES]...
```

Exactly one of `--to` and `--from` is given. `--to` writes JSON, and is what most of this document is about. `--from`
is the other direction, and has [its own section](#reading-back-with---from).

## Inputs

The inputs are the `FILES`, in the order that they are written, and then the standard input if `--stdin` is given,
which is processed as one more file, named `<stdin>`, after the others. A run needs at least one of them, and without
one the command line is refused as a usage error (exit code 2).

For `json-exact` and `jsonl-exact` an input is read as a values file. For `json-typed` and `jsonl-typed` the inputs are those of
[the check command](check-command.md), as [its section](#the-format-json-typed) says. The inputs are read one at a time. An
input that cannot be read ends the run at once with exit code 2. An input that has a syntax error, or for `json-exact`
and `jsonl-exact` is a types file, ends the reading with exit code 1, so that an unreadable input after it is never
reached, as for [the check command](check-command.md). The standard input is only read if none of the files has failed.

- **A types file** is an error, whatever its place: `NAME is a types file; convert reads values files`, at line 1,
  column 1 of the file. The kind of a file is decided by its first token, as the check command says.
- **A file with no declarations and no values**, which is empty or has only comments, is ignored. The parser takes such
  a file for a types file, but as it has nothing in it the kind makes no difference.
- **Values** in a file are separated by semi-colons, as they are everywhere. There are no types, so nothing is checked
  against a schema: `[1, "x"]` is converted as it is. A name is not looked up either. A name is a const or a constructor
  by its form alone, as [decision 35](../decisions/0035-a-const-name-starts-with-a-lower-case-letter.md) says.

For these two formats the prelude is not loaded and no project is read, because nothing is resolved. Of the options of
the check command, only `-x` and `--no-experimental` are accepted, since the postfix `?` has to be switched on to be
parsed. A `?` is written as the `Some(...)` that it stands for. The options that `json-typed` takes in addition, such
as `--project` and `--no-prelude`, are refused: `ERROR: --no-prelude does not apply to json-exact, which needs no
types`, with exit code 2.

## One value, or all of them

This section is about `json-exact` and `json-typed`, which each write a single document. The lines formats write every
value, as [their sections](#the-format-jsonl-exact) say.

- **By default the first value is converted.** It is the first value that there is, across the inputs in order, so an
  input with no values is passed over. If any value follows it, in the same input or in a later one, there is one
  warning, at the position of the first value that follows: `More values follow; only the first was converted, so use
  --to-array for all`. Only the first of them is named, and the exit code is still 0.
- **With `--to-array` every value of every input is converted**, in order, and written as one JSON array. No values at
  all give `[]`. There is no warning.
- **Without `--to-array`, no value at all is an error**: `No value to convert`, at line 1, column 1 of the first input,
  with exit code 1.

Only the values that are converted are encoded, and for `json-typed` only they are type-checked. So a duplicate name in
a value that comes after the first one is not an error unless `--to-array` is given, and nor is a type error. Converting
the first value is a convenience, and it does not ask for the rest of the file to be right.

## Output

The output is written to the standard output. A format that writes one document, `json-exact` and `json-typed`, writes
it only if the conversion is whole, since a document that was cut short by an error would not be valid JSON, so an error
leaves the standard output empty. A lines format writes each line as it is converted, as [its
section](#the-format-jsonl-exact) says. Warnings and errors go to the standard error, in the usual form, which for a
warning starts `WARNING:`. See [the exit codes](exit-codes.md).

If the output cannot be written, as when it is a pipe whose reader has gone, the run ends with exit code 2. A reader
that has gone, as `head` does, is what the reader asked for, so there is no message for it, and for any other failure
the message is `ERROR: Cannot write the output: REASON`.

`--compact` writes no white space at all: `{"nat":"5"}`. Without it the output is indented by two spaces, with a space
after each `:` and `,`, and it is laid out for reading. A container is written on one line if that line is at most 80
characters wide. The width is that of the text of the container alone, so the indentation, the key that it follows and
a comma after it are not counted, and a container is laid out the same wherever it is. A container that is wider is
spread over lines, with one member on each, and each member is laid out by the same rule. An empty container is always
on one line. So `{"nat": "5"}` and `{"constructor": "Write", "tuple": [{"string": "hi"}]}` are one line each, and a
longer value is spread, with the parts of it that are narrow enough still on one line:

```json
{
  "Config": {
    "name": "n",
    "on": false,
    "speed": "Slow",
    "tags": ["x"],
    "size": {"w": 2.0, "h": 1.0}
  }
}
```

There is no maximum column, since indentation can make a line as long as the nesting is deep. A string is never broken,
so a long one makes a long line. The width is in characters, with each escape counted as the characters that it is
written with. The layout depends on how long the text of a value is, so a change to a string can change the lines of
the value that holds it.

The output ends with a new line in both layouts. Strings are written with the escapes of RFC 8259: the quote, the
backslash and the control characters are escaped, and everything else, including what is not ASCII, is written as it is.

## The format `json-exact`

Every value is a JSON object that says what kind of value it is, so that its kind, its structure and the text of its
numbers are kept, and it can be read back as the same value. What is not kept is the text of the file around the
values: comments, layout, and the spelling of the escapes in a string, since `"a"` and `"\u0061"` are the same string.

| A value                        | Is written as                                                                      |
|--------------------------------|------------------------------------------------------------------------------------|
| `5`                            | `{"nat": "5"}`                                                                     |
| `+5`, `-7`                     | `{"int": "+5"}`, `{"int": "-7"}`                                                   |
| `1.50`                         | `{"float": "1.50"}`                                                                |
| `"hi"`                         | `{"string": "hi"}`                                                                 |
| `[1, 2]`                       | `{"array": [{"nat": "1"}, {"nat": "2"}]}`                                          |
| `{ b: 1, a: 2 }`               | `{"record": {"b": {"nat": "1"}, "a": {"nat": "2"}}}`                               |
| `red`                          | `{"const": "red"}`                                                                 |
| `Quit`                         | `{"constructor": "Quit"}`                                                          |
| `Move { x: 1 }`                | `{"constructor": "Move", "record": {"x": {"nat": "1"}}}`                           |
| `Write("hi")`                  | `{"constructor": "Write", "tuple": [{"string": "hi"}]}`                            |
| `Span(1, end: 3)`              | `{"constructor": "Span", "tuple": [{"nat": "1"}], "named": {"end": {"nat": "3"}}}` |
| `Users[u1, Bob]`               | `{"constructor": "Users", "array": [{"const": "u1"}, {"constructor": "Bob"}]}`     |

(The examples are shown on one line each. Pretty output spreads those that are wider than 80 characters, as above.)

- **Numbers keep their text.** The number is a string, exactly as it was written, so `+5` keeps its plus, `1.50` its
  trailing zero and `-2.5e3` its exponent. A number is never made into a JSON number, so that none can be rounded.
- **A string** is the decoded string, with its escapes resolved and then escaped again for JSON.
- **A name** is kept as it was written, and a qualifier is part of it: `shapes::Circle` and `a.b::red`. A name is a
  `constructor` if the last part of it starts with a capital letter, and a `const` if it starts with a lower-case letter
  or `_`. A const has no body.
- **The members of a record** keep the order in which they were written, and so do the named arguments.
- **A constructor** has the key `constructor` and then at most one key for its body, `record`, `tuple` or `array`. An
  empty body is kept: `Ping()` is `"tuple": []`, `Move {}` is `"record": {}` and `Users[]` is `"array": []`, and none
  of them is the same as no body at all.
- **A tuple** has the positional arguments in `tuple`, and the named ones, if there are any, in `named` after it. The
  grammar puts the positional arguments first.
- **The keys** of a constructor are always in the order `constructor`, its body, `named`.

### Errors of the encoding

A JSON object cannot hold a name twice. A record, a record of a constructor, or the named arguments of a tuple that
gives a name twice is an error at the second, and the conversion stops. The messages are those of the type check:
`Duplicate field a`, and `Argument a is given more than once in NAME`.

## The format `jsonl-exact`

`--to=jsonl-exact` writes each value of each input, in order, as the `json-exact` encoding of it on one line, with no
white space in it, and a new line at the end of each line. So a line is exactly what `--compact` gives for a value, and
the lines are the elements of the array that `--to-array` gives for `json-exact`.

- **Every value is converted**, so there is no first value to choose and no warning for the ones that follow.
- **No values at all give no output**, and the exit code is 0. The lines of nothing are nothing, and an empty array
  would not be a line.
- **`--compact` makes no difference**, since a line has no layout. It is accepted.
- **`--to-array` is a usage error**, exit code 2: `ERROR: --to-array does not apply to jsonl-exact, which writes each
  value on a line of its own`. An array is how one document holds many values, and a lines format has no such thing, so
  asking for both is refused and not ignored.
- **The lines are streamed.** Each is written as soon as its value is converted, so a large input does not have to be
  held as output, and a program that reads the lines can start before the conversion is over.
- **The errors are those of `json-exact`**, and the conversion stops at the first. Every input is read before any value
  is converted, so an error in the syntax of any of them, or a types file, leaves the standard output empty. An error in
  a value, such as a name given twice, comes when the value is reached, and the lines before it have been written. Every
  line that is written is complete, and the error is reported on the standard error with exit code 1.

## The format `json-typed`

`--to=json-typed` writes natural JSON, in the shapes that serde writes for the Rust types that the declarations would
be, so that a program that reads such JSON can read it. What a value is written as depends on its type, which is why the
types are needed. They decide whether `5` is `5` or `5.0`, what an omitted argument is, and which names are `Bool` and
`Option`. See [decision 37](../decisions/0037-typed-json-follows-serde.md).

### Inputs and checking

The inputs are those of the check command, in the same order: the types files of the project named by
`--project=FOLDER`, then the `FILES`, then the standard input if `--stdin` is given. The `FILES` and the standard input
may hold types files as well as values files, told apart by the first token, and every types file must come before every
values file. Only the values files give values. The prelude is loaded unless `--no-prelude` is given, and `Bool` and
`Option` are those of the prelude.

The types are checked as the check command checks them: syntax, names, and the validity checks. Then the values that are
converted are type-checked, and any error is reported as the check command reports it, with exit code 1 and nothing on
the standard output. The run stops at the first type error. The first value, the warning, `--to-array`, the layout and
`--compact` are as for `json-exact`.

### The options

| Option                            | Meaning                                                  |
|-----------------------------------|----------------------------------------------------------|
| `--project=FOLDER`                | The project whose types files come first.                |
| `--prelude`, `--no-prelude`       | Whether the prelude is loaded.                           |
| `--widening`, `--no-widening`     | Whether a `Nat` fits where an `Int` or `Float` is.       |
| `--allow-refs`, `--no-allow-refs` | Whether the value of a const may name another const.     |
| `--infer-limit=N`                 | The most nodes that the type of a const may have.        |
| `--default-limit=N`               | The most nodes that an omitted argument's default has.   |
| `--dev`, `--no-dev`               | Whether imported folders are left out of the lock check. |
| `-x`, `--no-experimental`         | The experimental features of the syntax.                 |

They mean what they mean for [the check command](check-command.md), and the keys of `[check]` of a settings file with
the same names, `prelude`, `widening`, `allow-refs`, `infer-limit`, `default-limit` and `dev`, set them. The values are
always type-checked, and the run stops at the first error, so `--no-type-check` and `--continue` are not accepted, and
nor are `--verbose` and `--print-config`. A project that imports a folder needs its lock, unless `--dev` is given, as
for the check command.

### The mapping

| A value                                  | Is written as                              |
|------------------------------------------|--------------------------------------------|
| `5`, `+5`, `-5`, `1.50`                  | `5`, `5`, `-5`, `1.50`                     |
| `"hi"`                                   | `"hi"`                                     |
| `[1, 2]`                                 | `[1, 2]`                                   |
| an untagged record `{ b: 1, a: "x" }`    | `{"b": 1, "a": "x"}`                       |
| `Bool`: `true`, `False`                  | `true`, `false`                            |
| `Option`: `None`, `null`                 | `null`                                     |
| `Option`: `Some(1)`                      | `1`                                        |
| a const reference `five`                 | the value of the const, written as here    |
| a bare constructor `Dot`                 | `"Dot"`                                    |
| a record constructor `Circle { r: 1.0 }` | `{"Circle": {"r": 1.0}}`                   |
| a tuple constructor `Pair(1, 2)`         | `{"Pair": [1, 2]}`                         |
| a tuple with one parameter `Wrapper(5)`  | `{"Wrapper": 5}`                           |
| a tuple with none, `Ping()`              | `{"Ping": []}`                             |
| an array constructor `Items[1, 2]`       | `{"Items": [1, 2]}`                        |

- **Names.** The name of a constructor is the one that its enum declares, whatever project it is in and however the
  importing project has hidden or renamed it. `lib::Circle` from the project `com.example.lib` is `"Circle"`.
- **Enums.** A constructor with no body is a string, and any other is an object with the name for its one key. So an
  enum of bare constructors is a string, as in `"Fast"`, and a bare constructor of an enum that has others with fields,
  such as `Dot`, is a string too.
- **Tuples.** The arguments are written in the order of the parameters, with no names, and an argument that was left out
  is written as its default. A tuple with one parameter is written as that value alone, as serde writes a newtype
  variant, and one with none is `[]`.
- **Records.** The fields are written in the order of the declaration, wherever they were written. An untagged record
  has the type that its value gives it, so it keeps the order that it was written in, and a field of the type
  `{ w: Float, h: Float }` has the order of that type.
- **Numbers.** A number is written as the text that it was written as, so that one of any size is exact, and what a
  reader can hold is the reader's limit. A `+` is dropped and so are the zeros in front of the whole part, since JSON
  allows neither (`+5` is `5` and `007` is `7`). A number in a `Float` slot that has no point or exponent gets `.0`, so
  `1` where a `Float` is expected is `1.0`, and a typed reader sees a float. The slot is the type that the value has
  when the whole of it has been checked, so in `[1, 2.5]` both are floats.
- **Bool and Option.** They are told by the project that declares them, and not by their names, so an enum of your own
  that is called `Bool` is an ordinary enum, and so are both of them under `--no-prelude`. `Some(x)` is written as `x`,
  and `None` and `null` as `null`.
- **`Some(None)`.** An `Option` inside an `Option` is written as the one that is inside, and `Some(None)` would be read
  back as `None`, so it is an error: `Some(None) cannot be written as JSON, since it would read back as None`, at the
  value, with exit code 1. `Some(null)` is the same, since `null` is `None`.
- **Omitted arguments.** The default of a type is the zero of a number (`0`, `0`, `0.0`), the empty string, the empty
  array, the record of the defaults of the fields, and the first constructor of an enum with the defaults of its fields,
  as [decision 18](../decisions/0018-an-omitted-named-argument-takes-the-default-of-its-type.md) says. For `Bool` that
  is `false`, for `Option` it is `null`, and a one-parameter tuple is written as its one value.
- **Consts.** A reference to a const is replaced by its value, which is checked again against the type that the
  reference has, so that it is written for that type. `five`, where `const five = 5;`, is `5` on its own and `5.0` where
  a `Float` is expected. Each reference to a generic const is written for its own type.

### Errors of the encoding

Besides the errors of the check, the encoding refuses a value that is nested more than 300 levels deep, counting the
consts that it refers to: `The value is nested too deeply to convert`. A chain of consts is as deep as it is long, so a
very long one is refused and not followed. It also refuses a value of more than 1,000,000 nodes in all, counting
every reference to a const as the whole of the value of the const: `The value is too large to convert`. Without this, a
const that refers twice to another, and so on for a few dozen levels, would be written out as billions of nodes.
Nothing else is an error: a value that has passed the check has a JSON form.

## The format `jsonl-typed`

`--to=jsonl-typed` writes each value of each input, in order, as the `json-typed` form of it on one line, with no white
space in it, and a new line at the end of each line. So a line is exactly what `--compact` gives for a value with
`json-typed`, and the lines are the elements of the array that `--to-array` gives. It behaves as `jsonl-exact` does,
with the inputs, the options, the settings and the checking of `json-typed`:

- **Every value is converted, and so every value is type-checked.** There is no first value to choose and no warning for
  the ones that follow.
- **The lines are streamed**, as for `jsonl-exact`. The types, and the syntax and the names of every input, are checked
  before the first value is converted, so an error in any of them leaves the standard output empty. After that the first
  error in a value, such as a type error or `Some(None)`, ends the run with exit code 1, and the lines before it have
  been written.
- **No values at all give no output**, and the exit code is 0, including where the inputs are only types files.
- **`--compact` makes no difference**, and is accepted. **`--to-array` is a usage error**, exit code 2, for the reason
  that it is for `jsonl-exact`.

## Reading back with `--from`

`--from` reads the JSON that `--to=json-exact` or `--to=jsonl-exact` writes, and writes the values as text, in the
syntax of data-values, to the standard output. `--to` and `--from` say which way the conversion goes and not which is
the input: in both cases the files are the inputs and the standard output is the output. The inputs are `FILES` and
then `--stdin`, as for `--to`. There is no option to name an output file, since the standard output can be redirected.

- **`--from=json-exact`** reads a JSON document in each input. An object is one value. An array is a list of values,
  which is what `--to-array` writes, and each element is read as a value. Anything else is an error. An array may be
  empty, and gives no values. A file with no JSON in it, empty or only white space, is an error: `Expected a JSON
  value`. That is unlike a file of data-values, which may be empty, because an empty file is not a JSON document.
- **`--from=jsonl-exact`** reads a value from each line, as `--to=jsonl-exact` writes them. A line that is empty or
  has only white space is skipped, and a line may end `\r\n`. Each other line is a JSON object. An array on a line is an
  error, since a line is one value. A file with no lines gives no values and is not an error. The positions in an error
  are those of the file, so a problem on line 5 says line 5.
- **The reading is strict.** An object has exactly the keys that `--to` writes and no others, in any order. A key that
  is repeated, missing or unknown is an error at that key, and so is a body that is not one of `record`, `tuple` and
  `array`, and `named` without a `tuple`. The members of a `record` and of `named` are kept in the order written. A
  number is a JSON string, and its text must read back as a literal of the kind that its key names, so `"+5"` is an
  `int` and is not a `nat`. A JSON number in its place is an error that says to write it as a string. A name has to be
  one that data-values can have, a const for `const` and a constructor for `constructor`, and a field name for the
  members. The lexer and parser of data-values decide all of this, and the reader holds no second grammar.
- **A file of `json-typed` is not read.** Its objects, such as `{"Circle": {"r": 1.5}}`, have no key that names a kind,
  so the error is `Unknown key "Circle"` with a note that the file may have been written with `--to=json-typed`.
  `json-typed` and `jsonl-typed` as values of `--from` say that they cannot be read back yet.
- **The values are written as they are read.** Each value ends with `;` and a new line. It is on one line if that is at
  most 80 characters wide, not counting the indentation, the name of a member or the `;`. Otherwise it is spread, with
  one member, element or argument to a line, indented by four spaces, and each of those follows the same rule. A name
  or a number is never broken. `--compact` puts every value on one line. A string is written with the escapes of JSON,
  and with `\u00XX` for any other control character, which data-values cannot hold written raw.
- **The values are written as the run goes, and it stops at the first problem.** The values before it have been
  written, and the exit code is 1. A JSON document is read whole before any value of it is written, so a syntax error
  in it writes nothing, but a bad element in an array comes after the elements before it. The lines of `jsonl-exact` are
  read as they come, one at a time, so a line from standard input is converted and written without waiting for the end
  of the input, which is not held in memory, and the lines before a bad one are written. An input that cannot be read
  part way, as when it is not valid UTF-8, is exit code 2 after the lines before that point. The inputs are read one
  at a time as well: a syntax error in an input stops the run there, and an input that cannot be read, which is exit
  code 2, is found after the values of the inputs before it.
- **The options.** `--compact`, `--stdin`, `--config` and `--cache` are accepted. `--to-array`, `-x`,
  `--no-experimental` and the options for types are usage errors, exit code 2, since they would have no effect.

What comes back is the values and the text of their numbers. It is not the text of the file that was converted:
comments, layout, the spelling of the escapes in a string and the order of the values in a lines file that was
rearranged are not kept. See [decision 38](../decisions/0038-reading-json-exact-back-is-strict.md).

## The options `--to` and `--from`

Exactly one of them is required, and giving both or neither is a usage error, exit code 2. The value of each is matched
exactly. `--from` takes `json-exact` and `jsonl-exact`, and a bare `json` or another name is `ERROR: NAME is not a
format; choose json-exact or jsonl-exact`. For `--to`, `json-exact`, `jsonl-exact`, `json-typed` and `jsonl-typed` are
the only ones that exist. Any other value is a usage error, exit code 2. A bare `json` is not a format, so that nobody
takes the exact form for the natural one by default. The message starts `ERROR: json is not a format; choose json-exact
(every kind kept, no types)` and goes on to name the others. Any other name, `jsonl` among them, gives `ERROR: NAME is
not a format; choose json-exact, jsonl-exact, json-typed or jsonl-typed`. See [decision
36](../decisions/0036-two-kinds-of-json.md).

## Settings

`--to-array` and `--compact` are choices for a run and are not settings. The configuration file has no table for the
command. With `--from` no setting is used, but the file is read and has to be valid. Otherwise it reads `[global]` and
`[experimental]`, and for the typed formats also the keys of `[check]` that are listed
above, as [the configuration file](config-file.md) says. The global options `--config` and `--cache` are accepted. The
file that `--config` names is read and has to be valid, and `--cache` has to be a folder that is not empty. The cache is
used by the typed formats for a project from a repository, and otherwise neither is used.

## Exit codes

| Code | When                                                                                          |
|------|-----------------------------------------------------------------------------------------------|
| 0    | The values were converted, with or without a warning.                                         |
| 1    | An input has a syntax error, a name is given twice, or for a document format there is no      |
|      | value to convert. With `--from`, JSON that is not valid or is not `json-exact`. For the exact |
|      | formats an input that is a types file. For the typed formats an error in the types, a type    |
|      | error, `Some(None)`, or a project that does not match its lock.                               |
| 2    | An option is not understood, `--to` and `--from` are both given or neither, the format is not |
|      | one, `--to-array` is given with a lines format or with `--from`, an option for types is given |
|      | with a format that needs none, there are no inputs, one cannot be read, the project or the    |
|      | settings file is bad, or the output cannot be written.                                        |

## Related

- [The check command](check-command.md), for how the inputs are read, and how types files and values files are told
  apart.
- [The exit codes](exit-codes.md) and [the configuration file](config-file.md).
- [Validity checks](validity-checks.md), [type checking](type-checking.md) and
  [resolving identifiers](resolving-identifiers.md), for the checks that the typed formats make before they convert.
- Decisions 18, 35, 36, 37 and 38, in `docs/decisions/`.
