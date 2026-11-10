# Plan: Steps 7 and 8 of the convert subcommand, `--from=json-typed` and `--from=jsonl-typed`

This plans Steps 7 and 8 of [convert-subcommand.md](convert-subcommand.md), which has the decisions. This document is
about how to get there from the code as it is. The reading of a value is the same for both steps and the framing is
the only difference, so the plan is for both, and Step 8 is a small last addition, as Step 6 was.

## What the code has today

- `check_with_layout` in `src/lib.rs` parses every source as a types file or a values file, resolves the names (which
  rewrites them to canonical form such as `Circle@com.example.shapes`), validates, checks the consts, and then walks
  the values of the values files. A `ValueMode` says what it does with each value: `Check`, or `Convert`, which type-
  checks the value with `check_value_typed` and encodes it with `encode_typed`. `--to=json-typed` is
  `ValueMode::Convert`.

- A JSON file is not a values file, so it cannot be one of the sources of that pipeline. The pipeline has to be run
  for the types, and the JSON has to be read inside it, where the `Env` is alive, as the encoder is.

- `Resolution::show(file, text, files)` rewrites the canonical names in a text as they would be written in the source
  with that index: bare where a bare name resolves to that enum, and in full otherwise. It is what decides the names
  of the output, so the rule of the task (plain where it resolves, qualified where ambiguous) is already implemented,
  for messages.

- The `Env` has `declared_enum`, `constructor` and `constant`, and `Ty` has `Nat`, `Int`, `Float`, `Str`, `Array`,
  `Enum(name, args)` and `Record`. `typed.rs` already works from a node type to the shapes of an enum, with the
  arguments of the type substituted for its parameters, and it knows the defaults, with the builder and `--default-
  limit`. The reader needs the same facts in the other direction, and must share the code and not copy it.

- The prelude's `Bool` and `Option` are told by `prelude_kind` in `typed.rs`, by the canonical project name.

- The types grammar has `Parser::ty`, which parses a type expression, but only inside a declaration. There is no entry
  for a type on its own.

- `main.rs` classifies a file by `source_kind` and reads the inputs in `read_sources`, in the same way for `check` and
  `--to=json-typed`. `convert_back` handles `--from` for the exact formats, and refuses every typing option.

- `src/json.rs` parses JSON with positions, `src/exact.rs` reads the exact form, and `src/values_text.rs` writes the
  values. These are reused as they are: the typed reader builds `ast::Value`s and the writer renders them.

## Decisions of the owner

Those of the task, summarised, and one that the plan relies on:

- `--type=TYPE` is required, takes any type expression, and a document is exactly one value of it.

- The numbers are adjusted to the slot and refused where they cannot fit, the keys are strict, a field with a default
  may be left out, and the names are plain where they resolve and qualified where ambiguous.

- Types files come among the files, told apart by the first token. An empty file is an ignored types file before the
  first JSON file and is JSON after it. `--` ends the types files, and then the sides are strict.

## Design

### The type, by a synthetic declaration

A type expression must be parsed with the types grammar and resolved in the root project, with its imports, so that
`Shape`, `[Shape]` and `Option<Shape>` mean what they would in a types file of that project. Writing a second resolver
for that would copy the resolution of generics, imports, hidden and renamed names, and their errors. Instead the
expression is put into a declaration that the existing pipeline already handles. It is a types source of the root
project, which is not on disk and is named `--type`:

```text
enum ConstructoTarget = ConstructoTarget(
TYPE
);
```

- It goes through the same parse, resolution and validation as any types file, so a syntax error, an unknown name, a
  wrong number of generic arguments and a hidden name are reported by the code that reports them for a types file. The
  expression is on a line of its own, so the position of a problem in it has line 2, which is shown as line 1 of
  `--type`, and the column is the column in the expression.

- The name of the enum is chosen not to be one that the root project, or the other sources, can have:
  `ConstructoTarget`, with underscores added while any source mentions it. The target type is then the type of the one
  parameter of its one variant, read from the resolved declaration, so it has canonical names and is what the checker
  would use.

