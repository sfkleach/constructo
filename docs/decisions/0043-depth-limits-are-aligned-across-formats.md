# 0043 - Depth limits are aligned across formats, 2026-11-14
> Amends [decision 38](0038-reading-json-exact-back-is-strict.md) and
> [decision 41](0041-reading-xml-back-is-strict-with-a-parser-of-our-own.md), whose parsers had a nesting limit of
> 200 levels of their own syntax.

## Issue

**Fork.** Each recursive parser had a limit of 200 levels, but the levels were of different things. The parser of
data-values counts values. The JSON parser counts arrays and objects, and `json-exact` spends two of them on a value, as
in `{"array": [...]}`. The XML parser counts elements, and `xml` spends up to three on a value, as in
`<constructor><record><field>`, with the root `values` one more. So a value that `check` accepts could be written with
`--to=json-exact` or `--to=xml` and then refused by `--from` with the same format: at about 100 levels for JSON and 66
for XML. Code review of PR 21 found this for XML, and measuring found it for JSON too.

The limits were also spread over the modules that used them.

## Factors

- A limit exists for one of two reasons only: to stop a recursive part of the program from growing its stack until the
  process crashes, or to stop a small input from causing an excessive amount of work. A limit that guards against
  neither is not added, whatever else it would achieve.
- If an operation can be done in practical time, it is done. So a limit should be no tighter than its protection
  needs, and the limits should be reasonably consistent with each other where that is practical. A document parser
  that refuses what the writer of its own format produces, from a value that the source parser accepted, is tighter
  than it needs to be.
- The JSON and XML parsers are general: they do not know which elements or keys are values.

## Decision

- **The limits of the JSON and XML parsers are derived from `MAX_DEPTH`,** the limit of the parser of data-values:
  `MAX_JSON_DEPTH` is `2 × MAX_DEPTH + 1` and `MAX_XML_DEPTH` is `3 × MAX_DEPTH + 1`, the most levels that the format
  spends on a value that the parser accepts, and on the array or root that holds the values. They still protect the
  stack, of the document parser and of the reader that walks its tree, which is no deeper than the tree. A change to
  `MAX_DEPTH` changes them with it.
- **No limit is put on the depth of the value that a reader produces.** A reader turns a tree that its parser has
  already bounded into a value that is no deeper, so a second limit would protect nothing. Such a limit was considered,
  so that `--from` would refuse a value that `check` would refuse for its depth, and was rejected for that reason.
- **All of the limits on input and on work are in `src/limits.rs`,** with the reasons for their values: besides the
  three above, the nesting and the node budget of the `json-typed` encoder, the depth of a type whose default is worked
  out, and the defaults of `--infer-limit` and `--default-limit`. Constants that only shape the output, such as the
  width of a line, are not limits and stay with the code that uses them.

## Consequences

- A value that `check` accepts can be written by `json-exact`, `jsonl-exact` and `xml` and read back, at every depth
  that the parser allows. The tests show it at 200 levels for each way that a value can hold another.
- A document that was not written by `convert` can be deeper in values than the parser of data-values allows: up to
  about 400 levels of arrays in `json-exact` and 600 in `xml`. `--from` reads it and writes text that `check` refuses
  as `Nesting is too deep`. That is accepted, since refusing it earlier would not protect anything.
- The same holds for `json-typed`, which can write a value deeper than 200 levels, since it writes consts as their
  values and fills in defaults. Its JSON is read back up to the JSON parser's limit.
- The parsers of JSON and XML accept deeper documents than before, which takes more stack. A release build needs about
  330 KB for the deepest value, within the 1 MB main thread of Windows. A debug build needs about 2.5 MB, more than
  the 2 MB of a test thread, so the tests of the deepest values run in a thread with a stack of 8 MB, the size of the
  main thread on Linux.

## Options and Outcome

1. Raise the limits of the JSON and XML parsers to cover the levels that they spend on a value. **Chosen.**
2. As 1, and also limit the depth of the value that each reader produces to `MAX_DEPTH`.
3. Count only the elements and keys that are values in the document parsers.
4. Make the document parsers iterative, so that they need no limit.

## Pros and Cons of Options

### Option 1: derived document limits

- Pros: a document that `convert` writes from an accepted value is never refused. The protection of the stack is kept,
  and the limits follow `MAX_DEPTH`.
- Cons: more stack is used for the deepest documents.

### Option 2: derived document limits and a limit on the value

- Pros: `--from` would never write text that `check` refuses for its depth.
- Cons: the limit protects neither the stack nor the time, which the document limits already bound, so it does not
  meet the reason for having limits.

### Option 3: count only values in the document parsers

- Cons: the general parsers would have to know the vocabulary of the formats. A chain of wrappers that are not values,
  such as `<field><field><field>...`, would be uncounted, so the stack would no longer be bounded.

### Option 4: iterative document parsers

- Pros: no limit in the document parsers.
- Cons: the readers that turn a tree into values are recursive and are bounded only by the depth of the tree, so they
  would need a limit of their own. It is more work for the same protection.
