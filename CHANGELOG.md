# Change Log for Constructo

Following the style in https://keepachangelog.com/en/1.0.0/

## Unreleased

### Added

- The `constructo` command-line tool, with the subcommand `constructo check [--stdin] FILES...`. It is a thin layer
  over a library crate, whose entry points are `check_sources` and `check_sources_with`.
- `constructo check` checks data-type declarations. It parses types files with a hand-written lexer and parser, and
  then checks for duplicate types, constructors, fields and type parameters, names that clash with the basic types
  (`Nat`, `Int`, `Float`, `String`), unknown type names, generic arity, and unused type parameters. All the inputs
  share one namespace, and the order of declarations does not matter.
- Type parameters may start with an underscore, e.g. `<_T>`, to mark them as intentionally unused.
- Diagnostics have the form `ERROR: <message> (<file>, line L, col C)`, with `<stdin>` for standard input. A duplicate
  also says where the name was first declared. The exit code is 0 for success, 1 if the inputs contain errors and 2
  for usage or I/O problems.
- The standard prelude (`enum Bool = True, False;` and `enum Option<T> = None, Some(T);`) is loaded by default by
  `constructo check`; `--no-prelude` disables it. The library exposes it as `PRELUDE` and `prelude_source()`.
- `constructo check` is silent on success. `--verbose` (`-v`) reports each input as `OK: <name>` or
  `FAILED: <name>` (`<stdin>` for standard input, never the prelude) and ends with `OK: checked N file(s)` when
  everything passed.
- A syntax error stops the run, and names the enclosing enum when it is known, e.g.
  `ERROR: Expected comma or semi-colon (f.types, line 3, col 1); in enum Shape`.
- `constructo check` classifies each input by its first token as data-types (`enum`, or no tokens) or data-values, and
  checks that a data-values file is well-formed, reporting the file name and location of any syntax error. In verbose
  mode each input is reported with its kind, e.g. `OK: prod.values (data-values)`.
- The parser rejects input nested more than 200 levels deep instead of overflowing the stack.
- `constructo check` type-checks data-values against the data-types that come before them on the command line (and
  the prelude). Constructors must exist and have the declared shape, records must have exactly the declared fields,
  and the type parameters of generic enums are unified across a top-level value. Numbers widen from Nat to Int to
  Float, and `[1, 2.5]` has type `[Float]`. Untagged top-level values are checked for internal consistency, and `null`
  is `None` of `Option<T>`. The first type error stops the run. See decisions 9 to 11.
- The options `--no-type-check` (only check that values are well-formed), `--no-widening` (numbers must have exactly
  the expected type) and `--continue` (carry on with the next value after a type error).
- In verbose mode a type-checked values file is reported as `OK: prod.values (data-values, type-checked)`.
- Library modules for the lexer, syntax tree, parser, validation and type-check, with `CheckOptions` to choose the
  options and `Report` to describe the outcome for each source.
- `tests/functest.sh`, driven by the examples in `syntax/examples/`.
- `const name: Type = value;` definitions in types files, which may be interleaved with enums and come in any order.
  A file whose first token is `enum` or `const` is a types file, and `const` is a reserved word. Consts share the
  constructor namespace, may have a name of either case, and cannot be named after a basic type. The value of a
  const is checked against its declared type, and a values file can refer to a const by name. The type is required
  for now.
- `constructo check --allow-refs` lets the definition of a const refer to other consts. Without it such a reference is
  an error, and with it a circular definition is.
- Consts may have type parameters, as in `const none<T>: Option<T> = None;`. Each use of one gets its own type
  arguments, and its value is checked against the parameters as unknown types.
- The standard prelude declares the consts `null`, `true` and `false`, which replace the `null` literal. A const
  definition may refer to them without `--allow-refs`. Under `--no-prelude` they are unknown.

### Changed

- Data values are separated by semi-colons, with an optional trailing one. This resolves the ambiguity of a
  constructor followed by a record or tuple. The values examples and grammar are updated.
- `docs/vision.md` diagnostics now include the file name.