- It is a source, so it is checked with the others, and the checks that apply to a declaration apply to it: the type
  must be valid. It is not shown in a result that lists the sources, which `convert` has none of.

### The reading happens inside the pipeline

`ValueMode` gets a third mode, `Read`, which carries a function that is given what the reading needs once the types
have been checked and the `Env` built, and which returns when the reading is over:

```rust
/// What the reader of typed JSON is given: the checked declarations, the type to read as, and how to write a name.
struct ReadContext<'a, 'e> {
    env: &'e Env<'a>,
    target: Ty,
    namer: Namer<'e>,
}
```

- The function belongs to the caller, and loops over the JSON inputs: it takes one, parses it with `json.rs`, reads it
  as the target, renders each value with `values_text::render` and writes it. So the pipeline is the same as for
  `--to=json-typed` up to the point where the values would be walked, and everything before it (the resolution, the
  validity checks, the consts, the limits and `--no-prelude`) is shared. A problem in the types ends the run before
  the function is called, with nothing written, as for `--to=json-typed`.

- `Namer` wraps `Resolution::show` for the source of the root project, so that the reader asks for the written form of
  a canonical name. The names that the reader writes are those of constructors only. If the `Resolution` cannot be
  reached from the place where the mode runs, it is passed in, and that is the first thing to find out (Step 7a).

- The public entry point is `convert_typed_back(sources, layout, options, target, inputs, from, compact, out)`, in the
  library as `convert_typed` is, and it returns `LinesError` as the others do. `inputs` hands out the JSON sources one
  at a time, so that the later ones are read only when they are reached.

### The reader, `src/typed_back.rs`

`read_typed(ctx, ty, node) -> Result<Value, Diagnostic>` turns a JSON tree into a value, by the type. It is a
recursive function whose depth is that of the JSON, which the parser limits to 200, so the stack is bounded. It
follows the mapping of decision 37 in reverse:

- **`Str`** wants a JSON string. **`Array(t)`** wants a JSON array, and each element is read as `t`.
  **`Record(fields)`** wants an object, and the members are matched with the fields.

- **Numbers.** `Nat`, `Int` and `Float` want a JSON number, and its text is the text of the value, adjusted to the
  slot by one function, the inverse of `number_text`: `+` is added to a non-negative `Int`, `.0` is added to a `Float`
  that has no point, and `.0` is put before an exponent that follows a whole number. The digits are never parsed. A
  number that cannot fit is an error at the number: a point, an exponent or a minus in a `Nat`, and a point or an
  exponent in an `Int`. JSON allows no leading zeros, so none arrive.

- **Enums.** A declared enum is looked up with `declared_enum`, with the arguments of the type, and the JSON picks the
  variant: a string is a unit variant, an object with one key is a variant with a body (a record, a tuple that is an
  array, an array, or a one-parameter tuple that is unwrapped), and `{"Ping": []}` is an empty tuple. The key must be
  a variant of that enum, so an unknown one is an error at the key with the variants listed.

- **The prelude.** `Bool` is read from `true` and `false`, and `Option` from `null` and from the unwrapped value, so
  `Option<Option<T>>` reads `null` as `None` and anything else as `Some` of the inner reading. These are told by
  `prelude_kind`, as the encoder does.

- **Keys.** For a record, or a variant with fields, an unknown key is an error at the key, a missing field that has a
  default is left out of the value, and a missing field with no default is an error at the object, naming it. Whether
  a type has a default is decided by the code of the checker and of the encoder, shared and not copied, and the
  `--default-limit` applies to it. A repeated key is an error at the second.

- **Errors** say what was expected, by the type as it is shown in the checker's messages, and what was found:
  `Expected a string for Shape, found a number`. The position is that of the JSON node.

- **The value** is an `ast::Value` with the position of its node, so that it can be rendered. A constructor is
  `Tagged`, with its name written by the namer, a body, and positional and named arguments as the declaration says:
  positional ones for the unnamed parameters, then the named ones by their names.

The reader does not type-check what it builds. It builds from the type, so the result fits the type by construction. A
test checks that claim, by running `check` on the text that comes out.

