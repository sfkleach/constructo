# Plan: Step 1 of the convert subcommand, `--to=json-exact`

This plans Step 1 of [convert-subcommand.md](convert-subcommand.md). The work is on a branch that is stacked on
`consts_are_lowercase`, because `json-exact` writes a bare name as a constructor or a const by its form alone, which
that change (decision 35) makes true.

## What the code has today

- `parse_file` in `src/parser.rs` classifies a source by its first token and parses it. `parse_values_with` gives the
  values of a values file as `Vec<Value>`, with the experimental features that are switched on. A `Value` has a `pos`
  and a `ValueKind`: `Nat`, `Int` and `Float` keep their text, `Str` is the decoded string, and `Array`, `Record` and
  `Tagged { name, body }`, where the body is a `Record`, a `Tuple` of `Argument`s (positional or named) or an `Array`.
  A name that starts with a lower-case letter or `_` is a const, and one with a capital is a constructor. The postfix
  `?` has already become `Some(...)` when the value is parsed.
- `check` in `src/main.rs` reads the inputs one at a time, stops at the first syntax error, and reads the standard input
  last. An unreadable input is exit code 2. The processing is in the library, and the command only reads, calls it and
  prints.
- `Diagnostic` in `src/diagnostics.rs` always prints `ERROR:`. It is made only with `Diagnostic::new`, so adding a
  field to it touches that file only.
- The settings file is read for `[global]`, `[experimental]` and `[check]`, and `experimental(...)` in `main.rs` applies
  `-x` and `--no-experimental` to the settings of the file.
- There is no JSON code and no JSON dependency. `serde` is there for the TOML.

## Design

### Where it lives

- A new module `src/convert.rs`, public from the library as the check is, so that `convert` can be used without the
  command line:

  ```rust
  pub enum Format { JsonExact }
  pub struct ConvertOptions {
      pub format: Format,
      pub to_array: bool,
      pub compact: bool,
      pub experimental: Experimental,
  }
  pub struct Converted { pub output: String, pub warnings: Vec<Diagnostic> }
  pub fn convert(sources: &[Source], options: &ConvertOptions) -> Result<Converted, Diagnostic>
  ```

  `convert` is given the inputs already read, in order. It stops at the first error, which it returns, and so nothing
  is ever half written. The same module will hold `json-typed` and the other formats later.
- The command is `Command::Convert` in `main.rs`, which reads the inputs as `check` does, calls `convert`, writes the
  warnings to the standard error and the output to the standard output.
- No JSON library for now. The values of `json-exact` need only strings, arrays and objects, and a small writer is
  shorter than the dependency and gives control of the layout. The reverse direction (Step 5) needs a parser, and the
  choice of `serde_json` or one of our own can be made then.

### A small JSON tree and its writer

`convert.rs` builds a tree with three kinds, `Str(String)`, `Array(Vec<Json>)` and `Object(Vec<(String, Json)>)`, and
writes it:

