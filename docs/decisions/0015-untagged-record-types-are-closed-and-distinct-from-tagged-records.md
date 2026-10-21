# 0015 - Untagged record types are closed and distinct from tagged records, 2026-10-21

## Issue

**Fork.** A const needs a type for an untagged object such as `{ left: "hand", right: "heart" }`. The type language
already had anonymous records, which were added for the fields of enum variants, so the type of that object is written
`{ left: String, right: String }`. We have to fix how such a type compares with values, and with the types of tagged
records, now that it can be the type of a const. This record is written after the behaviour was in place.

## Decision

- A record type is closed. A value has the type only if it has exactly the declared fields, in any order, as for the
  record variants of an enum (decision 10). A missing, unknown or duplicate field is an error.
- Two record types are the same type if they have the same fields with the same types, whatever the order in which
  the fields are written. A reference to a const of one is accepted where the other is expected, and the field types
  are compared one at a time with the usual widening of numbers.
- A tagged record has the type of the enum that declares its constructor and never has an untagged record type, and an
  untagged record is never a value of an enum. So `const x: { a: Nat } = Foo { a: 1 };` and `const x: Foo = { a: 1 };`
  are both errors.
- `{}` is the record with no fields, as a type and as a value. It is not a bare constructor and not `Pong {}`, which is
  a tagged record.
- The grammar needs no change: `Type` already includes a record body.

## Consequences

- A tagged record cannot be turned into an untagged one, so the tag cannot be forgotten by accident.
- There is no way to say "any record that has at least these fields". That would need open records.
- An untagged record type has no name. A type that is used more than once has to be written out each time until
  there is a way to name one.

## Options and Outcome

The chosen option is the one under Decision. Alternatives considered:

- Open record types, in which extra fields are allowed: a reference would also lose the extra fields, and it differs
  from the variants of an enum.
- Letting a tagged record be accepted by its fields: the tag would be forgotten, and it makes the type of a value
  depend on how it is used.