### Telling the files apart, and `--`

- The types files are a prefix of the files, and the end of the prefix is found by walking the files in order and
  stopping at the first of two things: the marker `--`, or a file that has a first token that is not the start of a
  types file, which is the first JSON file. An empty file, which has no token, is a types file and is ignored, so the
  walk goes on past it. Everything from the end of the prefix on is JSON, and is not looked at again by the walk.

- A file is looked at only as far as its first token, so that the walk reads a few bytes of a JSON file and then closes
  it. The JSON files are opened again later, after the types are known, and are streamed. Nothing is read in full
  except the types files, which are small and are read whole for the check. A JSON file is never read to find out what
  it is, so a file of any size costs nothing at this stage.

- With `--`, the files before it are all types files, even an empty one, and are not peeked at: they are read whole and
  parsed as types, so a JSON file among them is a syntax error of the types parser, at its first token. The files after
  it, and `--stdin`, are all JSON, even an empty one, which is the error `Expected a JSON value`, and a types file among
  them fails as JSON when it is reached, since `enum` is not a JSON value. So the walk of a command line with `--` does
  not open a file after the marker at all.

- The walk is a pure function of the files in order, the position of `--` and a function that gives the first token of
  a file, and it gives the types files, the JSON files, or an error. It is unit-tested without the command line or the
  file system, with a fake that gives the first tokens.

- The position of `--` is found in the raw arguments, with the indices that clap gives for the values of `FILES`. The
  first `--` ends the options. A spike, in Step 7b, shows that the indices of `FILES` and of the marker are
  comparable. If they are not, the arguments are scanned by hand, as `config_occurrences` is.

- A types file or a first token that cannot be read is exit code 2 and nothing is written, since the types are
  needed before any value is read. A JSON file that cannot be read is found when it is reached, and is exit code 2
  after the values of the inputs before it have been written, as for `--from=json-exact`. A syntax error in the JSON
  of an input is found when that input is reached, so the values of the inputs before it have been written.

- `--stdin` is JSON, and comes last, so it is always after the marker and is never a types file.

### The command

- `--type` is an option of `convert`. With `--from=json-typed` or `--from=jsonl-typed` it is required, and a missing
  one is the usage error `--type is required with --from=json-typed`. With `--to` or `--from=json-exact` it is
  refused: `--type does not apply to json-exact, which needs no types`.

- `FromFormat` gets `JsonTyped` and `JsonlTyped`, which are no longer refused. `needs_types` and `is_lines` are
  defined for it as for `Format`.

- The refusals of `convert_back` depend on the format: for the typed ones the typing options are accepted, and `--to-
  array` and `-x` are refused as before. The settings of `[check]` are read as for `--to=json-typed`, with the same
  keys.

- The project is loaded by `load_projects`, as for `--to=json-typed`, with the same lock rules, `--dev` and cache.

### Step 8, the framing

The lines of `jsonl-typed` are read one at a time by the same function of the caller, with the framing of Step 6: the
lines are cut at `\n`, a trailing `\r` is dropped, a blank line is skipped, and each other line is parsed with
`parse_line` and read as the target. An array on a line is a value only if the target is an array type, which the
reader needs no rule for. An empty file has no lines, so it is no values. The values of the lines before a problem
have been written.

## Steps

### Step 7a: the type, and the hook

1. The synthetic declaration, the name that cannot clash, and the reading of the target type from the resolved
   declaration, with unit tests: a declared type, an array, an `Option`, an inline record, a generic with arguments,
   an unknown name, the wrong number of arguments, a hidden name, and a syntax error, each with the position that it
   should have in `--type`.

2. `ValueMode::Read`, the context, the namer, and `convert_typed_back` with a reader that is for now only able to say
   what the target is. The tests of `check` and of `--to=json-typed` must pass unchanged, which is the proof that the
   pipeline was not changed.

### Step 7b: the files

1. The walk that finds the end of the types files, with its tests: it stops at the first JSON file, an empty file
   before it is skipped and one after it is JSON, it stops at `--` whichever comes first, an empty file on either
   side of the marker, a JSON file before the marker, and a types file after it. The first-token function reads a
   bounded prefix, which a test shows with a file that is much larger than the prefix.

