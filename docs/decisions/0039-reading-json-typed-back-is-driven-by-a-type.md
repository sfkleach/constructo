# 0039 - Reading json-typed back is driven by a type, 2026-11-10

## Issue

**Fork.** `constructo convert --from=json-typed` reads the natural JSON that `--to=json-typed` writes. That JSON does
not say what it is: `5` is a `Nat`, an `Int` or a `Float` as the schema decides, and `"Dot"` is a constructor of some
enum. There are choices about how the type is given, how the JSON is told from the types among the inputs, how
numbers and names are written, and what is read into memory.

## Decision

- **`--type=TYPE` gives the type of each value**, and is required with the typed formats and refused with the others.
  `TYPE` is any type of the grammar of the types, resolved in the root project. A document is one value of it, and an
  array is read as the value only where the type is an array type.
- **The type is resolved as a declaration.** The expression is put into a source of the root project that the user
  never sees, `enum ConstructoTarget = ConstructoTarget(TYPE);`, so that the parsing, the resolution of imports and
  generics, and the validity checks are those of a types file and are not written a second time. The name is chosen
  not to clash with any that a source mentions. A problem in it is reported in the file `--type`, with the position in
  what was written.
- **The reader runs inside the check.** JSON inputs cannot be sources of the check, so a third mode of the pipeline
  hands the checked declarations, the target type and the way to write a name to a function of the caller, which reads
  the JSON. Everything before it, the resolution, the validity checks, the consts, the limits, is shared with `check`.
- **The types come from `--project` and from the files that come first.** The first files are types files, and they end
  at the first of a `--` and a file that does not start a declaration. A file is looked at only as far as its first
  token, so that JSON of any size is never read to find out what it is. An empty file is an ignored types file before
  the first JSON file, and JSON after it, and `--` makes the sides strict. The position of `--` is found from the
  arguments, since the indices that clap gives count options and their values and not the marker.
- **The reading is strict and follows the type.** Keys are exact, in any order, and an unknown or repeated key is an
  error at the key. A number keeps its digits and takes the text of its type, `5` as `+5` or `5.0`, and is refused where
  the type cannot hold it. A name is written as the root project can write it, and the names of the prelude are bare.
  Each value is checked against the type with the check of `check`, before it is written.
- **A record's fields are all required.** The decision of the draft was that a field with a default may be left out.
  The check requires every field of a record, so a text that left one out would be refused. Only the trailing named
  parameters of a tuple can be left out, since the checker allows that.
- **A document is read whole, and the lines of a lines file one at a time.** The values are written as the run goes and
  it stops at the first problem, as for `--from=json-exact`. JSON files are opened one at a time, after the types.

## Consequences

- `--from=jsonl-typed` has no memory cost beyond a line. A `json-typed` document is one value, and costs its size. A
  streaming reader for one very large document would be separate work.
- A name from another project is written in full, as `com.example.lib::Circle`, which is longer than a group would let
  one write it, and is always accepted by `check` in the project.
- `Resolution::show`, which writes the names of a message as a file can write them, is the one rule for names, in
  messages and in the values that are written.
- A types file among the JSON fails as JSON when it is reached, which is the first thing it is, and not as a file of
  the wrong kind. The error is `Expected a JSON value`.

## Options and Outcome

The chosen option is the one under Decision. Alternatives considered:

- A second resolver for type expressions: it would copy the resolution of generics, imports, hidden and renamed names
  and their errors, and would drift from it.
- Reading every file first to say whether it is types or JSON: simple, and it reads terabytes of JSON before the types
  are known.
- Only a declared name as the type: shorter to implement, and `[Shape]` and `Option<Shape>` are what a person wants.
- Leaving out a field that has a default: it would write text that `check` refuses.
