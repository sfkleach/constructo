# 0036 - Two kinds of JSON, exact and typed, 2026-11-09

## Issue

**Fork.** `constructo convert` writes values as JSON, and there are two things that a person may want. One is JSON that
a program written for JSON can read as the data that it expects, such as `{"Circle": {"radius": 1.5}}`. That needs the
types, because a bare `5` is a `Nat` in Constructo and could be an `Int`, a `Float` or a `Nat` in the schema, and a
field with a default may have been left out. The other is JSON that keeps every kind of value, needs no types, and
can be read back to the same values, such as `{"nat": "5"}`. A single option `--to=json` would have to choose, and
whichever it chose, a person who expected the other would get a result that looks right and is not.

## Decision

- There are two formats, named for what they do: `json-exact` (every kind of value is explicit, no types are needed) and
  `json-typed` (the natural, serde-shaped JSON, which needs the types). The first is built first.
- The format is matched exactly, and there is no bare `json` and no default. A bare `json` is an error that names both.
- `json-exact` writes a number as a string, with the text that was written, so that nothing is rounded, and a plus sign
  or a trailing zero is kept. Each value is an object with one key that names its kind: `nat`, `int`, `float`, `string`,
  `array`, `record`, `const` and `constructor`. A constructor has a second key for its body, `record`, `tuple` or
  `array`, and a third, `named`, for the named arguments of a tuple. An empty body is kept as an empty one.
- By default only the first value is converted, and a `WARNING:` says if more follow. `--to-array` converts every value,
  into an array. The warning is the first diagnostic that is not an error, with `WARNING:` where an error has `ERROR:`,
  and a run that only warns exits with 0.
- A name that is given twice in a record is an error, since a JSON object cannot hold it twice, and no other check is
  made.
- The pretty layout favours reading over a rule: two spaces of indentation, and a container is on one line if its text
  on one line is at most 80 characters wide, and is otherwise spread over lines with one member on each. The width is
  of the text alone, not counting the indentation or the key, so there is no maximum column, which nesting by
  indentation would make meaningless, and a container is laid out the same wherever it is. A string is never broken.
  Two rules were tried first and dropped: one line only if the container held no container, which made the common typed
  shape `{"Circle": {"r": 1.0}}` three lines, and one line if it held at most two levels, which let an array of any
  number of members become one line.

## Consequences

- `convert` needs no types and no project for `json-exact`, and a values file is converted from its text alone, which
  decision 35 made possible.
- The spelling of the keys is part of the format, and a later `--from=json-exact` reads exactly this. What it gives
  back is the values and the text of their numbers. It is not the text of the file: comments, layout and the spelling
  of the escapes in a string, where `"a"` and `"\u0061"` are the same string, are not kept.
- The exit-codes document had said that there are no warnings. It now says that a warning is a line that starts
  `WARNING:`, and that it does not change the exit code. `check` still has none.
- `json-typed` is a larger task, because it needs the type check to be shared with `check`. Until then it is an error to
  ask for it, with a message that says so.

## Options and Outcome

The chosen option is the one under Decision. Alternatives considered:

- A single `--to=json` that is typed when there are types and exact when there are none: the meaning of the output would
  depend on what else was given, and a missing types file would change the shape of the result.
- `json-exact` with numbers as JSON numbers: shorter, but a `Nat` beyond 2^53 would be rounded by many readers, and `+5`
  and `1.50` would lose their form, so the output could not be read back to the same numbers.
- `--to=json` for the exact form and a later name for the typed one: the shorter name would go to the format that a
  person is less likely to want, and people would pick it without reading.