2. The spike of the indices of `--`, with a test of a command line with options before and after the files.

### Step 7c: the reader

1. `src/typed_back.rs` for the scalar types, arrays and records, with the numbers, and unit tests for each of them,
   including the adjustment of each number and each refusal.

2. The enums: a unit variant, a record, a tuple, a one-parameter tuple unwrapped, an empty tuple, an array variant,
   the prelude, a nested `Option`, a generic enum, and the keys (unknown, missing with and without a default,
   repeated). A test of each error with its position.

3. The names, by the namer: plain, qualified where it is ambiguous, with a renamed and a hidden import, and the
   prelude always plain.

4. The check of the claim that the result fits the type: for the values of the typed examples, run `--to=json-typed`
   and then the reader, and `check` the text.

### Step 7d: the command

1. `--type`, the new formats in `FromFormat`, the refusals, the walk and the lazy reading of the JSON files, and
   `--` in `main.rs`.

2. Functional tests, with the files that `--to=json-typed` writes in `syntax/examples/convert/typed/`: a round trip of
   each, the array type, `Option`, a type in a project that is named by `--project`, types files among the files, `--`
   with an empty file on each side, a number adjusted to each kind of slot, an unknown key, a missing field with and
   without a default, an unknown constructor, a type that does not exist, a JSON syntax error, the usage errors, and
   `--stdin`.

3. The spec section of `docs/specs/convert-command.md`, `exit-codes.md`, `config-file.md`, the CHANGELOG and the
   README, and decision 39, which records the synthetic declaration, the shared reading of the types and the two rules
   for numbers and keys. Tick the tasks of Step 7.

### Step 8: the lines

1. The framing of `jsonl-typed` in the function of the caller, and unit tests: the lines, a blank line, `\r\n`, no
   last new line, an array on a line for an array type and not for another, a line that stops half way, an empty file,
   and the positions that say the line of the file.

2. Functional tests with the files that `--to=jsonl-typed` writes, including a round trip and the values before a bad
   line, the spec section, and the CHANGELOG.

## Risks

- **The hook in `check_with_layout`.** It is the invasive part, since a mode that runs a caller's function at that
  point has to be given the `Resolution`, the `Env` and the target, which are made in different functions. Step 7a
  does this first and on its own, so that the risk is found before the reader is written.

- **A synthetic source in the layout.** It must be owned by the root project and be a types source, and the `Layout`
  records the owner of each source by index. It goes last among the types sources and before nothing, so the indices
  of the real sources do not move. If the root project is the plain one, which has no name, it is its source too.

- **Positions in `--type`.** The line and the column of a problem must be those of the text that the user wrote. A
  test for each kind of error.

- **A second copy of the shapes.** The reader repeats what the encoder knows about variants, defaults and the prelude.
  It uses the same functions, which are made available to both from `typed.rs` or from the checker, and does not do
  its own.

- **Stack depth.** The reader recurses as deep as the JSON, which is at most 200 levels, and the types that it follows
  are as deep as that. The builder of defaults is not needed, since a missing field is left out.

- **Size of a JSON document.** A JSON file is streamed from the start only for `jsonl-typed`. A `json-typed` document
  is parsed whole, as in Step 5, because it is one value, so its memory is the size of the document. Large data goes in
  `jsonl-typed`. A streaming reader for one large document would be separate work, and the spec says so.

## Verification

- `cargo test`, `cargo clippy --all-targets`, `cargo fmt --check`.

- `bash tests/functest.sh _build/target/debug/constructo`, with the tests of `check` and of `--to` unchanged by 7a.

- `python3 -I .claude/skills/review-specs/check_specs.py`.

- By hand: the typed examples through `--to=json-typed` and back, read as text and then checked with `check`.

## Size

The largest of the reverse steps, because of the hook in the pipeline and the reader of enums. It could be two pull
requests: 7a to 7c, which change nothing for the user, and then 7d and Step 8.
