# Plan: Steps 5 and 6 of the convert subcommand, `--from=json-exact` and `--from=jsonl-exact`

This plans Steps 5 and 6 of [convert-subcommand.md](convert-subcommand.md), which has what the commands do. This
document is about how to get there from the code as it is. The two steps share the reading of a value and the writing of
data-values, and differ only in how the input is cut into values, so they are planned together. Steps 7 and 8 will read
natural JSON with the types, and build on the same parser and writer.

## What the code has today

- `convert.rs` has the tree `Json`, which can only be written, and the encoder `encode_exact`. There is no JSON parser.
  The module is already large, so new code goes in modules of its own.
- `ast::Value` is the tree of a data-value: `Nat`, `Int` and `Float` keep their text, `Str` is the decoded string, and
  `Array`, `Record` and `Tagged { name, body }`, where a body is a record, a tuple of arguments or an array. A `Pos` is
  on every node. A name is kept as it was written, qualifier included, and a bare name is a const or a constructor by
  its first letter.
- There is no writer of data-values. Nothing today turns a `Value` back into text.
- The lexer and parser are the authority on what a name, a number and a string may be. `parse_values_with` gives the
  values of a text, and its string escapes are those of JSON. A control character, which includes DEL and the C1 range,
  is refused raw in a string.
- `Format::from_name` and the `convert` arm in `main.rs` know only `--to`, and `--to` is required. The refusals of the
  options that do not apply (`--to-array` with a lines format, the typing options with a format that needs none) are
  done in that arm, and `stream_lines` and `write_failure` write and report the output.
- The inputs of `--to` are read by `convert_files`, which reads every file before converting, and an unreadable file
  gives exit code 2.

## Decisions of the owner

These are in the task, and are listed here because the design leans on them.

1. `--from` and `--to` are exclusive, one is required, and the data-values side is implicit. The output is always the
   standard output.
2. A `json-exact` document is one object, which is one value, or an array of objects, which is a list of values. An
   empty or white-space-only file is a syntax error, and `[]` is no values.
3. The data-values are laid out by the width rule of the JSON output, and `--compact` is one value to a line.
4. The output is written as values are converted, and the run stops at the first problem with exit code 1.
5. The JSON parser is our own, with positions. The reader is strict.
6. JSONL: blank lines are skipped, `\r\n` is accepted, a line is an object, an empty file is valid with no values, and
   a problem on a line stops the run there, with no reading of the whole file first.

## Design

### The JSON parser, `src/json.rs`

A recursive-descent parser of RFC 8259 that builds a tree with a position on every node:

```rust
pub struct Node { pub pos: Pos, pub kind: Kind }
pub enum Kind {
    Null, Bool(bool),
    Number(String),                  // the text, as written
    Str(String),                     // decoded
    Array(Vec<Node>),
    Object(Vec<Member>),             // in the order written
}
pub struct Member { pub key: String, pub key_pos: Pos, pub value: Node }
pub struct JsonError { pub pos: Pos, pub message: String }
pub fn parse_document(text: &str) -> Result<Node, JsonError>
pub fn parse_line(text: &str, line: u32) -> Result<Node, JsonError>
```

- A document is white space, one value and white space, and anything after the value is an error. An empty document, or
  one with only white space, is `Expected a JSON value`. A single byte order mark at the start is skipped, as the lexer
  of the values does.
- A line is parsed with the same code, with the line number of the file as the first line of its positions, so a problem
  on line 5 of a JSONL file says `line 5`. A line may not run on to the next.
- A position is a line and a column, counting characters, as `Pos` does elsewhere.
- Numbers follow the grammar of RFC 8259 exactly: no leading `+` or zeros, a digit on each side of a point, and an
  exponent with digits. They are kept as text, never parsed.
- Strings: the escapes of RFC 8259, with a pair of `\u` surrogates joined to one character. A lone surrogate cannot be
  held in a string, so it is an error, as is a raw control character below U+0020.
- The nesting is limited to 200 levels, as the other parsers limit it, and a value deeper than that is
  `JSON is nested too deeply`. The parser is recursive, so this keeps the stack bounded.
- No comments and no trailing commas. A repeated key in an object is kept by the parser and refused by the reader, which
  can name the key, so the parser stays a plain parser of JSON.
- An error has the message of what was expected: `Expected a JSON value`, `Expected "," or "}"`, `Unterminated string`,
  and so on, at the place where the parser could not go on.

### The writer of data-values, `src/values_text.rs`

`render(value: &Value, compact: bool) -> String` gives the text of one value, followed by `;` and a new line.