- Strings are written with the escapes of RFC 8259: `"` and `\`, the control characters below U+0020 as `\n`, `\t` and
  the like or `\u00XX`, and everything else, including non-ASCII, as it is in UTF-8.
- `--compact` writes it with no white space: `{"nat":"5"}`.
- The default writes it with two spaces of indentation, with one rule to keep the output short: a container that holds
  no other container is written on one line, with `": "` and `", "`. So a number is `{"nat": "5"}` and not five lines,
  and the layout of a value is

  ```json
  {
    "array": [
      {"nat": "1"},
      {"nat": "2"}
    ]
  }
  ```

- The output ends with a new line.

### The encoding

A function from a `Value` to the tree, following the table of Step 1:

- `Nat`, `Int` and `Float` are `{"nat": text}`, `{"int": text}` and `{"float": text}`, with the text as it was written,
  so `+5` keeps its plus. `Str` is `{"string": s}`.
- `Array` is `{"array": [...]}`. An untagged `Record` is `{"record": {...}}`, with the members in the order written.
- A `Tagged` value with a lower-case or `_` initial is `{"const": name}`. One with a capital is `{"constructor": name}`
  with no body, or with a second key `"record"`, `"tuple"` or `"array"` for its body. The keys come in that order.
- A tuple is `"tuple": [positional...]`, and if it has named arguments, a `"named": {...}` object after it. The
  positional ones come first by the grammar. An empty tuple is `"tuple": []` and an empty record `"record": {}`.
- A name is the text as written, qualifier included.

The only checks are the ones that the encoding needs, because a JSON object cannot hold a name twice: a record, a
tagged record or the named arguments of a tuple that has a name twice is an error at the second, `Duplicate field a` or
`Argument a is given more than once in NAME`, as `check` words them. Nothing else is checked: there are no types, so
`[1, "x"]` is converted as it is.

### The inputs and the result

- The inputs are `FILES...` and `--stdin`, in that order, as for `check`: a file that cannot be read is exit code 2 and
  stops the run, a syntax error is exit code 1 and stops it, and the standard input is only read if nothing has failed.
  With no inputs it is a usage error, exit code 2.
- A source is classified by `parse_file`. A types file is
  `ERROR: NAME is a types file; convert reads values files (NAME, line 1, col 1)`, exit code 1, at the first one. A
  file with no declarations and no values, which is empty or has only comments and which the parser takes for a types
  file, is not one: it is ignored.
- The first value of the first input that has one is converted. The first value after it, in any input, gets a
  `WARNING:` at its position, and only that one:
  `More values follow; only the first was converted, so use --to-array for all`. The exit code is still 0.
- With `--to-array` every value of every input is in an array, in order, and no values gives `[]`. Without it, no
  value at all is `ERROR: No value to convert (NAME, line 1, col 1)`, with the first input named, exit code 1.

### The warning

`Diagnostic` gets a `severity`, `Error` or `Warning`, defaulting to `Error` in `new`, and `Diagnostic::warning` as a
second constructor. `Display` writes `WARNING:` or `ERROR:`. Nothing that `check` does changes. The `lint` task will use
the same form.

### The command line

```text
constructo convert --to=FORMAT [--to-array] [--compact] [-x] [--stdin] [FILES]...
```

- `--to` is required. Its value is matched exactly. `json-exact` is the only format that exists, and any other value,
  including a bare `json`, is an exit code 2 error that lists the choices and says what `json` would have meant:

  ```text
  ERROR: json is not a format; choose json-exact (every kind kept, no types) or json-typed (natural, needs types)
  ```

  At this step `json-typed` is not available, and the message says so, as the task describes.
- `--to-array` and `--compact` are options of this subcommand and are not settings. `--compact` has no effect on the
  JSONL formats that come later. The settings file has no table of its own for `convert`; it reads `[global]` and
  `[experimental]`.
- `-x` and `--no-experimental` are as for `check`, because the postfix `?` has to be switched on to be parsed. It is
  written as the `Some(...)` that it stands for.
- `--config` and `--cache` are global options, so they are accepted. The file named by `--config` is read and has to be
  valid, as for `project init`, and is not used otherwise.
- None of `check`'s options for projects, the prelude, consts and typing are accepted.

## Steps

### Step 1a: the warning form

1. Add `Severity` and `Diagnostic::warning`, and have `Display` use them. Unit tests: a warning prints `WARNING:` with
   the file, position and note, and every existing diagnostic still prints `ERROR:`.

### Step 1b: the encoder

1. `src/convert.rs`: the tree, the writer with the escapes and the two layouts, the encoding of a value, and the
   duplicate checks.
2. Unit tests, each on text that is parsed and encoded:
   - every row of the table in the task: the three numbers (the plus kept), a string with escapes and non-ASCII, an
     array, an empty array, a record, a const, a bare constructor, a record, tuple, tuple with named arguments, empty
     tuple, empty record and array constructor, and a qualified name;
   - the postfix `?` under `-x`;
   - a nested value, for the layout, in both the default and `--compact` forms, and the new line at the end;
   - the one-line rule: a leaf node is on one line, a container with a container in it is not;
   - a duplicate field in a record, in a tagged record and in the named arguments;
   - the key order of a constructor node: `constructor`, then the body, then `named`.

### Step 1c: the inputs and the options

1. `convert()` over the sources: the types file error, the first value and the warning, `--to-array`, no values, and
   the empty file. Unit tests for each, and for the order of the inputs.
2. `Command::Convert` in `main.rs`: the options, the format check with its message, reading the inputs with the
   exit codes of `check`, and writing the output only when there is no error.

### Step 1d: the functional tests

In a new section of `tests/functest.sh`, with files in `syntax/examples/convert/` that hold the values and the expected
JSON, compared as text:

1. A conversion of each kind of value, pretty and `--compact`.
2. Two values: the first is converted, one warning is on the standard error, the exit code is 0 and the standard output
   is only the JSON. Then the same with `--to-array`, and an input of no values with it.
3. No value: exit code 1 and an error. A types file: exit code 1. A syntax error, and a duplicate field: exit code 1.
   An unreadable file and no inputs: exit code 2. In every error the standard output is empty.
4. `--to=json`, `--to=JSON-Exact` and no `--to`: exit code 2, and the message for `json`.
5. The standard input with `--stdin`, and with a file as well.
6. Examples of the check commands, such as `check --project`, are unchanged.

### Step 1e: the documents

1. A spec, `docs/specs/convert-command.md`: the inputs, the options, the one-value rule and the warning, the layout, the
   encoding with the table, the errors and the exit codes. It names `json-typed` as to come.
2. Decision 36: the two JSON strategies and why, the explicit form of `json-exact`, the warning, and that the format is
   matched exactly and there is no bare `json`.
3. `docs/specs/exit-codes.md`, which says that there are no warnings: add that a warning is a `WARNING:` line, that a
   run that only warns exits with 0, and a section for `convert`. `check-command.md` is about `check`, and does not
   change.
   `docs/specs/config-file.md`, which lists what each subcommand reads: `convert` reads `[global]` and `[experimental]`.
4. The CHANGELOG, and the README line about the subcommands.
5. `/review-specs`, and the task document: tick Step 1.

## Decisions of the owner

1. **The pretty layout.** A container that holds no container is written on one line, so `{"nat": "5"}` is one line.
2. **More than one error.** `convert` stops at the first error.
3. **The first value.** The first value that exists, across the inputs in order, is converted, so an empty file is
   skipped. If no input has a value it is an error, `No value to convert`, unless `--to-array` is given, and then the
   output is `[]`.
4. **Settings.** `--to-array` and `--compact` are choices for a run and not settings, so the settings file has no table
   for `convert` at this step.

## Verification

- `cargo test`, `cargo clippy --all-targets`, `cargo fmt --check`.
- `bash tests/functest.sh _build/target/debug/constructo`.
- `python3 -I .claude/skills/review-specs/check_specs.py`.
- By hand: convert the values of `syntax/examples/values/` that parse, and read the output for the shapes.

## Size

A module of a few hundred lines with its tests, the subcommand, a field on `Diagnostic`, a spec, a decision and a new
section of the functests. It is a pull request of its own, stacked on `consts_are_lowercase`.
