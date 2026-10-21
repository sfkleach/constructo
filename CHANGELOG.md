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
- The standard prelude (`enum Bool = True, False;` and `enum Option<T> = None, Some(T);`, and the consts below) is
  loaded by default by `constructo check`; `--no-prelude` disables it. The library exposes it as `PRELUDE` and
  `prelude_source()`.
- `constructo check` is silent on success. `--verbose` (`-v`) reports each input as `OK: <name>` or
  `FAILED: <name>` (`<stdin>` for standard input, never the prelude) and ends with `OK: checked N file(s)` when
  everything passed.
- A syntax error stops the run, and names the enclosing enum when it is known, e.g.
  `ERROR: Expected comma or semi-colon (f.types, line 3, col 1); in enum Shape`.
- `constructo check` classifies each input by its first token as data-types (`enum` or `const`, or no tokens) or
  data-values, and checks that a data-values file is well-formed, reporting the file name and location of any syntax
  error. In verbose mode each input is reported with its kind, e.g. `OK: prod.values (data-values)`.
- The parser rejects input nested more than 200 levels deep instead of overflowing the stack.
- `constructo check` type-checks data-values against the data-types that come before them on the command line (and
  the prelude). Constructors must exist and have the declared shape, records must have exactly the declared fields,
  and the type parameters of generic enums are unified across a top-level value. Numbers widen from Nat to Int to
  Float, and `[1, 2.5]` has type `[Float]`. Untagged top-level values are checked for internal consistency. The first
  type error stops the run. See decisions 9 to 11.
- The options `--no-type-check` (only check that values are well-formed), `--no-widening` (numbers must have exactly
  the expected type) and `--continue` (carry on with the next value after a type error).
- In verbose mode a type-checked values file is reported as `OK: prod.values (data-values, type-checked)`.
- Library modules for the lexer, syntax tree, parser, validation and type-check, with `CheckOptions` to choose the
  options and `Report` to describe the outcome for each source.
- `tests/functest.sh`, driven by the examples in `syntax/examples/`.
- Const definitions in types files, `const name<Params>: Type = value;`, which may be interleaved with enums and come
  in any order. See decisions 12 to 16.
  - A const gives a name to a value. Its value is checked against its declared type with the checker used for values
    files, and a values file can refer to a const by name, and so can the definition of another const.
  - A const shares the constructor namespace, may have a name of either case, and cannot be named after a basic type
    or be a duplicate of a constructor or another const. It may share its name with an enum type.
  - A const may refer to another only with `constructo check --allow-refs`. Without the option such a reference is an
    error, and with it a circular definition is. References to the consts of the prelude are always allowed.
  - A const may have type parameters, as in `const none<T>: Option<T> = None;`. Each use gets its own type arguments,
    and the value is checked with the parameters as unknown types, so `Some(1)` is not an `Option<T>`.
  - The type of a const may be left out and is then inferred from its value: `const count = 10;` is a Nat and
    `const nothing = [];` is `[T]` for any `T`. Only the type of the const is exact, and a reference still widens. A
    const without a type may not have type parameters. A type of more than 10,000 nodes is refused, and
    `--infer-limit=N` changes that limit.
  - The type of an untagged object is written like the object, with a type in place of each value, e.g.
    `const x: { left: String, right: String } = { left: "hand", right: "heart" };`. A record type is closed, its
    fields may be in any order, and it never converts to or from a tagged record.
  - In verbose mode the consts of a file are listed with their types, marking the inferred ones, e.g.
    `const count: Nat (inferred)`.
  - The grammar files, the diagrams and `syntax/grammar.md` describe consts, and `ConstRef` in the data-values grammar.
- The standard prelude declares the consts `null`, `true` and `false`, as `const null<T>: Option<T> = None;`,
  `const true: Bool = True;` and `const false: Bool = False;`. Under `--no-prelude` they are unknown.

### Changed

- Data values are separated by semi-colons, with an optional trailing one. This resolves the ambiguity of a
  constructor followed by a record or tuple. The values examples and grammar are updated.
- `docs/vision.md` diagnostics now include the file name.
- A file is a types file if its first token is `enum` or `const`, or it has no tokens, and a values file otherwise.
- `null` is no longer a literal that the type-checker treats specially. It is the const `null` of the prelude, whose
  type is `Option<T>` for any `T`, so it is unknown under `--no-prelude`. The error "Cannot use null because the type
  Option is not in scope" is replaced by "Unknown const null", and a type error now describes it as, for example,
  `found const null of type Option<_>`.
- A lowercase name in a values file is a value, a reference to a const. It used to be a syntax error.
- `const` is a reserved word, so it can no longer be used as a field name.
