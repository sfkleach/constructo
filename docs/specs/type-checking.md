# Type checking

A values file holds data-values, and a types file can hold consts, whose values are data-values as well. The type check
decides whether such a value fits the types that were declared. This document gives the rules, the messages and the
options. It comes after the checks of [the validity of the declarations](validity-checks.md), and it is stage 5 of a run
of [the check command](check-command.md). It is skipped by `--no-type-check`, and then a value only has to parse.

## What is in scope

A value sees every enum and every const of every types file of the run, and of the prelude. That is because the types
files must come before the values files, and the order of declarations does not matter. A name is resolved in the
project of the file that writes it, as [resolving identifiers](resolving-identifiers.md) says.

A values file holds top-level values separated by semi-colons. Each is checked by itself, and nothing is carried from
one to the next. So the type that a generic parameter takes in one value has no effect on the next.

## What a value can be

| Value                 | Example                  | Fits                                                         |
|-----------------------|--------------------------|--------------------------------------------------------------|
| Natural number        | `5`                      | `Nat`, and with widening `Int` and `Float`                   |
| Integer               | `+5`, `-5`               | `Int`, and with widening `Float`                             |
| Float                 | `5.0`                    | `Float`                                                      |
| String                | `"text"`                 | `String`                                                     |
| Array                 | `[1, 2]`                 | `[T]` when every element fits `T`                            |
| Untagged record       | `{ x: 1, y: 2 }`         | a record type with those fields                              |
| Tagged value          | `Circle { radius: 1.0 }` | the enum that declares the constructor `Circle`              |
| Reference to a const  | `limit`, `null`          | the type of the const                                        |

The sign decides the type of a number. `+5` is an `Int`, even though it is not negative, so it does not fit a `Nat`:
the error is `Expected Nat but found Int`. A negative number never fits a `Nat`. A number with a point is a `Float`,
and the fraction may be followed by an exponent, as in `1.5e3`. `1e5` is not a number.

### Numbers and widening

By default a number fits any type that is at least as wide as its own, where `Nat` is narrower than `Int`, which is
narrower than `Float`. So `5` can be given for a `Float`. `--no-widening` makes each number exact: `5` is only a `Nat`.
Widening applies in the same way to a reference to a const, as below, and to the elements of an array. Widening to
`Float` may lose precision for a very large `Nat` or `Int`.

### Tagged values

A constructor belongs to one enum, which decides the type of the value. The form that is written has to be the form that
was declared, for each of the four forms of a variant:

| Declared as           | Written as               | Wrong form                          |
|-----------------------|--------------------------|-------------------------------------|
| `Dot`                 | `Dot`                    | `Constructor Dot takes no fields`   |
| `Circle { r: Float }` | `Circle { r: 1.0 }`      | `Constructor Circle takes a record` |
| `Pair(Nat, Nat)`      | `Pair(1, 2)`             | `Constructor Pair takes a tuple`    |
| `Items[Nat]`          | `Items[1, 2]`            | `Constructor Items takes an array`  |

So `Ping()` is a tuple with no values and `Pong {}` is a record with no fields, and neither is accepted for the other or
for a bare `Ping`. The array form is a form of its own, and `Items([1, 2])` is not accepted for it, nor `Items[1, 2]`
for a tuple that takes an array.

**Records.** The fields of the value must be those of the declaration, as a set, in any order. `Duplicate field X`,
`Unknown field X in NAME` and `Missing field X in NAME` are the errors, where NAME is the constructor, or `record` for
the fields of an untagged record. The position of a missing field is that of the record.

**Tuples.** The arguments are bound to the parameters:

- The arguments come first by position, and then by name as `name: value`. Positional arguments bind a prefix of the
  parameters, and the named ones may then follow in any order.
- An argument can only be given once, whether by position or by name: `Argument x is given more than once in NAME`. A
  name that is not a parameter is `Unknown argument z in NAME`.
