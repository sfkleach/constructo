# Task: Implement the constructo tool with the `check` subcommand

## Overview

The `constructo` CLI tool will implement several subcommands, including `check`,
`query`, `convert` and `complete`. This task focusses on the initial
implementation using just a single subcommand.

Our implementation language will be Rust and our implementation will support 
both `constructo` as a CLI tool and also as a library for processing Constructo
files.

We will proceed step-by-step - mainly so I can catch up and review the changes.

## Step 1: Skeleton Implementation

Provide a skeleton implementation that simply processes the command line
and prints the options back to the terminal. The syntax for the check 
subcommand is:

```
constructo check [--stdin] FILES...
```

Note that the `--stdin` option is intended to process the standard input as 
if it was another file. However at this stage, it is just echoed back to the
terminal.

## Step 2: Implement `check` for data-type declarations

Implement the `check` subcommand for data-type declarations. It should check
the syntax of data-type declarations and then run the post-parse checks below.

### Inputs and scope

- Every input (each file, and `--stdin`) is treated as a types file. There is no
  sniffing of extensions or contents yet; a values file simply fails to parse.
- All inputs together form a single namespace. A type declared in one file may be
  used in another, and duplicates between files are errors.
- Declaration order does not matter: forward references and mutual recursion are
  allowed.

### Post-parse checks

- No duplicate top-level declarations (type names).
- No duplicate constructor names, within a declaration or between declarations.
- No duplicate field names in a record, including anonymous records nested inside
  a type.
- Type names and constructor names live in separate namespaces, so
  `enum Config = Config { ... }` is legal. Neither may be a basic type name.
- Every type reference must resolve to a basic type, a declared enum or an
  in-scope type parameter. This one rule replaces the earlier separate "leaf types"
  and "type-names are known" rules.
- Generic arity must match: basic types and type parameters take no arguments, and
  an enum takes exactly as many as it declares (a bare `List` for `List<T>` is an
  error).
- Type parameters must be distinct, and must not clash with a basic type or a
  declared enum. (To be revisited if explicit type parameters are added.)
- A type parameter that is never used is an error, unless its name starts with an
  underscore.

The basic types are: Nat, Int, Float and String.

### Grammar change

Type parameters may start with an underscore: `TypeParam = '_'? TypeName`.
Update `syntax/dt_grammar_tabatkins.txt`. An underscore parameter is declared
but may not be used in the body (it exists only to satisfy arity).

### Diagnostics

- The parser reports the first syntax error in each file, with no error recovery. (Step 4 later changed this:
  the run stops at the first syntax error.)
  Post-parse checks report every error found, ordered by file, line and column. They are
  skipped if any file had a syntax error, since the unparsed file may hold declarations
  the others use.
- Format: `ERROR: <message> (<file>, line L, col C)`, with `<stdin>` as the file
  name for standard input. A duplicate also names the first site, e.g.
  `first declared at line L, col C`. Update `docs/vision.md` to match.
- Diagnostics go to stderr. Exit code 0 means no errors, 1 means errors were found
  in the inputs, and 2 means a usage or I/O problem (e.g. an unreadable file).

### Implementation

- A hand-written lexer and recursive-descent parser with no new dependencies.
- Library modules for the lexer, AST, parser, checker and diagnostics; `main.rs`
  stays thin.
- Unit tests in the library, plus `tests/functest.sh` modelled on
  `../fuselage/tests/functest.sh`, driven by the directories in
  `syntax/examples/types/`: `valid/` expects exit 0; `invalid/` and
  `parses-but-invalid/` expect exit 1. `mutual.types` (which uses `Option`)
  is on an expected-failure list until Step 3 provides the prelude.
- New examples in `syntax/examples/types/`:
  - `parses-but-invalid/`: duplicate enum declaration; a constructor clashing
    across two enums; a type called `Nat`; a constructor called `Int`; duplicate
    type parameters; a type parameter clashing with a type; an unused type
    parameter; an undeclared type variable.
  - `valid/`: a type and a constructor sharing a name; an unused `_T` parameter.
- Record the design decisions (namespaces, multi-file scope, diagnostics and exit
  codes, underscore parameters) in `docs/decisions/`.

## Step 3: Implement the standard prelude

The standard prelude is automatically loaded ahead of any other input (unless disabled). It includes:

```
enum Bool = True, False;
enum Option<T> = None, Some(T);
```

Loading can be disabled with `--no-prelude`. 

Implement the prelude loading as an initial action. Integrate the examples
that were failing (such as `mutual.types`) into the functional tests.

## Step 4: Verbose mode

The `--verbose` option was implemented in the previous steps and, as expected,
generates additional commentary. This step refines its behaviour. When it is
enabled:

- Each file that is successfully checked should be reported clearly, along
  with the file name.
