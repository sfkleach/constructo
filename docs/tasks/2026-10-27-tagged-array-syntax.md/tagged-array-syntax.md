# Task: Tagged Array Syntax

## Overview

We will support enums with a variant that takes a single array type as their
argument with this syntax:

```
enum Users = Users[ User ], ...;
```

The value syntax mirrors this: `Users[User{...}, ...]`. It is roughly
equivalent to this:

```
enum Users = Users([ User ]), ...;
```

## Questions

1. One question this raises is whether the variant `Users([User])` is different
or the same as `Users[User]`. We have determined the answer is that they will be
treated as different forms.

2. Single type: Users[A, B] should be a syntax error, and so should Users[] with
no element type. The body holds exactly one element type.

3. Named parameters: a tuple variant can have named parameters (decision 17).
The array form has no parameter name hence it can't be omitted with a default by
name. The default of an array is an empty array.

4. Postfix ?: Users[u1, u2]? should be Option<Users>, since ? binds to the whole
primary. That falls out of the existing rule, but it needs a test.

5. Value parsing: Users[1] after a bare constructor is already safe, because
top-level values are separated by semi-colons (a previous decision). The grammar
still needs a Tagged alternative with an array body.


## The new form

Implement a new variant form of tagged arrays, very similar to the way
tagged records work.

```
enum JoinList<T> = Items[T], Join[JoinList<T>];
```

In the value notation the constructors `Items` and `Join` are tags that are
applied to an array and hold the types of their members.

```
const j: JoinList<Nat> = Join[Items[1, 2], Join[Items[3, 4], Items[5, 6]]];
```



## Step 1: Syntax

- Add `ArrayBody` as a fourth alternative of `Variant`, and an array alternative of `Tagged` in the values grammar.
  Update the grammar files, the diagrams and `syntax/grammar.md`.
- Lexer, parser and syntax tree: the body holds exactly one element type, so `Users[A, B]` and `Users[]` are syntax
  errors in a declaration. A value may have any number of elements, including none, and may not have named arguments.
- The new nodes count towards the nesting limit, as the levels of a postfix `?` do.
- Parser unit tests, and examples of valid and invalid syntax in `syntax/examples/`.

## Step 2: Validation

- Resolve the element type, report an unknown type name and a wrong generic arity as for any other type, and check
  duplicates and the shared constructor namespace as for other constructors.
- A type parameter used only inside the brackets, as `T` is in `Items[T]`, counts as used.
- Unit tests, and examples under `syntax/examples/types/`.

## Step 3: Type-check

- The elements of `Items[1, 2]` are checked against the element type, with the usual widening, and the type
  parameters of a generic enum are unified across the elements and across the top-level value.
- `Items[]` is valid while `T` is still unknown, as `[]` is.
- The default of an array is `[]`, so the default of an enum whose first variant is an array variant is `Items[]`, and
  `Join[JoinList<T>]` as a first variant is not recursive. An array has no name, so it cannot be left out by name.
- Examples under `syntax/examples/values/` and `syntax/examples/typed/`.

## Step 4: Errors and shapes

- The shapes are distinct (question 1), and the messages say so in both directions: `Users({...})` or `Users(...)`
  against an array variant gives `Constructor Users takes an array`, and `Users[...]` against a record, tuple or bare
  constructor says what that one takes.
- Types are shown in messages as the constructor with its brackets, and a mismatch names the element at fault.
- Invalid examples with `// expect:` for each of these, which `tests/functest.sh` runs.

## Step 5: Postfix `?`

- On a type, `Users?` is `Option<Users>`, and the element type may have one too, as in `Items[Nat?]`.
- On a value, `Users[u1, u2]?` is `Some(Users[u1, u2])` and has type `Option<Users>`. It needs `-x`, and the
  gate has an example.
- Functional tests and unit tests for each.

## Step 6: Decision record

Write decision 0022: array variants are a distinct form from `Users([User])`, they hold exactly one element type, and
the default of an array is `[]`. Question 1 is a real design decision and nothing records it yet.

## Step 7: CHANGELOG

Bring the CHANGELOG up to date.

