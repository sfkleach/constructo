# Task: Adjust Tokenisation

The first iteration of the syntax (see `syntax/`) keeps identifiers simple:
ASCII only, with an uppercase first letter for type and variant names and a
lowercase first letter for field names. This implicitly makes every
lowercase-initial word unavailable as a variant or type name, so `enum` and
`null` are effectively reserved.

This task replaces that implicit arrangement with explicit rules, and deals
with Unicode. We will be pulling in some of the ideas from the Monogram project.
See https://github.com/monogram-project/monogram.

## Ideas to work through

### Reserved words

- Reserve `enum`, `null`, `true` and `false` explicitly, so they cannot be used
  as field names.
- Open question: are `true` and `false` JSON-compatibility sugar for the
  prelude variants `True` and `False` (as `null` is sugar for `None`)? If so
  they type-check only against `Bool`. The alternative is to reserve them with
  no meaning for now.
- Keep the lookahead for telling a types file from a values file trivial: a
  values file never starts with `enum`, and a types file always does.

### Escape mechanism

- Add backtick quoting to force a non-reserved interpretation, e.g.
  `` `enum` `` as a field name.
- Open question: may backticks contain only a reserved word, or any characters
  (e.g. `` `content-type` ``, `` `Name` ``)? Arbitrary contents would let
  Constructo describe JSON-style keys that the identifier rules currently
  forbid, but needs a rule for a backtick inside the name.
- Assumed to apply to field names only, since type and variant names are
  uppercase and cannot collide with the lowercase reserved words. It must work
  in both notations, e.g. `` `enum`: Int `` in a schema and `` { `enum`: 1 } ``
  in a value.
- Deferred: whether backticked names appear in the future path notation.

### Unicode

- Decide whether identifiers may contain Unicode letters.
- The upper/lowercase convention does not work for caseless scripts, so a
  replacement rule is needed for distinguishing type and variant names from
  field names.

## Deliverables

- Updated grammars in `syntax/dt_grammar_tabatkins.txt` and
  `syntax/dv_grammar_tabatkins.txt`.
- Updated examples in `syntax/examples/`, including invalid cases such as
  `{ enum: 1 }` and `enum LOL = enum;` and valid backticked cases.
- A note in `docs/vision.md` if the user-visible behaviour changes.
