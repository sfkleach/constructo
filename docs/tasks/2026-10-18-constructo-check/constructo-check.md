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
    Update `syntax/dt_grammar_tabatkins.txt`. An underscore parameter is declared but
    may not be used in the body (it exists only to satisfy arity).

### Diagnostics

- The parser reports the first syntax error in each file, with no error recovery.
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
enum Bool = True | False;
enum Option<T> = None | Some(T);
```

Loading can be disabled with `--no-prelude`. 

Implement the prelude loading as an initial action. Integrate the examples
that were failing (such as `mutual.types`) into the functional tests.


## Step 99: Populate the CHANGELOG

Before finishing the task, populate `CHANGELOG.md` in the
[Keep a Changelog](https://keepachangelog.com/) format, recording the changes
made in each step under an "Unreleased" section.
