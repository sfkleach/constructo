# 0037 - Typed JSON follows the conventions of serde, 2026-11-09

## Issue

**Fork.** `convert --to=json-typed` writes values as the JSON that other programs expect (decision 36). There is no one
shape for that. A Constructo value could be written as many things: a bare constructor as a string or as an object, a
tuple as an array or an object with numbered keys, `Some(x)` as `x` or `{"Some": x}`. Where there is a convention that
programs already read, following it makes the output useful without a custom reader, and the constructors of these
declarations are the enums of Rust, where the convention is serde's.

## Decision

- The shapes are those that serde writes by default for the Rust types that the declarations would be, which is the
  externally tagged form. A constructor with no body is a string, and any other is an object with the name of the
  constructor for its one key. A record constructor has an object for its value, a tuple constructor an array, a tuple
  with one parameter the value alone, as for a newtype variant, and an array constructor an array. A tuple with no
  parameters is `[]`.
- `Bool` and `Option` of the prelude are `true`, `false`, `null` and the value itself, as they are for a Rust `bool`
  and `Option`. They are told by the project that declares them (`Bool@constructo.prelude`) and not by their names.
- Where the convention is silent, the choice is made here:
  - **Defaults are written out.** An omitted argument is written as its default (decision 18), because the JSON has no
    way to say that something was left out, and a reader that fills in its own would not agree with ours.
  - **Consts are inlined.** A reference is written as its value, checked against the type of the place that it is
    used in. JSON has no references.
  - **Numbers are the text that was written**, with a `+` and the zeros in front dropped, since JSON allows neither, and
    `.0` added where a `Float` is expected and the text has no point or exponent. They are not parsed, so a number of
    any size is exact. A typed reader needs the point to see a float.
  - **Fields are in the order of the declaration**, so that one value is always the same text. An untagged record has no
    declaration and keeps the order that it was written in.
  - **`Some(None)` is refused.** It would be written as `null` and read back as `None`. Serde has the same ambiguity
    for `Option<Option<T>>`, and refusing is better than losing the value without a word.
- Only the values that are converted are type-checked, as only they are encoded.
- The types are checked as `check` checks them, with the same options, and the type of every part of a value is kept
  by the checker while it checks, so that the encoder writes each part for the type that it was found to have, and not
  for the type that it looked to have when it was first met. In `[1, 2.5]` both numbers are floats.

## Consequences

- A reader that is given the types can read the output back. One that is not has to guess: `5` could be a `Nat`, an
  `Int` or a `Float`, and `"Dot"` a string or a constructor. The reverse direction (Steps 7 and 8 of the task) needs the
  types for that reason.
- The output of two values that have different types may have the same shape, which is what makes it natural and not
  exact.
- The checker keeps a record of the type of each part of a value when it is asked to, which it did not before. The
  record is not kept for `check`, so `check` costs what it did.
- A value that is nested more than 300 levels deep, counting the consts that it refers to, is refused, so that the
  encoder, which follows them, cannot run out of stack.

## Options and Outcome

The chosen option is the one under Decision. Alternatives considered:

- **Adjacently or internally tagged** (`{"type": "Circle", "r": 1.0}`): readable, but not what serde writes by default,
  so it would need an attribute on the reading side, and it cannot hold a tuple or a bare value in the content.
- **`Some(x)` as `{"Some": x}`:** exact, but not what a program that reads JSON expects of an optional field.
- **A number as a JSON string when it is large:** it protects a reader that holds only doubles, but makes the shape
  depend on the size of the number. What a reader can hold is the reader's limit.
