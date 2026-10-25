# Task: Implement the Postfix `?` constructor and type constructor

## Overview

Because `Option<T>` is such a ubiquitous type, we want a convenient shorthand
for `Option<T>`, namely `T?`. And since the constructor `Some(t)` gets used a 
lot, we want to be able to write (say) `0?`, equivalent to `Some(0)`.

## Design points that apply to both steps

- **Pure sugar.** `T?` is exactly `Option<T>` and `v?` is exactly `Some(v)`. The parser rewrites them, and nothing
  after parsing can tell how they were written. In particular, error messages always show `Option<Nat>`, never
  `Nat?`.
- **No prelude, no `?`.** Because it is sugar for the names `Option` and `Some`, it behaves as if those names had been
  written. Under `--no-prelude` the ordinary unknown-type or unknown-constructor error is reported. The rewritten
  node has the position of the start of its operand, so `0?` is positioned like the `0`, and a mismatch reads
  "Expected Nat but found Some of type Option" at the same place as it does for `Some(0)` written out.
  An `Option` that the programmer declares for themselves works with `?` too.
- **Stacking is allowed.** `Nat??` is `Option<Option<Nat>>` and `0??` is `Some(Some(0))`. There are no special cases.
- **`?` is an ordinary token.** Whitespace and comments may separate it from what it follows, as with `,` and `>`.
  `Nat ?` is the same as `Nat?`.
- **It binds to the whole primary.** Anything that is a complete type or value can take the `?`: for types a name
  with its arguments (`Box<Nat>?`), an array (`[Nat]?`) or a record (`{ x: Nat }?`); for values a literal, an array,
  a record, a constructor with its body (`Point(1, 2)?`), or a const name. To make the elements optional instead of the
  whole, put the `?` inside: `[Nat?]`.
- **Interaction with omitted arguments.** `Option<T>` has the default `None`, so a named parameter such as
  `Job(name: String, retries: Nat?)` may be omitted at a use, with no further work in the checker. The task
  should add an example that shows this.

## Step 1: `?` as postfix `Option` generic type-constructor

Implement a postfix operator `?` in the data-types grammar, which is a 
syntactic shorthand for the `Option<...>` type constructor.

- The grammar gets a `?` token after `TypeRef`, `ArrayType` and `RecordBody` in `Type`, repeatable. The diagram
  `syntax/dt_grammar_tabatkins.txt` and `syntax/grammar.template.md` are updated, and `just grammar-md` is run.
- The feature is not experimental.
- Examples: valid types files that use `?` on each kind of type, stacked, and with whitespace before it, including a
  named parameter that is then omitted; invalid ones for a `?` with nothing before it (`enum E = A(?);`).
- Write a decision record for the type shorthand.

## Step 2: `?` as postfix `Some` constructor

Implement a postfix operator `?` in the data-values grammar, which is a 
syntactic shorthand for the `Some(...)` constructor.

This feature is only available with the `-x,--experimental` option. This
option either takes no value OR enables all experimental features. This 
feature has the name `some-query` on the command line and `SomeQuery` in the library.

- The gate is on the syntax and not on the kind of file, so `0?` is a syntax error without `some-query` wherever a
  value is parsed, including in the body of a const in a types file.
- The grammar diagram `syntax/dv_grammar_tabatkins.txt` shows the `?`, and the grammar documentation says that it
  is experimental.
- Examples: valid values files that use `?`, and an invalid one that the gate rejects. The functional tests run the
  valid ones with `-x` and `--experimental=some-query`, and check that without the option they fail with the message
  below.
- Write a decision record that covers both the value shorthand and the experimental-feature mechanism. It is kept
  apart from the one for types because it is expected to be revised, or to graduate by dropping the gate.
- Graduating a feature means removing its name from the set and the gate from the parser, and the option itself stays
  as long as any experimental feature exists.

### Considerations for the `--experimental` option

- The option follows the usual pattern for an option with an optional value: the value must be supplied with `=`.
  So `-x` or `--experimental` on its own enables all the experimental features, and `-x=some-query` or
  `--experimental=some-query` enables just that one. Because of this, `constructo check -x file.values` is
  unambiguous: `file.values` is a file and not a feature name. In clap this is `num_args = 0..=1`
  with `require_equals` and a `default_missing_value` of `all`, so no fallback should be needed.
- `all` is also accepted as a value, and it is what a bare `-x` stands for.
- Feature names are kebab-case, like the other options. The option may be repeated, as in `-x=a -x=b`, which
  enables both. A bare `-x` enables everything.
- An unknown feature name is a usage error with exit code 2, and the message lists the known names, so a typo cannot
  silently leave a feature switched off.
- Every subcommand that parses values accepts the option, which today is `check`, and which will be `lint` too.
  It is defined once and shared, so that a new subcommand picks it up.
- The library has the same switch. `CheckOptions` gets an `Experimental` struct with one `bool` per feature
  (`some_query`) and `Experimental::all()`, which a bare `-x` selects. This keeps `CheckOptions` `Copy` and
  `Default`. The parser is given the struct, so `parse_file` and `syntax_error` gain a parameter, and it reports `0?`
  as a syntax error when `some_query` is off, with a message such as "Postfix '?' is experimental; enable it with
  --experimental=some-query". Only the first use is reported, like any syntax error.
- The command line uses clap's value-enum support, so the error for an unknown name, which lists the valid ones and
  exits with 2, comes from clap and the names are defined once.
- The help text of the option lists each experimental feature by name, so that `--help` shows what a bare `-x` would
  turn on.
- Only Step 2 is gated. The postfix `?` for types in Step 1 is not experimental.

