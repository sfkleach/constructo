# Tasks: const definitions

## Overview

We are adding a new kind of definition that goes in the data-types files. These
can be freely interleaved with `enum` definitions. The general syntax is:

```
const IDENTIFIER [: TYPE_EXPRESSION] = VALUE_EXPRESSION;
```

Note that the semi-colon is a terminator for these definitions, the same as 
enums.

In order for these new definitions to work, it is very helpful to be able to
give types to untagged objects such as `{ left: "c", right: "r" }`. In this
case the type would be `{left: String, right: String}`.

In general, the type of an untagged object is denoted by substituting type
expression for where the value expressions would go.

## Step 1: Add `const` to Constructo data-type files

Extend the `check` command to recognise `const` definitions. The type expression
is optional. 

- Value-expressions are extended to support references to const identifiers
- `const` definitions may not reference `const` identifiers unless the 
  `--allow-refs` option is set
- `const` definitions are evaluated on demand i.e. order does not matter
- `const` definitions may not have a circular dependency
- The type of a const identifier is the type given in its definition
- In this step the type must be present.

Note that Constructo data-type files can now begin with either `enum` or `const`.
This affects the detection of the type of file. 

### Refinements agreed during the step

- Without `--allow-refs`, a reference to a const inside a `const` definition is
  an error at the reference, and the message names the flag. The order and cycle
  rules only apply when the flag is set. The flag is a `check` option and only
  governs const-to-const references.
- Const references may appear in values files and, with `--allow-refs`, in
  `const` bodies.
- `const` is a reserved keyword everywhere, so classifying a file is still a
  first-token test: `enum` or `const` means a types file.
- Consts share the constructor namespace and may be mixed-case. This follows
  Rust's split between the type namespace and the value namespace. These are
  duplicate or clashing-name errors, with the usual "first declared at" note:
  const vs constructor, const vs const (within and across files), and const vs
  basic type name (as for constructors). A const may share a name with an enum
  type, just as a constructor may, so `enum Config = Settings { port: Nat };
  const Config: Config = Settings { port: 80 };` is valid.
- In this step the type expression uses only the existing forms: basic types and
  enums with their generic arguments. Type parameters are not in scope.
  Record-valued consts need a tagged constructor until Step 2.
- A const's value is checked against its declared type with the same
  type-checker as values files, and a reference to a const has the type declared
  in its definition.


## Step 2: Extend the standard prelude with constants true, false and null

The standard prelude should now include:
```
const null<T>: Option<T> = None;
const true: Bool = True;
const false: Bool = False;
```

Note that `null` needs a type parameter, since a bare `None` can be an `Option` of anything.

### Generic consts

A `const` may declare type parameters, with the same syntax as an `enum`:

```
const NAME<T, ...>: TYPE_EXPRESSION = VALUE_EXPRESSION;
```

This follows Rust's (unstable) `generic_const_items`. Rust does not allow the placeholder `_` in the type of a const,
and neither does Constructo.

- The type parameters are in scope in the type expression only. They must be
  distinct, must not clash with a basic type or an enum, and must be used in the
  type expression unless their name starts with an underscore.
- Each reference to a generic const instantiates its parameters with fresh type
  variables, so that the uses of `null` in one value may have different types.
  The type arguments cannot be written at a use, only inferred.
- The value is checked against the declared type with the parameters taken as
  unknown types. So `None` is accepted as an `Option<T>` and `Some(1)` is not.
- A generic const has no special status otherwise: it shares the constructor
  namespace, and its references are type-checked as in Step 1.

### Replacing the `null` literal

- `null`, `true` and `false` are ordinary names that refer to the prelude
  consts, and the special treatment of `null` in the parser and the type-checker
  is removed (see decision 10). Under `--no-prelude` they are unknown consts.
- The parser no longer rejects `null` as the name of a const. Redefining `null`,
  `true` or `false` is a duplicate of the prelude's const.
- A reference to a const that is defined in the prelude is always allowed in a
  `const` definition. Other references still need `--allow-refs`.

## Step 3: Add untagged object type expressions

We have a gap in the type expression language - we cannot denote the type of
this const `x`:
```
const x : TYPE = {left: "hand", right: "heart"};
```
In this step we close that gap by extending the type language to include type
expressions for untagged objects. e.g.
```
const x : {left: String, right: String} = {left: "hand", right: "heart"};
```

Note that `{}` is both a type expression and a value expression. 

### Refinements agreed during the step

- The gap was already closed when the type language gained anonymous records in
  the original `check` task: a record type can be written wherever a type can,
  so the example above parses and type-checks as it stands. This step adds no
  language feature. It pins the behaviour down with examples, unit tests and a
  decision record, and it records that the grammar needs no change.
