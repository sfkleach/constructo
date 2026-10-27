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
- The standard prelude (`enum Bool = False, True;` and `enum Option<T> = None, Some(T);`, and the consts below) is
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
- The parser rejects input nested more than 200 levels deep instead of overflowing the stack. The limit is on the
  height of the tree, which includes the levels that a postfix `?` adds, as well as on the depth of the calls.
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
- Named parameters and arguments for tuple constructors. See decisions 17 and 18.
  - A parameter may be given a name, as in `enum Image = Image(String, String, hOffset: Nat, alignment: String);`.
    Unnamed parameters come first and can only be bound by position. Named parameters can be bound by position or
    by name, and names are per constructor.
  - An argument list has its positional arguments first, followed by named arguments in any order, as in
    `Image("a.jpg", "A", alignment: "left", hOffset: 100)`. A parameter can be bound only once, and an unknown name
    is an error. Names affect only the matching of arguments to parameters, not the types. A mismatch for a named
    parameter says which argument it is for, as in `Expected Nat for argument hOffset but found String`.
  - A named argument may be left out, and then takes the default value of its type: `0` for `Nat` and `Int`, `0.0`
    for `Float`, `""` for `String`, `[]` for an array, the defaults of the fields for a record, and the first
    constructor of an enum with the defaults of its fields. Unnamed parameters must still be supplied. It is an error
    if the default is recursive (`Type List<T> has no default value`), if its type is not known, or if it has more
    nodes than `--default-limit=N` allows (10,000 by default; the library option is `CheckOptions::default_limit`).
  - The grammar files, the diagrams and `syntax/grammar.md` describe `NamedParameter` and `NamedArgument`, and
    show that named ones come after unnamed ones.
- A postfix `?` on a type is shorthand for `Option` (decision 19): `Nat?` is `Option<Nat>`, `[Nat]?` is
  `Option<[Nat]>` and `Nat??` is `Option<Option<Nat>>`. It applies to the whole type before it, it is only sugar so it
  needs an `Option` to be declared (the prelude has one), and messages show `Option<Nat>`. An `Option` parameter that
  is named may be left out, as it defaults to `None`.
- An experimental postfix `?` on a value is shorthand for `Some`: `0?` is `Some(0)` (decision 20). It is a syntax
  error unless it is switched on, with `constructo check -x` for every experimental feature or `-x=some-query`
  (`--experimental`) for this one. The option's value must be given with `=`, it can be repeated, and an unknown name
  is a usage error. The library has `CheckOptions::experimental`, and `parse_file` and `syntax_error` take an
  `Experimental` argument. The grammar files, the diagrams and `syntax/grammar.md` show both.
- A configuration file, named with `constructo check --config=PATH` (decision 21). It is in TOML, with the tables
  `[global]`, `[experimental]` and `[check]`, and a key is the long form of the option without the `--`, such as
  `allow-refs` or `infer-limit`. An option on the command line overrides the file, which overrides the built-in
  default. There is no search for a file, and `--config` can be given once, before or after the subcommand.
  - Every problem is an error with exit code 2, in the usual form `ERROR: <message> (<file>, line L, col C)`: an
    unknown key or table (the message lists the known ones), a value of the wrong type or out of range, and an error
    in the TOML itself. A file that cannot be read is also exit code 2.
  - The library has the `config` module, with `Config::parse` and `Config::check_options`, so a program that embeds it
    can read the same file. The format is described in `docs/specs/config-file.md`.
- `constructo check --print-config` prints the settings that the run would use, after the defaults, the file and the
  options have been combined, as a TOML file with a comment on each key, and exits without checking anything. It is
  a template for a configuration file and `--config` reads it back.
- Every boolean option has a negative form, and the last one given wins: `--verbose` and `--no-verbose`, `--prelude`
  and `--no-prelude`, `--type-check` and `--no-type-check`, `--widening` and `--no-widening`, `--continue` and
  `--no-continue`, and `--allow-refs` and `--no-allow-refs`. So an option can undo a setting from the configuration
  file. `--no-experimental`, with an optional `=FEATURE`, switches experimental features off, and it and `-x` apply
  in order, feature by feature.
- The dependencies `serde` and `toml`, for reading the configuration file.
- Array variants (decision 22): a variant may hold an array of one type, as in `enum Users = Users[User], NoUsers;`,
  with values written `Users[u1, u2]` or `Users[]`. It is a different form from `Users([User])`, and a value of one is
  not accepted for the other (`Constructor Users takes an array`). The brackets of a declaration hold exactly one
  type, so `Users[A, B]` and `Users[]` are syntax errors, and the type parameters of a generic enum, as in
  `enum JoinList<T> = Items[T], Join[JoinList<T>];`, are unified across the elements. The default of an array is the
  empty array. The grammar files, the diagrams and `syntax/grammar.md` show `ArrayBody` and the array form of `Tagged`.

### Changed

- The prelude declares `enum Bool = False, True;` and not `True, False`, so that the default of a `Bool` parameter
  that is left out is `false`.
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
