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
inferred.

## Step 5: Update the grammar files

To be done



## Step 99: Update the CHANGELOG

To be done