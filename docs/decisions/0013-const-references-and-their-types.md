# 0013 - References to consts need a flag, and take the declared type, 2026-10-20

## Issue

**Fork.** Values and the definitions of consts may refer to consts by name. We have to decide when that is allowed,
how circular definitions are handled, and what type a reference has.

## Decision

- A values file may always refer to the consts of the types files that come before it, as it sees only their enums.
- The definition of a const may refer to another const only under `--allow-refs`. Without the flag the reference is
  an error at the reference, and the message names the flag.
- With `--allow-refs` the order of the definitions does not matter, within a file or between files. A circular
  definition is an error, reported at the reference that closes the circle together with the consts on it, e.g.
  `Circular const definition A -> B -> A`. The check does not depend on the type-check.
- The type of a reference is the type in the definition of the const, and it is never inferred from the value. A
  reference is accepted wherever a literal of that type would be, so numbers widen unless `--no-widening` is set.
- The value of each const is checked against its declared type with the checker used for values files, over every
  enum in the run. That check is skipped under `--no-type-check`. The consts are checked before the values, and a
  type error stops the run unless `--continue` is given.
- Consts are not evaluated, so there is no evaluation order: it is enough that the declared types agree.
- In this step the type is required. Type parameters are not in scope in a const.

## Consequences

- A const defined in terms of another is only as precise as the type of the other, and a type error in one
  definition does not hide the others when `--continue` is given.
- A const cannot be of a type that has no written form until untagged object types are added.

## Options and Outcome

The chosen option is the one under Decision. Alternatives considered:

- Allowing references without a flag: the flag keeps the simple case, where consts are independent, free of the cycle
  rules.
- Inferring the type of a reference from its value: this needs an evaluation order and defeats the point of declaring
  the type.
