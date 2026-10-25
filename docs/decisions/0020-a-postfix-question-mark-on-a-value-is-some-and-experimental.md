# 0020 - A postfix `?` on a value is shorthand for `Some` and is experimental, 2026-10-25

## Issue

**Fork.** `Some(0)` is common in values that have `Option` types, and `0?` is a short way to write it, to go with the
`T?` of decision 19. One aim of the format is that a value read without its types has the same structure as the value
read with them, with elements possibly missing. A shorthand that stands for a constructor fits that aim, whereas an
implicit conversion from `0` to `Some(0)` would not, as the structure of the value would then depend on the types. It
is not yet clear that this is the right syntax, so it should be possible to try without committing to it.

## Decision

- A postfix `?` after a value wraps it in `Some`. `0?` is exactly `Some(0)`. Like the `?` for types it applies to the
  whole value before it (a literal, an array, a record, a constructor with its body, or a const name), can be
  repeated, can follow white space, and has the position of the start of its operand.
- It is experimental. Without the feature `some-query` it is a syntax error, with the message
  `Postfix '?' is experimental; enable it with --experimental=some-query`.
  - The gate is on the syntax and not on the kind of file, so it covers the value of a const in a types file as well.
  - The parser enforces it, and so only the first use is reported.
- `-x` or `--experimental` switches on features:
  - On its own it switches on all of them. A value must be given with `=`, as in `-x=some-query`, so
    `constructo check -x file.values` is unambiguous. `all` is also accepted as a value.
  - It can be repeated, and an unknown name is a usage error with exit code 2, which lists the names that are known.
  - Features are named in kebab-case, like the other options. The option is accepted by every subcommand that parses
    values, which today is `check`.
- The library has `Experimental`, with one flag for each feature and `Experimental::all()`, in
  `CheckOptions::experimental`. It is a small `Copy` struct so that `CheckOptions` stays `Copy`. `parse_file` and
  `syntax_error` take it as an argument.
- A feature graduates by removing its gate in the parser, and its name from the option once the feature is no longer
  experimental.
- An argument written with `?` is not given the "for argument x" form of a mismatch message, since its `Some` and its
  operand have the same position and the parameter's type is not that of the operand.

## Consequences

- A values file that uses `?` is not portable to a tool that has not switched the feature on, which is the point.
- Adding an experimental feature means one flag, one name and one check in the parser.
- A parameter such as `retries: Nat?` can be given as `retries: 3?`, and left out, or given as `null`.

## Options and Outcome

The chosen option is the one under Decision. Alternatives considered:

- No shorthand, which is safe but leaves `Some(` and `)` around every value of an optional member.
- An implicit conversion from `T` to `Option<T>` when the types are known: it makes the meaning of a value depend on
  the types, so the value could not be read without them.
- A gate that is checked after parsing, which reports every use: it would keep the parser free of options, but needs
  another pass and a mark in the syntax tree.
- A set of feature names in the options: it scales, but `CheckOptions` would no longer be `Copy`.
- Accepting `-x` with no value only, as a fallback: not needed, as clap supports a value that must follow `=`.
