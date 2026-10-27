# 0022 - An array variant is a distinct form with one element type, 2026-10-27

## Issue

**Fork.** An enum often has a variant that is just an array, and the tuple form `Users([User])` makes that clumsy to
write and to read. A variant could be written `Users[User]` instead, with the value `Users[u1, u2]`. We have to
decide whether that is only another spelling of `Users([User])` or a form of its own, and what goes in the brackets.

## Decision

- An array variant is a fourth form of variant, beside the bare constructor, the record and the tuple. `Users[User]`
  holds an array whose elements have type `User`, so `Users[[Nat]]` holds an array of arrays.
- It is a different form from `Users([User])`, as a record variant is from a tuple variant that takes a record
  (decision 15). A value of one is never accepted for the other, and the message says what the constructor takes:
  `Constructor Users takes an array`.
- The brackets of a declaration hold exactly one type. `Users[A, B]` and `Users[]` are syntax errors. A value may have
  any number of elements, including none, and the elements are plain values: an array has no names, so there are no
  named arguments.
- The default of an array is the empty array. So the default of an enum whose first variant is an array variant is
  that constructor with no elements, `Items[]`, and `Join[JoinList<T>]` as a first variant is not recursive.
- A postfix `?` follows the existing rules (decisions 19 and 20): `Users?` on a type is `Option<Users>`, `Nat?` inside
  the brackets makes the elements optional, and on a value `Users[u1, u2]?` is `Some(Users[u1, u2])`.

## Consequences

- The shape that is written in a value has to be the shape that was declared, for all four forms, and that is easy to
  explain and to check.
- There are two ways to declare a variant of one array. A later lint could suggest the shorter one.
- The shape of a variant is declared and cannot be recovered from a native value, so a serialiser with no mapping to
  the types has to be told which shape to write. This is already so for the bare constructor, `Ping()` and `Ping{}`,
  which all have no fields, and for a record variant `Users{user: User}` against a tuple that takes a record
  `Users({user: User})`. The array variant is one more member of that family and not a new kind of problem.
- Until serialisation has a better answer, a serialiser writes the shortest form that can hold the value: `Ping` and
  not `Ping()` or `Ping{}`, `Users[User]` and not `Users([User])`, and `User{id: ...}` and not `User({id: ...})`. The
  output then fails the check against a schema that declared the longer form, so such a schema has to be told, or
  changed.
- A variant that is an array cannot also take other arguments. That needs the tuple form, `Users([User], limit: Nat)`.

## Options and Outcome

The chosen option is the one under Decision. Alternatives considered:

- `Users[User]` as shorthand for `Users([User])`, so that both forms are the same. For arrays the two differ only in
  punctuation, so it is tempting. But a record variant cannot be shorthand for a tuple that takes a record, because
  the record has field names, and the rule that the declared brackets are the written brackets would have an
  exception for arrays.
- Allowing several types in the brackets, as a union. That would be a new kind of type, and a tuple is the way to
  say that an element has a particular shape.