- **The forms.** A number is its text. A string is in quotes, with `\"` and `\\`, `\n`, `\r`, `\t`, `\b` and `\f`, and
  `\u00XX` for any other character that is a control character, which the lexer refuses raw. Everything else is as it
  is. A record is `{ a: 1, b: 2 }` and `{}` when empty, a tagged record `Move { x: 1 }` and `Move {}`, a tuple `Pair(1,
  2)`, `Span(1, end: 3)` and `Ping()`, an array `[1, 2]` and `[]`, a tagged array `Items[1, 2]` and `Users[]`, and a
  name is as written.
- **The layout.** The one-line text of a record, array, tuple or tagged form is measured, and if it is at most 80
  characters wide, not counting the indentation or the `;`, it is written on one line. Otherwise it is spread: the
  opening, one member, element or argument to a line, indented by four spaces, with a comma after each but the last,
  and the closing, and each part applies the same rule to itself. A member is `name: ` and then its value laid out by
  the rule. `--compact` is the one-line text of every value, whatever its width.
- **Measuring.** The one-line text is built with a limit and given up when it is passed, so a large value costs about 80
  characters of work for each level that is turned down, and not its whole size, as `flat_width` does for the JSON.
- **The check.** The writer is the half of a round trip, so it is tested against the parser: the text of any value that
  the parser accepts is parsed again and must be the same value apart from its positions, in both layouts.

### The reader of `json-exact`, `src/exact.rs`

`value_from_exact(node: &Node, file: &str) -> Result<Value, Diagnostic>` turns a tree into a `Value`, with the position
of the node on each part, and is the strict reader of the task.

- **The kind.** If the object has a `constructor` key it is a constructor, whose other keys are its body (`record`,
  `tuple` or `array`, at most one) and `named`, which needs `tuple`. Otherwise it has exactly one of `nat`, `int`,
  `float`, `string`, `array`, `record` and `const`. `array` is both a kind and the body of a constructor, so the key
  `constructor` decides which it is. Any other key, a missing key, two kinds, or a repeated key is an error at the key.
- **The members.** A `record` and a `named` are objects, whose members are in the order written and whose names must be
  field names. A `tuple` and an `array` are arrays of values. Each part is read by the same function, so a part that is
  wrong is reported where it is.
- **Numbers.** The value of `nat`, `int` or `float` must be a JSON string, and its text is checked by running the lexer
  on it: it must be one token of that kind. So `+5` is an `int`, `5` is a `nat`, and `1.50` is a `float`. A JSON number
  is an error with the hint to write it as a string.
- **Names.** A `const`, `constructor` or field name is checked by parsing it with the real parser: `NAME;` must give
  one value, a name of the right kind with the same text and no body, and `{ NAME: 0 }` must give a record with a member
  of that name. The grammar of names, qualifiers and reserved words is the parser's, and is not written a second time.
- **The hint.** An object with no kind key gets `Unknown key "Circle"`, and where it may be `json-typed`, says so.
- **Not an object.** A value that is not an object, as a number or a string, is
  `Expected an object for a value`, and an array is allowed only at the top of a `json-exact` document.

### The command

- `Format` is for `--to`. A `From` enum of `JsonExact` and `JsonlExact` is for `--from`, with `from_name` that matches
  exactly. `json-typed` and `jsonl-typed` say that they are not available yet, and a bare `json` is not a format.
- In the arguments, `--to` and `--from` are an argument group that needs exactly one, so clap refuses none or both with
  exit code 2. The `convert` arm becomes a match on which was given.
- With `--from`, `--to-array`, `-x`, `--no-experimental` and the typing options are refused as usage errors, as they are
  for the formats that do not take them, and `--compact`, `--stdin`, `--config` and `--cache` are accepted.
- The library has `convert_from_source(source, from, compact, out) -> Result<(), LinesError>`, which converts one input
  and writes its values, so that the command reads an input, converts it and goes on to the next. An unreadable later
  input is then found after the output of the earlier ones and is exit code 2, and a syntax error in an earlier one is
  found first and is exit code 1, as for `--to`. `LinesError` is reused: `Invalid` for the diagnostics and `Write` for
  the output.
- The values are written to the standard output through the buffered writer of `stream_lines`, which is flushed before
  an error is reported.

### The framing

- **`json-exact`.** `parse_document` on the text, then an object is one value, and an array is each element read as a
  value. The document is parsed whole, so a syntax error comes before any value of it is written.