- A parameter that has a name may be left out, as the next section says. A parameter that has no name must be
  supplied, or else `Constructor NAME takes N value(s) but has M` is reported.
- The names of the parameters are not part of the type. They only decide which parameter an argument is for.

**Arrays.** Every element must fit the one element type of the declaration. The elements are plain values, with no
names.

### Omitted arguments

An argument that is left out takes the default value of the type of its parameter. The default depends on the type
alone:

| Type            | Default                                                                      |
|-----------------|------------------------------------------------------------------------------|
| `Nat`, `Int`    | `0`                                                                          |
| `Float`         | `0.0`                                                                        |
| `String`        | `""`                                                                         |
| an array        | `[]`                                                                         |
| a record        | the record of the defaults of its fields                                     |
| an enum         | its first constructor, with the defaults of its fields or parameters         |

So the order of the constructors of an enum matters: the prelude has `enum Bool = False, True;` so that the default of a
`Bool` is `false`, and `Option<T>` has `None` first. The checker only checks, and does not write the omitted values out.

A default that cannot be used is an error where the argument is omitted, and not at the declaration, so a type that is
never defaulted is not affected:

- `Type T has no default value`, if the default contains the type itself.
- `The default value of T has more than N nodes, which is too large; give argument x or raise --default-limit`, if it is
  not recursive but is too big.
- `The default value of T is nested more than N levels deep, which is too large; give argument x`, if it is nested too
  deeply.
- `Cannot work out the default value of argument x: its type is not known`, if the type is a type parameter that is
  still open.

The size of a default is the number of values in it, counted without building it. The limit is applied to each omitted
argument, and `--default-limit=N` sets it, with 10,000 as the default. An enum whose first constructor contains the enum
cannot be omitted, so write the base case first, as in `enum List<T> = Nil, Cons { head: T, tail: List<T> };`.

## Generic types

The type parameters of an enum are replaced with fresh unknown types each time a constructor is used. The unknowns
are then fixed by the values that are met, across the whole of the top-level value. So

- `Cons { head: 1, tail: Cons { head: "a", tail: Nil } }` is an error, `Expected Nat but found String`, at the `"a"`;
- `Cons { head: null, tail: Nil }` is fine, and so is `[None, Some(1)]`.

Where two numbers meet in an unknown type the result is the wider type, so `[1, 2.5]` has type `[Float]` and
`[1, +2]` has type `[Int]`. This is the join, and it needs widening: under `--no-widening` these are errors. A type
that was fixed by a declaration is not joined. An element of an `[Nat]` cannot be `2.5`.

An untagged value at the top level, such as `{ x: 1 }` or `[1, 2]`, names no type, so it is only checked for being
consistent in itself. An untagged record, in particular, never fails at the top level.

## Optional values

`Option<T>` is declared in the prelude as `enum Option<T> = None, Some(T);` and `T?` in a type is written for it by
the parser. So `retries: Nat?` is a field of type `Option<Nat>`. A value of type `Nat` is not accepted where an
`Option<Nat>` is expected, whatever the syntax, because the structure of a value must not depend on the types: it has
to be `Some(3)` or `3?`. The postfix `?` on a value is experimental and has to be switched on with `-x=some-query`.
Without it the error is `Postfix '?' is experimental; enable it with --experimental=some-query`. The [configuration
file](config-file.md) can switch it on too.

Under `--no-prelude` there is no `Option` unless the user declares one, and `T?` is then an unknown type.

## Consts

A const is a name for a value. It lives in the same namespace as the constructors, and does not take fields.

```
const limit: Nat = 5;
const none<T>: Option<T> = None;
const names = ["a", "b"];
```

### The type of a const

The type is the one that is written, or if it is left out, the type that is worked out for the value, as for a top-level
value. So `0` is a `Nat`, `+0` an `Int`, `0.0` a `Float`, a tagged value has the type of its enum, and `[1, 2.5]` is
`[Float]`. A type that is inferred is never wider than the value: a user who wants `const x` to be an `Int` has to write
the type.

