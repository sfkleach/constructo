# 0010 - Typing rules for data-values, 2026-10-20

## Issue

**Fork.** The rules by which a data-value is judged well-typed need to be fixed before they are implemented.

## Decision

- A tagged value is checked against the enum that declares its constructor, which is unique (Step 2).
- Shapes must match exactly: `Quit` for a bare variant, `Ping()` for a tuple variant, `Pong {}` for a record variant,
  and a tuple has the declared arity.
- Record fields must match as a set, in any order. Missing, unknown and duplicate fields are errors.
- Type parameters are instantiated with fresh variables and unified across the whole top-level value. Array elements
  unify too, so `Cons { head: 1, tail: Cons { head: "a", tail: Nil } }` is an error.
- Numbers: `5` is a Nat, `+5` and `-5` are Ints and `5.0` is a Float. With widening, which is the default, Nat fits
  Int and Float, and Int fits Float. A negative number never fits Nat. `--no-widening` requires the exact form.
- Join: where unification meets two numeric types, widening applies and the result is the wider type. So `[1, 2.5]`
  has type `[Float]` and `[1, +2]` has type `[Int]`. With `--no-widening` there is no join and these are errors.
- Untagged top-level values are checked for internal consistency only, since they name no type.
- `null` is `None` of `Option<T>`, handled specially as a stopgap. (Superseded by decision 14: it is now a const of
  the prelude.) The planned replacement is a `const` declaration,
  `const NAME: TYPE = VALUE` with the type optional, and the prelude will then include `const null = None;`. Until
  then, without the prelude (`--no-prelude`) and without a user-defined `Option`, a `null` is reported as an error
  that names the cause.

## Consequences

- Untagged records such as `{x: 1}` never fail, even if no declared type has such a field. This is accepted.
- The join makes the outcome of unification depend on widening, so the unifier has to know the option.
- Widening to Float may lose precision for very large Nats and Ints. This is accepted.
- `null` ties the checker to the name `Option`. The `const` mechanism removes that, and with it the special case,
  and `null` will then be missing under `--no-prelude` for the same reason `None` is.

## Options and Outcome

The chosen option is the one under Decision. Alternatives:

- Check each constructor with parameters unconstrained: never reports a mixed list.
- Exact numeric forms only: a field of type Int could not take `5`.
- Require an expected type for every top-level value: no way to give one yet.
