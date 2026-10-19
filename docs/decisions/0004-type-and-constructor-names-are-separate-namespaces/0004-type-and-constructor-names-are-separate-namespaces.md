# 0004 - Type and constructor names are separate namespaces, 2026-10-18

## Issue

**Fork.** Should a type and a constructor be allowed to share a name, as in `enum Config = Config {...}`
(used throughout `docs/vision.md`), and may a constructor be named after a basic type?

## Decision

Separate namespaces for types and constructors. Neither may be a basic type name (`Nat`, `Int`, `Float`,
`String`). `Config`/`Config` is therefore legal, while `enum E = Int;` and `enum Nat = A;` are errors.

## Consequences

A value such as `Config { ... }` can never be mistaken for a type, because the two appear in different
syntactic positions. Forbidding basic-type names for constructors keeps the names unambiguous for readers and for
future tooling.

## Options and Outcome

The chosen option is the one under Decision. Alternatives:

- Single namespace: simpler rule, but breaks the vision document's own example.
- Separate, with only type names restricted: permits a constructor called `Nat`, which is legal but confusing.
