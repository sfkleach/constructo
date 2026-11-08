# 0035 - A const name starts with a lower-case letter, 2026-11-08

## Issue

**Fork.** Decision 12 let a const have a name of either case, and put consts and constructors in one namespace. So a bare
uppercase name in a value, such as `Red`, could be a constructor or a const, and only a lookup in the declarations could
tell. The parser could not classify it, and a values file could not be read, or converted to another format, without the
types. A lowercase name was already always a const.

## Decision

- A const name starts with a lower-case letter or `_`, as a field name does. A const name that starts with a capital
  letter is a syntax error: `Expected const name that starts with a lower-case letter`.
- So the form of a bare name in a value says what it is. A name that starts with a capital letter is a constructor, and
  any other is a const, with no lookup. A tagged value with no body is always a constructor.
- A const and a constructor cannot have the same name, because they start differently. A const may still be spelt like a
  type, apart from its first letter: `enum Colour = ...;` and `const colour: Colour = ...;`.
- The prelude consts `null`, `true` and `false` are unchanged.

## Consequences

- A name in a value is classified from the text of the file alone. A later `convert` subcommand, which writes a values
  file as JSON, depends on this.
- The check for a const that has the name of a constructor, and the message `Const x is a value and takes no fields`,
  can no longer happen. A const with a body, `x { a: 1 }`, is a syntax error.
- A const that was named in capitals, or with a capital initial, has to be renamed, in the types file and in each values
  file that names it. This is a change before 0.1.0 and no compatibility is kept.
- A const may not be named `Nat`, `Int`, `Float` or `String`, since these start with a capital letter.
- The message for an unknown name in a value is now exact: `Unknown constructor X` for a capital name and `Unknown
  const x` for the others.

## Options and Outcome

The chosen option is the one under Decision. Alternatives considered:

- Leaving decision 12 as it was, and letting a converter or a reader ask the types: it makes the meaning of a values file
  depend on files that are not in it.
- Restricting consts to upper-case-with-underscores names (`MAX_WIDTH`): it would also keep them apart, but it would not
  let a const be written like the lower-case names that a values file already uses, such as `null`, `true` and `false`.
