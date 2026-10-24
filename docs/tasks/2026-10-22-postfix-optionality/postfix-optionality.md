# Task: Implement the Postfix `?` constructor and type constructor

## Overview

Because `Option<T>` is such a ubiquitous type, we want a convenient shorthand
for `Option<T>`, namely `T?`. And since the constructor `Some(t)` gets used a 
lot, we want to be able to write (say) `0?`, equivalent to `Some(0)`.

## Step 1: `?` as postfix `Option` generic type-constructor

Implement a postfix operator `?` in the data-types grammar, which is a 
syntactic shorthand for the `Option<...>` type constructor.

# Step 2: `?` as postfix `Some` constructor

Implement a postfix operator `?` in the data-values grammar, which is a 
syntactic shorthand for the `Some(...)` constructor.

This feature is only available with the `-x,--experimental` option. This
option either takes no value OR enables all experimental features. This 
feature has the name `SomeQuery`.

### Considerations for the `--experimental` option

- The option follows the usual pattern for an option with an optional value: the value must be supplied with `=`.
  So `-x` or `--experimental` on its own enables all the experimental features, and `-x=SomeQuery` or
  `--experimental=SomeQuery` enables just that one. Because of this, `constructo check -x file.values` is
  unambiguous: `file.values` is a file and not a feature name. If this proves awkward to implement, the fallback is
  to accept `-x` with no value only, meaning all the features.
- The option may be repeated, as in `-x=A -x=B`, which enables both. A bare `-x` enables everything.
- An unknown feature name is a usage error with exit code 2, and the message lists the known names, so a typo cannot
  silently leave a feature switched off.
- The library has the same switch. `CheckOptions` gets a set of enabled features, which a bare `-x` fills with all
  of them. The parser reports `0?` as a syntax error when `SomeQuery` is not in the set, with a message such as
  "Postfix '?' is experimental; enable it with --experimental=SomeQuery".
- The help text of the option lists each experimental feature by name, so that `--help` shows what a bare `-x` would
  turn on.
- Only Step 2 is gated. The postfix `?` for types in Step 1 is not experimental.

