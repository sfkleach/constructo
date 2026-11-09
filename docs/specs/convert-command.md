# The convert command

`constructo convert` reads data-values and writes them in another notation. This document says what it reads, what it
writes, what it prints when something is wrong, and what its exit code means. Only one format exists so far,
`json-exact`, which writes every kind of value in an explicit form and needs no types. The formats that follow are the
natural JSON that needs types (`json-typed`), JSON lines, and the conversions back, and each will be added here.

```text
constructo convert --to=FORMAT [--to-array] [--compact] [-x] [--stdin] [FILES]...
```

## Inputs

The inputs are the `FILES`, in the order that they are written, and then the standard input if `--stdin` is given,
which is processed as one more file, named `<stdin>`, after the others. A run needs at least one of them, and without
one the command line is refused as a usage error (exit code 2).

An input is read as a values file. The inputs are read one at a time. An input that cannot be read ends the run at once
with exit code 2. An input that has a syntax error, or is a types file, ends the reading with exit code 1, so that an
unreadable input after it is never reached, as for [the check command](check-command.md). The standard input is only
read if none of the files has failed.

- **A types file** is an error, whatever its place: `NAME is a types file; convert reads values files`, at line 1,
  column 1 of the file. The kind of a file is decided by its first token, as the check command says.
- **A file with no declarations and no values**, which is empty or has only comments, is ignored. The parser takes such
  a file for a types file, but as it has nothing in it the kind makes no difference.
- **Values** in a file are separated by semi-colons, as they are everywhere. There are no types, so nothing is checked
  against a schema: `[1, "x"]` is converted as it is. A name is not looked up either. A name is a const or a constructor
  by its form alone, as [decision 35](../decisions/0035-a-const-name-starts-with-a-lower-case-letter.md) says.

The prelude is not loaded and no project is read, because nothing is resolved. Of the options of the check command, only
`-x` and `--no-experimental` are accepted, since the postfix `?` has to be switched on to be parsed. A `?` is written as
the `Some(...)` that it stands for.

## One value, or all of them

- **By default the first value is converted.** It is the first value that there is, across the inputs in order, so an
  input with no values is passed over. If any value follows it, in the same input or in a later one, there is one
  warning, at the position of the first value that follows: `More values follow; only the first was converted, so use
  --to-array for all`. Only the first of them is named, and the exit code is still 0.
- **With `--to-array` every value of every input is converted**, in order, and written as one JSON array. No values at
  all give `[]`. There is no warning.
- **Without `--to-array`, no value at all is an error**: `No value to convert`, at line 1, column 1 of the first input,
  with exit code 1.

Only the values that are converted are encoded. So a duplicate name in a value that comes after the first one is not an
error unless `--to-array` is given.

## Output

The output is written to the standard output, and only if the conversion is whole: an error leaves the standard output
empty. Warnings and errors go to the standard error, in the usual form, which for a warning starts `WARNING:`. See
[the exit codes](exit-codes.md).

`--compact` writes no white space at all: `{"nat":"5"}`. Without it the output is indented by two spaces, with a space
after each `:` and `,`. A container is written on one line unless it holds a container that is not empty. So a number is
`{"nat": "5"}`, an empty array is `{"array": []}`, and an array of numbers is spread over lines:

```json
{
  "array": [
    {"nat": "1"},
    {"nat": "2"}
  ]
}
```

The output ends with a new line in both layouts. Strings are written with the escapes of RFC 8259: the quote, the
backslash and the control characters are escaped, and everything else, including what is not ASCII, is written as it is.

## The format `json-exact`

Every value is a JSON object that says what kind of value it is, so that nothing is lost, and it can be read back.

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

(The examples are shown on one line. Pretty output spreads the outer containers over lines as above.)

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

## The option `--to`

`--to` is required, and its value is matched exactly. `json-exact` is the only one that exists. Any other value is a
usage error, exit code 2. A bare `json` is not a format, so that nobody takes the exact form for the natural one by
default. The message starts `ERROR: json is not a format; choose json-exact (every kind kept, no types)` and goes on to
say that `json-typed` (natural, needs types) is not available yet.

`json-typed` on its own gives `ERROR: json-typed is not available yet; choose json-exact`, and any other name gives
`ERROR: NAME is not a format; choose json-exact`. See [decision 36](../decisions/0036-two-kinds-of-json.md).

## Settings

`--to-array` and `--compact` are choices for a run and are not settings. The configuration file has no table for the
command. It reads `[global]` and `[experimental]`, as [the configuration file](config-file.md) says. The global options
`--config` and `--cache` are accepted. The file that `--config` names is read and has to be valid, and `--cache` has to
be a folder that is not empty, but neither is used otherwise.

## Exit codes

| Code | When                                                                                          |
|------|-----------------------------------------------------------------------------------------------|
| 0    | The values were converted, with or without a warning.                                         |
| 1    | An input has a syntax error or is a types file, there is no value to convert, or a name is    |
|      | given twice.                                                                                  |
| 2    | An option is not understood, `--to` is missing or is not a format, there are no inputs, or    |
|      | one cannot be read.                                                                           |

## Related

- [The check command](check-command.md), for how the inputs are read, and how types files and values files are told
  apart.
- [The exit codes](exit-codes.md) and [the configuration file](config-file.md).
- Decisions 35 and 36, in `docs/decisions/`.
