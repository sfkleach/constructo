# 0014 - Consts may be generic and the prelude defines null, true and false, 2026-10-21

## Issue

**Fork.** The literal `null` is a stopgap that ties the type-checker to the name `Option` (decision 10). It is
to be replaced by a const in the prelude, but `None` can be an `Option` of any type, so the type of that const needs a
type parameter. `true` and `false` are wanted for `Bool` as well.

## Decision

- The prelude declares `const null<T>: Option<T> = None;`, `const true: Bool = True;` and `const false: Bool = False;`.
  They are ordinary names in values, and the `null` literal is removed from the parser and the type-checker. Under
  `--no-prelude` they are unknown consts, as `None` is unknown.
- A const may declare type parameters with the syntax of an enum: `const NAME<T, ...>: Type = value;`. The rules for
  the parameters are those for an enum: they are distinct, they do not clash with a basic type or an enum, and each is
  used in the type unless its name starts with an underscore. They are in scope in the type of that const only.
- A reference to a generic const instantiates its parameters with fresh type variables, so that two uses of `null`
  in a value may have different types. The arguments cannot be written at a use.
- The value of a generic const is checked with the parameters as unknown types. `None` is an `Option<T>` and
  `Some(1)` is not, so the value is good whatever type is chosen, and a reference can never be wrongly typed.
- A reference to a const from the prelude is allowed in the definition of any const, without `--allow-refs`. The
  consts of the prelude refer to nothing, so they cannot be in a cycle. This is a holding position until the rule
  about leaf consts replaces it (see `docs/tasks/todo/2026-10-21-const-reference-limits`).
- A const named `null`, `true` or `false` is a duplicate of the prelude's const, as for any other prelude name.

## Consequences

- The special case for `null` in the type-checker is gone, and with it the dependence on the name `Option` when
  checking a literal. The prelude still defines `null` in terms of `Option`.
- A bare reference to a generic const in a value that is otherwise unconstrained, such as `null;` on its own, is left
  with a free type variable, as `None;` is.
- The prelude is identified by its source name when a reference is checked, which the user could in principle give
  to a file of their own. This is accepted for a holding position.

## Options and Outcome

The chosen option is the one under Decision. Alternatives considered:

- A wildcard `const null: Option<_> = None;`. Rust rejects `_` in the type of a const, and the value would have to be
  checked against an unknown type in any case, so it is the same as a type parameter but harder to explain.
- Keeping `null` as a literal that is special-cased to `Option`: it does not extend to other types or to `true` and
  `false`.