- If all files are successfully parsed, then a final `OK` message is printed
  along with a count of the files checked.
- If a data-types file is not successfully parsed, then the checking is stopped
  and the error, the associated enum (if known), and the location in the file
  is reported.
- If the "file" is stdin, then `<stdin>` is used to represent its name.

N.B This option will be elaborated further when data-value files are added.

## Step 5: Implement the well-formedness `check` for data-values

We will implement the check for data-values in two steps. In this step we simply
verify that any data-values file is well-formed; in the next step we verify that
it is well-typed.

- Classify each file as data-types or data-values
- If we find a data-values file, check if they are well-formed
- If not, raise an appropriate error message along with the filename and location
- If so, in verbose mode, list the name of the file.

### Refinements agreed during the step

- A file is classified by its first token: a types file always starts with
  `enum` and a values file never does. A file with no tokens is treated as a
  types file. The rule must not depend on the file name, because files holding
  both types and values are planned.
- Values in a file are separated by semi-colons, with an optional trailing one.
  Which constructors take a record or a tuple depends on the types in force, so
  juxtaposition would be ambiguous. See decision 7.
- Verbose mode shows the kind of each file, e.g. `OK: prod.values (data-values)`.
- Values are checked independently of any types in this step, and no types file
  is needed.
- A string accepts the escapes of the grammar, including `\uXXXX` and surrogate
  pairs. A raw control character or a newline in a string is an error.
- The parser rejects input nested deeper than 200 levels, rather than
  overflowing the stack.

## Step 6: Implement the type-check for data-values

When `constructo` encounters a data-value expression during a `check`, it should
verify that it is well-typed. Only the data-types encountered prior to the
expression are in scope - although the scope is mutually recursive. This 
check can be disabled with the option `--no-type-check` and the only checking
that is applied is the well-formed check in Step 5.

A failed type-check will normally cause an error and stop the processing of
subsequent values. However you can continue past these to get a more exhaustive
list with `--continue`.

The error will focus on the constructor of the sub-term which doesn't type-check.

### Refinements agreed during the step

- Scope: the types in force for a values file are the prelude plus the types files given *earlier* on the command
  line; later ones are not visible. (Mixed files are planned; "prior" will then mean "declared above".) Within that
  set the scope is mutually recursive. See decisions 9 to 11.
- A tagged value is checked against the enum that declares its constructor. Constructor names are unique across
  enums (Step 2), so no expected type is needed.
- Untagged top-level values (`3.0`, `[1, 2]`, `{x: 1}`, `"hi"`) have no declared type. They are checked for internal
  consistency only: array elements must unify, and nested tagged terms are checked as above.
- Shapes must match the declaration exactly: `Quit` only for a bare variant, `Ping()` only for a tuple variant and
  `Pong {}` only for a record variant. A tuple must have the declared arity.
- Records: the set of fields must match exactly (missing and unknown fields are errors), in any order. A duplicate
  field in a value is an error. This applies to anonymous records nested inside a type too.
- Generics: type parameters are instantiated with fresh variables and unified across the whole top-level value, so
  `Cons { head: 1, tail: Cons { head: "a", tail: Nil } }` is an error. `[T]` arrays unify their elements.
- Numbers: `5` is lexically a Nat, `+5` and `-5` an Int and `5.0` a Float. With widening (the default), a Nat fits
  Nat, Int and Float; an Int fits Int and Float; a Float fits Float only. A negative number never fits Nat.
  `--no-widening` requires the exact lexical form. The option has no effect with `--no-type-check`.
- Join: where unification meets two numeric types the result is the wider one, so `[1, 2.5]` is `[Float]` and
  `[1, +2]` is `[Int]`. With `--no-widening` there is no join and these are errors.
- Without the prelude and without a user-defined `Option`, a `null` is an error that names the cause. This is a stopgap
  until `const` declarations arrive and the prelude defines `null` (decision 10).
- `null` is handled specially as `None` of `Option<T>`. There is no general synonym mechanism yet.
- Errors: one error is reported per top-level value, located at the constructor (or literal) of the innermost
  failing term. The message says what was expected and what was found.
- `--continue` only spans values and files: after a type error, checking resumes with the next top-level value. A
  syntax error still stops the run (Step 4), since there is no error recovery.
- A file with type errors makes the run exit 1, like any other error.
- Verbose mode shows `OK: prod.values (data-values, type-checked)`, or `(data-values)` under `--no-type-check`.
- New examples go in `syntax/examples/values/`, with `valid/` and `invalid/` as in Step 5. Each needs the types
  it relies on, so the functional tests must pass the types files first.

## Step 99: Populate the CHANGELOG

Before finishing the task, populate `CHANGELOG.md` in the
[Keep a Changelog](https://keepachangelog.com/) format, recording the changes
made in each step under an "Unreleased" section.
