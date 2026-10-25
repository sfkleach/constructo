# 0019 - A postfix `?` on a type is shorthand for `Option`, 2026-10-25

## Issue

**Fork.** `Option<T>` is the most common generic type, and writing it out makes the types of a schema longer and
noisier than they need to be. A shorthand is wanted, which also has to decide what it means when there is no prelude,
how it combines with itself, and how it appears in messages.

## Decision

- A postfix `?` after a type makes it optional. `T?` is exactly `Option<T>`, and the parser rewrites it, so nothing
  after parsing can tell how the type was written.
- It applies to the whole type before it, whether a name with its arguments, an array or a record: `Box<Nat>?` is
  `Option<Box<Nat>>`, `[Nat]?` is `Option<[Nat]>` and `[Nat?]` is `[Option<Nat>]`.
- It can be repeated, and `Nat??` is `Option<Option<Nat>>`. There are no special cases.
- It is an ordinary token, so white space and comments may come before it.
- It stands for the name `Option`, so it needs one to be declared. Under `--no-prelude` the usual unknown-type
  error is reported, and an `Option` that the programmer declares works as well.
- Messages show `Option<Nat>`, however the type was written. The rewritten type has the position of the start of its
  operand.
- `Option<T>` defaults to `None` (decision 18), so a named parameter of this type may be left out.
- Each `?` counts towards the nesting limit of the parser, since a long run of them would otherwise build a type that
  is nested too deeply to process.

## Consequences

- A types file reads as the schema of a JSON document, with `?` marking the members that may be missing.
- The sugar adds no concept to the checker. A change to the prelude's `Option` changes the meaning of `?`.
- The shorthand is stable and not experimental. The matching shorthand for values is, see decision 20.

## Options and Outcome

The chosen option is the one under Decision. Alternatives considered:

- A built-in `Option` that `?` always means: simpler to explain, but then `--no-prelude` would no longer remove
  everything that the prelude provides.
- Forbidding `Nat??`: it guards against a typo, but `Option<Option<T>>` is meaningful and legal already, and it would
  need a special case in the grammar.
- Echoing the written form in messages: friendlier, but the syntax tree would have to remember how each type was
  spelled.
