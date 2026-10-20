# 0009 - Values see only the types given before them, 2026-10-20

## Issue

**Fork.** Step 6 type-checks data-values against data-types. We must decide which types are in scope for a given
values file, when all inputs are otherwise one namespace (decision 1).

## Decision

A values file is checked against the prelude plus the types files given *earlier* on the command line. Types files
given later are not visible to it. Within the visible set the scope is mutually recursive, as for `check` of types.

When mixed files are allowed, "earlier" will mean "declared above the value", which is the same rule at a finer grain.

## Consequences

- The order of arguments matters for values, though not for types. `constructo check a.types b.values` works and
  `constructo check b.values a.types` reports unknown constructors in `b.values`.
- A constructor that is not visible is reported as unknown, rather than as a type error of some other kind.
- `--stdin` counts at the position where it is processed.

## Options and Outcome

The chosen option is the one under Decision. Alternatives:

- Every types file is in scope for every values file: simpler, but contradicts the planned mixed files, where
  a value must not use a type declared below it.
- Allow mixed files now: a larger change, deferred.
