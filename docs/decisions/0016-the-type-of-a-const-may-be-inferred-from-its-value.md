# 0016 - The type of a const may be inferred from its value, 2026-10-21

## Issue

**Fork.** Writing out the type of every const is tedious when the value says it plainly. We have to decide what type
an omitted type stands for, and what the checker does with the parts that the value leaves open, such as `[]` and
`null`.

## Decision

- The type of a const without a declared type is the type that the type-checker works out for its value, as for a
  top-level value in a values file. So `0` is a Nat, `+0` an Int, `0.0` a Float, a tagged value has the type of its
  enum and an untagged record has the record type of its fields. Numbers in an array join, so `[1, 2.5]` is `[Float]`.
- Only the type of the const itself is exact. A reference to `const x = 0;` is accepted where an Int is expected, as it
  is for `const x: Nat = 0;`, unless `--no-widening` is set.
- The types that the value leaves open are generalised: `const e = [];` has the type `[T]` for any `T`, and
  `const m = null;` has the type `Option<T>`, exactly as if the const had been written with type parameters (decision
  14). Each reference instantiates them afresh. This is sound because values are immutable and nothing is evaluated.
- A const that omits its type may not declare type parameters, because they are scoped to a type. It is an error.
- The types are worked out in dependency order, so a const may refer to one that is defined later, and a circular
  definition is an error as before. A const whose type cannot be inferred is reported once and the references to it
  are not checked against it, so that one mistake is not reported again for every const that uses it.
- If the inferred type has more than 10,000 nodes the const is refused with a message that asks for a type. Types
  share their parts, so a short value can have a type that is exponentially large when written out, and working it
  out would take as long. `--infer-limit=N` changes the limit, and the library has `CheckOptions::infer_limit`.
- In verbose mode each const of a file is listed with its type, and the inferred types are marked, e.g.
  `const count: Nat (inferred)`. They are only listed when the values are type-checked.

## Consequences

- An inferred type cannot be wider than its value, so `const x = 0;` is a Nat, and a user who wants an Int has to
  write the type.
- The type of a const can change if its value is edited, and so change the check of the values that use it.
- A limit that is too high for the value can make a run slow or use a lot of memory. That is for the user to choose.
- A failure of one const can leave the consts that refer to it with a vaguer type, but only after an error has been
  reported.

## Options and Outcome

The chosen option is the one under Decision. Alternatives considered:

- Making it an error when the value leaves a type open: simpler, but `const none = None;` would always need a type.
- Widening an inferred number to the widest type that its uses need: it makes the type depend on the uses.
