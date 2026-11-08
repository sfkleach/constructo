# Validity checks

A types file can be well-formed and still make no sense: it can declare a name twice, use a type that does not exist, or
give a generic type the wrong number of arguments. The validity checks find these. They are stage 4 of a run of
[the check command](check-command.md), after parsing and the resolution of names, and before the
[type check](type-checking.md). Values files take no part in them.

## What is checked

The declarations of every types file of the run are checked together, as one namespace, and the prelude's are among
them. So a schema may be split over any number of files, in any order, and a duplicate between two files is an error
like any other. The names are those of one scope, as [resolving identifiers](resolving-identifiers.md) says: two
projects can each declare a `Shape` unless they share a group.

The run does not stop at the first error. Every error is reported, in order of file, line and column, with the
position of the name that is at fault. The checks are skipped if a syntax error has ended the run, because the file
that did not parse may have held the declarations that the others refer to, and the checks would report names as
unknown that are not. They are skipped as well if a name could not be resolved. If there is any error, the values are
not type-checked.

## Namespaces

There are three kinds of name, and a name can only be declared once in its own kind:

- **Types.** The names of the enums. A type parameter is a type name in the declaration that has it, and nowhere else.
- **Values.** The names of the constructors and of the consts, which share one namespace. A const may therefore not have
  the name of a constructor, nor of another const.
- **Members.** The names of the fields of a record and of the named parameters of a tuple constructor, which belong to
  the one record or constructor, and so cannot clash with anything outside it.

A type and a value may have the same name. `enum Config = Config { port: Nat };` is valid, and so is `enum Colour =
Rgb(Nat, Nat, Nat); const Colour: Colour = Rgb(255, 0, 0);`. They are told apart by where they are written.

The names of the basic types, `Nat`, `Int`, `Float` and `String`, cannot be given to a type, a constructor, a const or a
type parameter. They are always the basic types.

`const` is a reserved word, so it cannot be the name of a field. That is a syntax error.

## The errors

The message of a duplicate is followed by `; first declared at line L, col C`, or `; first declared at FILE, line L,
col C` when the first is in another file. The error is on the later declaration, where later is the order of the files
and then of the text.

### Names

| Message                                          | When                                                        |
|--------------------------------------------------|-------------------------------------------------------------|
| `Type name X clashes with a basic type`          | `enum Int = A;`                                             |
| `Constructor name X clashes with a basic type`   | `enum E = Nat;`                                             |
| `Const name X clashes with a basic type`         | `const Int: Nat = 0;`                                       |
| `Duplicate type X`                               | Two enums have one name.                                    |
| `Duplicate constructor X`                        | Two constructors have one name, in the same enum or not.    |
| `Duplicate const X`                              | A const has the name of a constructor or of another const.  |
| `Duplicate field X`                              | A record has a field twice, in a variant or in a record type. |
| `Duplicate parameter X`                          | A tuple constructor has two parameters with one name.       |

### Types

Every type that is written, in the field of a variant, the parameter of a constructor, the element of an array variant,
or the type of a const, is checked, including those inside arrays, records and the arguments of other types.

| Message                                          | When                                                         |
|--------------------------------------------------|--------------------------------------------------------------|
| `Unknown type X`                                 | The name is not a type parameter, a basic type or an enum.   |
| `Type X takes N type argument(s) but M supplied` | The number of arguments is not the number of parameters.     |

A basic type takes none, and so `Nat<Nat>` is an error. A generic type needs all of its arguments: `Option` on its own
is an error, as the arguments cannot be left to be worked out.

### Type parameters

The type parameters of an enum, and of a const that has a type, are checked when it is declared.

| Message                                          | When                                                         |
|--------------------------------------------------|--------------------------------------------------------------|
| `Duplicate type parameter T`                     | `enum E<T, T> = A(T);`                                       |
| `Type parameter X clashes with a basic type`     | `enum E<Nat> = A(Nat);`                                      |
| `Type parameter X clashes with a declared type`  | The name is that of an enum that the file can see.           |
| `Unused type parameter T`                        | `enum E<T> = A;`: the parameter is not used in the body.     |
| `Type parameters of const X need a type`         | `const x<T> = 0;`: the parameters belong to a type.          |

A parameter whose name starts with an underscore, `<_T>`, may be unused. It is a phantom parameter. It counts towards
the arity, and as `_T` is not a type name it cannot be used, so it is always unused.

### Consts

| Message                                              | When                                                     |
|------------------------------------------------------|----------------------------------------------------------|
| `Reference to const X in const Y requires --allow-refs` | Y names X, without the option. |
| `Circular const definition X -> Y -> X`              | With `--allow-refs`, the definition goes round in a circle. |

A value in a values file may always name a const. The value of a const may name another only under `--allow-refs`, and
a const of the prelude can always be named. The circle is reported at the reference that closes it, with the consts on
it. Both checks are made even under `--no-type-check`. The type of a reference is a
matter for the [type check](type-checking.md).

## What is not checked

These checks do not look at values. They also leave the following alone:

- A type that cannot be built, such as `enum A = A(A);`, is valid. No value of it can be written, and that is not an
  error until a value is given.
- A type, a constructor or a const that nothing uses is not reported.
- Names that are written in a values file. A constructor that is unknown there is a type error, and is found only if the
  values are type-checked.
- Whether a const's value fits its type, which is also left to the type check.

## The prelude

The prelude is a project of its own, in the `local` group of every project unless it is imported elsewhere. A
declaration in a project that has the name of a declaration in any project of its `local`, in the same namespace, is a
duplicate like the others above, with the note `first declared at FILE, line L, col C` for the other project. So `enum
Bool = No, True;` has two errors, for `Bool` and for `True`, when the prelude is loaded. A use of the name means the
project's own declaration, and is not reported again. Importing the prelude into another group, such as `std`, or
`--no-prelude`, removes the clash. The same holds for two imports that share a group and a name, in every group of every
project: the later declaration, in the order of the imports, is the duplicate, and there is no ambiguity at a use, as
[resolving identifiers](resolving-identifiers.md) says. These duplicates are found while the names are resolved, so the
other checks do not run when there is one. Decisions 31 and 32 record this.

## Related

- [The check command](check-command.md), for where the checks come in a run and how they are reported.
- [Type checking](type-checking.md), for what comes next.
- [Resolving identifiers](resolving-identifiers.md), for how a name is matched to a declaration.
- Decisions 1, 3, 4, 12 to 14, 25, 26, 31 and 32, in `docs/decisions/`.
