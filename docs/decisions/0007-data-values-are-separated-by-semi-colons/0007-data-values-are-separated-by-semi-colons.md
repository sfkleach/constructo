# 0007 - Data values are separated by semi-colons, 2026-10-20

## Issue

**Fork.** A values file was a plain sequence of values with nothing between them. That is ambiguous, because
constructors take an optional record or tuple: in `Quit` followed on the next line by `{ x: 1 }` it is unclear whether
there are two values or one `Quit { x: 1 }`. Whether a constructor takes a body is really decided by the data types that
are in force, and the values grammar does not have them.

## Decision

Values are separated by semi-colons, and the last value may be followed by one too: `Value (';' Value)* ';'?`. A
file with a single value needs no semi-colon. A constructor always takes the record or tuple that directly follows it,
so writing two values where the second starts with `{` or `(` needs the semi-colon: `Quit; { x: 1 }`.

This matches the types grammar, in which every `enum` declaration ends with a semi-colon, and the optional trailing
separator matches the trailing commas allowed in records, tuples and arrays.

## Consequences

- Existing values examples gained semi-colons. A single JSON-style value is still valid unchanged.
- Whitespace and newlines are not significant, so pretty-printing does not change meaning.
- A missing separator is reported as `Expected semi-colon` at the start of the next value.
- Files that hold both types and values, which are planned, can use the same separator.

## Options and Outcome

The chosen option is the one under Decision. Alternatives:

- Juxtaposition with a greedy rule, so a tag always takes the next record or tuple: no grammar change, but it silently
  gives the wrong reading to two adjacent values.
- A tag takes a body only when it is on the same line: needs newline-sensitive lexing, which breaks pretty-printed
  input.