- A record type is closed. A value must have exactly the declared fields, in any
  order, so `{a: Nat}` rejects `{a: 1, b: 2}` and `{a: Nat, b: Nat}` is the same
  type as `{b: Nat, a: Nat}`. This is the rule for the record variants of enums.
- A tagged record never converts to an untagged record type or the other way
  round. `Foo { a: 1 }` has the type of the enum that declares `Foo`, so `const
  x: {a: Nat} = Foo { a: 1 };` is an error.
- `{}` as a type is the record with no fields and its only value is `{}`. It is
  distinct from a bare constructor and from `Pong {}`, which is a tagged record.


## Step 4: Make the type-expression part of a `const` definition optional

In this step we allow the type expression to be skipped and the type is 
inferred. Note that widening will not happen here so there's a difference 
between `const x = 0;` and `const x: Int = 0`.

### Refinements agreed during the step

- The inferred type of a const is the type of its value as the type-checker
  works it out for a top-level value: `0` is a Nat, `+0` is an Int, `0.0` is a
  Float, a tagged value has the type of its enum, and an untagged record has the
  record type of its fields. Numbers in an array join as in values, so `[1,
  2.5]` is `[Float]`; under `--no-widening` there is no join. Only the type of
  the const itself is exact: a reference to `const x = 0;` is accepted where an
  Int is expected, as it is for `const x: Nat = 0;`.
- Type variables that the value leaves undetermined are generalised. `const e =
  [];` has the type `[T]` for any `T`, as if it were written `const e<T>: [T] =
  [];`, and `const none = None;` and `const m = null;` have type `Option<T>`.
  That is sound because values are immutable. Each reference instantiates the
  variables afresh, as in Step 2.
- A const that omits its type may not declare type parameters, because they are
  scoped to the type expression. It is an error, whose message says that type
  parameters need a type.
- A const with an inferred type may refer to any other const, annotated or not,
  under the same rules as in Step 1. The types are worked out in dependency
  order, so that the declaration order does not matter, and a circular
  definition is still an error under `--allow-refs`.
- Errors in the value of a const without a type are reported where the term is,
  as for a values file, e.g. `const x = [1, "a"];` fails at `"a"`.
- In verbose mode, each const of a file is listed with its type, e.g. `const x:
  Nat`, and the types that were inferred are marked as such. Nothing extra is
  printed unless `-v` is given.


## Step 5: Update the grammar files

Modify the syntax/dt_grammar_tabatkins.txt and syntax/dv_grammar_tabatkins.txt
grammars to be consistent with the new syntax.

### Refinements agreed during the step

- The data-types grammar gains `ConstDecl`, and a types file is a sequence of
  `EnumDecl` or `ConstDecl`. A file whose first token is `enum` or `const` is a
  types file.
- `ConstDecl` is drawn as `const Name [ [<TypeParams>] : Type ] = Value ;`. The
  parameter list is the quantifier of the type of the const and goes together
  with the annotation, so the diagram cannot show `<T>` without a `: Type`. The
  parser reads that form and validation rejects it, so the diagram is slightly
  stricter than the parser. The prose says so.
- `Value` in a `ConstDecl` is a cross-reference to the data-values grammar and
  is not redrawn. The prose of the data-types section says where it is defined,
  and `grammar.template.md` explains why it is undefined in that list.
- `Name` for a const is an identifier of either case, other than the reserved
  words `enum` and `const`.
- The data-values grammar loses the `null` literal and gains `ConstRef`: a name
  that starts with a lowercase letter or underscore, with no body. An uppercase
  name is a `Tagged` value, which may have no body, and the checker decides
  whether it is a constructor or a const. `Tagged` keeps its optional record or
  tuple body. The prose says that a bare uppercase name is a constructor or a
  const, that a lowercase name is always a const, and that `null`, `true` and
  `false` are consts of the prelude.
- The derived files are regenerated with `just grammar-md`: the PDFs and SVGs,
  which go through git-lfs, and `grammar.md`. The template's introduction and
  its description of a types file mention consts.


## Step 6: Update the CHANGELOG

Bring the CHANGELOG up to date.

### Refinements agreed during the step

- The entries were written as each step was done, so they are tidied and
  recategorised. The entries for the consts are one group under Added, with
  sub-bullets for the references, the type parameters, the inference and the
  record types, and with references to decisions 12 to 16.
- The effects on what already existed go under Changed: the classification of a
  file by `enum` or `const`, `null` as a const of the prelude and the new
  message for it, a lowercase name being a value, and `const` being reserved.
- The entry for the type-check no longer describes `null` as special.