The parts that the value leaves open are generalised: `const e = [];` has the type `[T]` for any `T`, and
`const m = null;` is an `Option<T>`. Each reference instantiates them again, so two uses of `null` in a value may have
different types. A const that is written with a type may have type parameters, which are in scope in that type only,
and which follow the rules of the parameters of an enum. A const without a type may not have them: `Type parameters of
const NAME need a type`.

The value of a const is checked against its type, with the same rules as a value in a values file. The value of a
generic const is checked with its parameters as unknown types, so it is good whatever type is chosen.

If the inferred type has more than 10,000 nodes the const is refused: `The type of const NAME has more than N nodes,
which is too large to infer; give it a type or raise --infer-limit`. The limit is `--infer-limit=N`. It is there because
a short value can have a type that is exponentially large when it is written out.

The types are worked out in the order of the references between the consts, so a const may refer to one that comes
later. A const whose type cannot be worked out is reported once, and the consts that refer to it are not checked against
it, so one mistake is not reported again for each of them.

### References

A value may always refer to a const of a types file. The definition of a const may refer to another const only under
`--allow-refs`, and then the references must not form a circle. See [validity checks](validity-checks.md). The consts of
the prelude, `null`, `true` and `false`, can always be used.

The type of a reference is the type of the const, and it is accepted wherever a value of that type would be, with the
same widening. So `const x: Nat = 0;` can be used where an `Int` is expected, but not under `--no-widening`, and a
`Nat` const is not accepted for an `Option<Nat>`. The mismatch says `Expected Int but found const x of type Nat`.

A bare name in a value is a constructor if it is the name of one, and otherwise a const. `Unknown constructor X` is
reported for a name that starts with a capital letter and `Unknown const x` for one that starts with a lowercase
letter or an underscore. A const that is followed by fields is `Const x is a value and takes no fields`.

### Record types

An untagged record has a type that is written as `{ left: String, right: String }`. It is closed: a value fits it only
if it has exactly those fields, in any order. Two such types are the same type if they have the same fields with the
same types, whatever the order. A tagged record never has an untagged record type, and an untagged record is never a
value of an enum, so `const x: { a: Nat } = Foo { a: 1 };` and `const x: Foo = { a: 1 };` are both errors. `{}` is the
record with no fields. There is no type of records with at least some fields.

## Errors

Every type error says what was expected and what was found:

```text
ERROR: Expected Nat but found String (bad.values, line 1, col 27)
```

- The position is that of the innermost term that is wrong, which is a literal or a constructor. Only that term is
  reported, so one mistake does not cause a series of messages about the terms around it.
- What is expected is a type as it is written, with `Option<Nat>` for `Nat?`, and `_` for a type that is still
  unknown. What is found is `Nat`, `Int`, `Float`, `String`, `an array`, `a record`, `NAME of type ENUM` for a
  constructor, or `const NAME of type TYPE` for a const.
- An argument of a constructor that has named parameters says which: `Expected Nat for argument width but found String`.
  This is not said for an argument that ends in `?`, which has the same position as its `Some`.
- An error that would make a type contain itself says `Cannot use X, which would make a type that contains itself`.

One error is reported for each top-level value, and for each const. The first one ends the run, so that is the only
error, unless `--continue` is given, in which case the check goes on with the next value, including the values of
later files. The consts are checked before the values. A types file with an error in a const is `FAILED` in the
verbose output even if its error was not one of those reported, and a values file that was not reached is not listed.

## Related

- [The check command](check-command.md), for the stages around this one.
- [Validity checks](validity-checks.md), which must pass first.
- [Resolving identifiers](resolving-identifiers.md), for the scope of names.
- Decisions 9 to 11, 13 to 20 and 22, in `docs/decisions/`.
