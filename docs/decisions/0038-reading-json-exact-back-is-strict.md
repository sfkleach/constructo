# 0038 - Reading json-exact back is strict, with a parser of our own, 2026-11-10

## Issue

**Fork.** `constructo convert --from` reads the JSON that `--to` wrote, and writes the values as text. There are choices
about how forgiving the reader is, where its JSON parser comes from, and how it decides what a name or a number is.

## Decision

- **The reader is strict.** An object has exactly the keys that `--to` writes, in any order, and a repeated, missing or
  unknown key is an error at that key. A number must be a JSON string whose text reads back as a literal of the kind
  that its key names. A JSON number in its place is refused with a note, since accepting it would round a `Nat` that is
  larger than 2^53 and lose `+5` and `1.50`, which is what `json-exact` exists to prevent.
- **The JSON parser is our own**, in `src/json.rs`, and not a dependency. It keeps the position of every node and key,
  so that every problem is reported at its line and column, as the other diagnostics are, and a line of a JSONL file is
  parsed with the number of its line in the file. It follows RFC 8259, with a nesting limit of 200 as the other parsers
  have.
- **Names and numbers are checked by the real lexer and parser.** The text of a number is parsed as a value, and a name
  is parsed as `NAME;` or as a field of `{ NAME: 0 }`, and must come back as the same text. The reader holds no second
  grammar, so a change to the grammar changes it too. The cost is the parse of a small string for each, which is small
  against reading the file.
- **An empty file is an error for `json-exact`**, `Expected a JSON value`, because an empty file is not a JSON document,
  though it can be a file of data-values. An empty array is no values. A file of `jsonl-exact` may be empty.
- **The values are written as the run goes, and the run stops at the first problem.** The output need not be tidy
  when something is wrong. A JSON document is parsed whole first, because JSON cannot be read in part, and its values are
  then written one by one.
- **A file of `json-typed` is not read back yet.** The error for one has a note that says what it may be.

## Consequences

- What comes back is the values and the text of their numbers, and a round trip through `--to` and `--from` is exact in
  that sense. It is not the text of the original file.
- The values are written in a layout of their own, in `src/values_text.rs`, by the same rule as the pretty JSON: one line
  if it is at most 80 characters wide, not counting the indentation.
- A dependency such as `serde_json` would have given less code, and no positions without more work.

## Options and Outcome

The chosen option is the one under Decision. Alternatives considered:

- Accepting JSON numbers for `nat`, `int` and `float`: more forgiving, and lossy for the numbers that `json-exact` keeps
  exact.
- `serde_json`, with a search of the text for the position of a problem: less code, and positions that are approximate.
- A reader that has its own rules for names and numbers: faster, and it would drift from the grammar.
