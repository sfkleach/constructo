# 0012 - Consts share the constructor namespace, 2026-10-20

## Issue

**Fork.** A `const` is a new kind of top-level definition, and a bare name in a value, such as `Red`, may now be a
constructor or a const. We have to decide which namespace the names of consts live in, and whether `const` is a
reserved word.

## Factors

- Rust keeps types and values apart: a type and a value may share a name, but two values may not.
- A bare name in a value must mean one thing, so that it can be resolved without looking at the types.
- Constructors are values, so they already live in the value namespace, apart from the type names (decision 4).

## Decision

- A const lives in the same namespace as the constructors, which is the value namespace. A const may therefore not
  share its name with a constructor or another const, in the same file or another one. As for constructors, a const
  cannot be named after a basic type.
- A const may share its name with an enum type, as a constructor may. For instance
  `enum Colour = Rgb(Nat, Nat, Nat); const Colour: Colour = Rgb(255, 0, 0);` is valid.
- A const name may be of either case, so `limit` and `Limit` are both allowed. A lowercase name in a value can only be a
  const, and takes no fields. `null` is a literal, so it is not accepted as a const name yet.
- `const` is a reserved word everywhere, so it cannot be a field name. That keeps the classification of a file a test of
  its first token: `enum` or `const` means a types file.
- A bare uppercase name in a value is looked up as a constructor first and then as a const. The order is immaterial
  because the namespace makes them disjoint.

## Consequences

- Adding a constructor can clash with an existing const, in another file of the same run.
- A values file that used `const` as a field name no longer parses.
- The message for an unknown uppercase name still says "constructor", even though it may have been meant as a const.

## Options and Outcome

The chosen option is the one under Decision. Alternatives considered:

- A separate namespace for consts: a bare `Red` would be ambiguous when both exist.
- Consts restricted to lowercase or upper-case-with-underscores names, which keeps them apart lexically: rejected
  because mixed case is wanted, and this is not needed once the namespace is shared.
- Rejecting a const named like an enum type: stricter than Rust, which keeps types and values apart.
