# 0008 - A file is classified as types or values by its first token, 2026-10-20

## Issue

**Fork.** The tool takes files of two kinds, data-types and data-values, with no file extension to rely on and with
standard input as an input too. Files that hold both are planned, so the rule must not depend on the name of the file.

## Decision

The first token decides. A types file always starts with `enum` and a values file never does. A file with no tokens
(empty, or only comments) is treated as a types file, which is how it was handled before values files existed. The kind
is known even when the file does not parse, and is shown in verbose mode as `OK: <name> (data-values)`.

Each input is parsed with the grammar that its kind selects. In this step the values in a values file are only checked
for being well-formed. They are not yet checked against the types.

## Consequences

- A values file that starts with the identifier `enum`, or a types file that starts with anything else, gets the
  syntax errors of the other grammar. For example `Expected 'enum'` for a stray value at the start of a types file.
- The decision is one token of lookahead, so it works for standard input.
- When types and values are allowed in the same file, this rule will have to give way to a per-item decision. That is
  accepted for now.
- The kind label in verbose mode makes a misclassification visible.

## Options and Outcome

The chosen option is the one under Decision. Alternatives:

- File extension: does not work for standard input or for files that hold both kinds.
- Treat an empty file as values, or reject it: changes how empty files were already handled.
