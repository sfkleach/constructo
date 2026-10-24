# 0005 - The standard prelude is an ordinary source loaded first, 2026-10-19

## Issue

**Leap.** `check` needs a standard prelude declaring `Bool` and `Option<T>`, loaded by default and disabled with
`--no-prelude`. The task describes loading it as an initial action.

## Decision

The prelude is a Constructo source text embedded in the library (`PRELUDE`), named `<prelude>` in diagnostics. The
command-line tool puts it ahead of the user's inputs, unless `--no-prelude` is given, and the library then
treats it like any other input. There is no special case in the parser or the checker.

The prelude text uses the comma-separated syntax of the grammar, `enum Bool = True, False;`. The task document
originally showed `|` between variants, which is not part of the grammar, so the task document was corrected.

Note: decision 18 later reordered the variants to `enum Bool = False, True;`.

## Consequences

- Redeclaring `Bool`, `Option`, `True`, `False`, `None` or `Some` is a duplicate error that points at
  `<prelude>`. A user who wants their own definitions must use `--no-prelude`, and then must declare everything
  they use.
- Tools that use the library get the prelude only by adding `prelude_source()` themselves, so the library API
  stays free of hidden inputs.
- Because the prelude is a source like any other, it takes part in the single shared namespace of
  [decision 1](../0001-all-inputs-to-check-share-one-namespace/0001-all-inputs-to-check-share-one-namespace.md),
  and any future prelude addition is a potential clash with existing user schemas.
- A user cannot shadow a prelude name without disabling the whole prelude. This is accepted for now.
