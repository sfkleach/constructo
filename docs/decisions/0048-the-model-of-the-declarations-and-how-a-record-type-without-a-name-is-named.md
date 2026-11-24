# 0048 - The model of the declarations, and how a record type without a name is named, 2026-11-24

## Issue

`constructo codegen` writes code for a project in a target language: Rust first, and Python and TypeScript later.
Every target needs the same facts about the declarations of a run: the projects, the enums and their constructors,
the types of their fields, the consts, how the root project writes each name, and the facts that follow from them,
such as which types are recursive. The question was what a target is given, so that each target only writes code and
none works these facts out again.

Two of the facts needed rules of their own. A record type without a name, such as `{ dx: Int, dy: Int }`, has no name,
and a target such as Rust needs one for its `struct`. And a default, which the reader fills in for an argument that is
left out, depends on the arguments of a generic type, so it cannot be worked out once for each declaration.

This is a **Leap** for the model as a whole: it follows from the draft of the task, and the alternative, a target that
reads the checker's own types, was never a live one. The rules for records were chosen between options, which are
below.

## Factors

- The checker's `Ty` has variables that only inference needs, and names in the canonical form
  `Shape@com.example.shapes`, which is internal and never written in a file.
- A target writes code that people read, so names must not change for a change that means nothing in Constructo,
  such as the order of the fields of a record, or of the declarations.
- A record type is closed and structural: two record types with the same fields and types, in any order, are the same
  type, wherever they are written. A value of one can be given where the other is expected.
- A type that holds itself by value has no size in Rust, so some field on every such cycle needs a pointer. Python
  and TypeScript need nothing, since their values are all references.
- The default of `Styled<T>` needs the default of `T`, and `check` works out a default with a limit on its size,
  `--default-limit`, and only where an argument is left out.
- `codegen` writes code for a project, and a project whose sample values are wrong is not one to write code for.

## Decision

**The model.** `constructo::build_model` checks the sources of a run as `check` does, values and all, and if there is
no error returns a `Model`, in `src/codegen/model.rs`, which is the same for every target. Its types are its own: a
type is `Nat`, `Int`, `Float`, `String`, an array, an enum by its full name with its arguments, a record type without a
name with its arguments, or a parameter. It holds:

- the projects of the run, in the order of the layout, with their imports;
- the enums, in the order of the files and the declarations, each with its parameters and its constructors in order,
  each constructor with its body and the form in which the root project writes it: bare, else through a group, else in
  full;
- the record types without a name;
- the consts, each with its parameters, its type, declared or inferred as `check` infers it, and its value with every
  name in full. A reference to another const stays a reference.

**A record type without a name** is one record in the model for each distinct type:

- **Its identity is its fields,** sorted by name, with their types, where the parameters of the declaration that it is
  written in are renamed `A`, `B` and so on, in the order in which they first occur in the sorted fields. So
  `{ x: T }` in `enum Foo<T>` and `{ x: U }` in `enum Bar<U>` are one record, `x<A>`, used as `x<T>` and `x<U>`, and
  `{ a: T, b: U }` and `{ a: U, b: T }` are one record used with its arguments swapped.
- **Its name is its field names, sorted and joined with `_`,** or `empty` for `{}`. The names are lower-case, as fields
  are, and a declared enum or constructor starts with a capital letter, so a name of a record cannot be one of them.
- **Where two records have the same name,** because they have the same fields with other types, or because the join is
  the same, the first met keeps it, in the order of the files and the declarations, and the others are numbered `_2`,
  `_3` and so on, skipping a name that is already taken. A record that holds another is met after it.
- **The model says which projects use each record.** A target may write it once, or once in each project's module, as
  the Rust draft does.

**What the model works out**, so that no target does it again:

- **`boxed`, on every field and parameter that holds its type by value on a cycle,** and on no other. The graph has a
  node for each enum and each record type without a name. A type holds another directly, in an enum such as `Option`,
  in a field, or through a type argument where the declaration holds that parameter, which is worked out first as a
  fixed point. An array holds nothing. Every field on a cycle is boxed, at every site, so that the type of a field
  depends only on whether it is on a cycle, and not on which declaration came first.
- **`has_default`, false for an enum whose default would expand for ever whatever its arguments:** its first
  constructor holds, through fields and records but not arrays, a type that leads back to the enum. Every other
  default is finite for finite arguments, and the generated code builds it where an argument is left out, within the
  default limit, as `check` does. An enum that holds one with no default has a default itself, whose building reports
  the one that has none, with `check`'s message.
- **Whether a type can be the absent value,** for a target that has one, such as `None` in Python: the prelude's
  `Option` can, and so can a parameter.

## Consequences

- A target is a writer of code from the model, and the facts that every target needs are tested once.
- A program can pass the `pos` of a `Foo<Nat>` as the `at` of a `Bar<Nat>`, as Constructo allows, since the two are one
  type in the generated code.
- The name of a record depends on order only where two records clash, and then only for the later ones.
- A record with many fields has a long name. A limit on it, such as a short hash of the rest, is left open until an
  example needs it.
- The model says which enums have no default, and not the default of each type, so the generated code builds defaults
  at run time and applies the limit there. Where a type grows as it expands, as `enum N<T> = Deep(N<[T]>), Flat(T)`
  does, the model says that it has no default, while `check` stops it at its limit with a message about the limit.
- `codegen` refuses a run that `check` refuses, including one whose values are wrong, which is stricter than
  `convert`, which checks only the values that it converts.

## Options and Outcome

For the identity of a record type without a name:

1. One record for each set of fields with the names of their parameters as written.
2. One record for each set of fields, with the parameters renamed in the order in which they first occur. **Chosen.**

For defaults:

1. The model holds the default of every type.
2. The model holds which enums have no default at all, and the generated code builds the rest. **Chosen.**

## Pros and Cons of Options

### Records by their parameters as written

- Pros: simpler, and the record's parameters keep the names that the declaration gave them.
- Cons: two `struct`s for one type of Constructo, so a value of one could not be given where the other is expected.

### Records with their parameters renamed

- Pros: one `struct` for each type, as the language has it.
- Cons: the parameters of the record are `A`, `B` and so on, and not the names that the declaration wrote.

### The default of every type in the model

- Pros: a target would only copy the defaults.
- Cons: the default of a generic type depends on its arguments, so there is no one default to hold, and one for each
  type that is used could be exponentially large.

### Which enums have no default

- Pros: a fact of the declarations alone, whatever the arguments, and the generated code applies the same limit as
  `check` at the same place.
- Cons: the generated code has to build defaults, and a type that grows as it expands is reported differently.