- **`jsonl-exact`.** The text is cut into lines at `\n`, a `\r` at the end of a line is dropped, a line with only white
  space is skipped, and each other line is `parse_line` and read as an object, one line at a time. A line that parses to
  an array is `Expected an object for a value`. An empty file has no lines. There is no pre-pass.

## Steps

### Step 5a: the JSON parser

1. `src/json.rs`, as above, with unit tests: each kind of value, nesting and the limit, every escape and the surrogate
   pairs, a lone surrogate, a raw control character, each way a number can be wrong (`+1`, `01`, `1.`, `.5`, `1e`),
   white space and a byte order mark, an empty document, text after the value, an unterminated string, array and
   object, a repeated key kept in order, positions of nodes and keys on one line and several, and the position of an
   error. A round trip: every `Json` that `convert.rs` can write, in both layouts, is parsed back to the same tree.

### Step 5b: the writer of data-values

1. `src/values_text.rs`, as above, with unit tests for each form, empty and not, the escapes, the layout at the width,
   the widths counted without indentation, nesting, and `--compact`.
2. The round-trip test: every valid file of `syntax/examples/values` is parsed, written in both layouts, parsed again,
   and compared without positions.

### Step 5c: the reader of `json-exact`

1. `src/exact.rs`, with a unit test for each kind of value and each body, and for each error: an unknown, missing,
   repeated or doubled key, `named` without `tuple`, a number that is not a string, text that is the wrong kind of
   number, each wrong kind of name, a value that is not an object, a JSON value of the wrong type for a part, and the
   hint for `json-typed`. Each message is at the place that is wrong.
2. The strong test: every value of `syntax/examples/convert/kinds.values`, and of the values of the other convert
   examples, is encoded by `encode_exact`, written, parsed, read, and compared without positions with the original. This
   tests the writer of JSON and the reader against each other.

### Step 5d: the command

1. `From`, the argument group, the refusals, and `convert_from_source`, for `json-exact`, with unit tests of the
   framing: an object, an array, an array with a bad element after good ones, and an empty document.
2. In `main.rs`, the loop that reads an input and converts it, with the exit codes.

### Step 5e: the functional tests and the documents

1. Functional tests, with the files that `--to` writes in `syntax/examples/convert/`: a round trip of each example,
   `--compact`, the array form, an error at a position in each of the kinds, a syntax error, an empty file, a number in
   place of a string, text of the wrong kind of number, a `json-typed` file, the earlier values written before an error,
   an unreadable file after a good one, `--stdin`, and the refusals and usage errors, including `--to` and `--from`
   together or neither.
2. The spec `docs/specs/convert-command.md` gets the section on reading, `exit-codes.md` and `config-file.md` the
   changes for `--from`, and the CHANGELOG and README one line each. A decision, 38, records that the reverse is strict,
   that the parser is our own, and why the names and numbers are checked by the real lexer and parser.

### Step 6: the lines

1. `jsonl-exact` in `convert_from_source`, and unit tests: the lines, a blank line, a line of white space, `\r\n`, no
   last new line, an array on a line, a line that stops half way, an empty file, and the positions that say the line of
   the file.
2. Functional tests with the files that `--to=jsonl-exact` writes, including a round trip, the values before a bad line,
   and the spec section.

## Risks

- **A second grammar.** The reader must not carry its own idea of a name or a number. It asks the lexer and parser, so a
  change to the grammar changes it too. The cost is a parse of a small string for each name and number, which is small
  against reading the file.
- **Stack depth.** The JSON parser, the reader and the writer are all recursive. The parser limits the nesting to 200,
  so the reader is at most that deep, and the writer's depth is that of the value it was given.
- **Escapes.** A character that JSON allows raw and the values lexer refuses, such as DEL, would write a file that does
  not parse. The writer escapes every control character, and the round-trip test over the examples, which include
  such characters in strings, is what shows it.
- **Positions in a line.** A JSONL line is parsed on its own, so the first line of its positions has to be given, or the
  errors on every line would say line 1.

## Verification

- `cargo test`, `cargo clippy --all-targets`, `cargo fmt --check`.
- `bash tests/functest.sh _build/target/debug/constructo`.
- `python3 -I .claude/skills/review-specs/check_specs.py`.
- By hand: convert each example to JSON with `--to` and back with `--from`, and check the result with `check`, which is
  the reader that the output is for. Compare the JSON that `--to` writes from the result with the first.

## Size

The same size as Step 3: three new modules, a change to the command, and a new section of the spec. It could be two pull
requests: 5a to 5c, which add the parser, the writer and the reader and change nothing for the user, and then 5d, 5e
and 6, which add `--from`.
