# 0049 - How the modules and names in Rust are chosen, 2026-11-25

## Issue

`constructo codegen --lang=rust` writes the Rust types of a project and the projects that it imports, from the model of
[decision 48](0048-the-model-of-the-declarations-and-how-a-record-type-without-a-name-is-named.md). A Constructo name
is not always a Rust name: it can be a keyword, two projects can declare the same name, a constructor can have the name
of an enum, and a name of the declarations can hide one that the generated code needs. And some Constructo types have
no Rust type that a program can use. The question was how the file is laid out, how each name is written, and what is
done where Rust cannot follow.

This is a **Leap**: the draft of the codegen task worked the rules out against the spikes, and building them changed
four of them, which are marked below. The record is written after the code, which compiles every example.

## Factors

- A program mounts the file with a name of its own, `mod service_a;`, and calls its types by those names, so a name
  that the generator chooses is a promise, and should change only when the declarations do.
- In Rust a module and a type share one namespace, and a type parameter hides a type of the same name in the item
  that declares it.
- A raw identifier, `r#type`, is the same name to Rust, but `self`, `super`, `crate`, `Self` and `_` cannot be raw.
- Rust refuses a type parameter that no field uses (E0392).
- Rust monomorphizes: each instance of a generic type has its own code and its own drop glue. A type that holds itself
  with other arguments, as `enum Nested<T> = Flat(T), Deep(Nested<[T]>)` holds a `Nested<[T]>`, has an instance for
  each depth, and Rust's drop-check overflows on it as soon as a value is made, through a `Box` or a `Vec` alike.
  `check` accepts it, and so do Haskell and OCaml, whose values have one representation, and Java, TypeScript and
  Python, whose generics are erased.

## Decision

**The file is the module of the root project,** and every other project of the run is a module in it, named by the
parts of its full name joined with `_`, with their case kept. The prelude has none, since its `Bool` and `Option` are
Rust's own, unless it declares something else. A project with no types has an empty module, so that a path to it does
not depend on what it declares.

**Every path is written in full:** the items of std as `::std::vec::Vec`, the primitives as `::std::primitive::u64`,
and a generated type as `self::Shape`, `super::Shape` or `super::com_example_ids::Uuid`. A type parameter is the only
name that is written bare. So no name of the declarations can hide one that the file needs: a record whose one field
is `u64` is a `struct u64`, and the record of `{ x: T, y: A }` has the parameter `A`, which would otherwise hide an enum
`A`. *Added while it was built,* since the draft wrote only the items of std in full.

**Names are kept where they can be, and every change is a warning at the declaration:**

- A keyword of Rust 2021, or a word reserved by it or by Rust 2024, such as `gen`, is a raw identifier, which is not a
  change. A name that cannot be raw gets a `_` at the end, repeated while it is taken.
- A parameter of a tuple with no name is a field named by its position, `_0`, `_1` and so on, and a named parameter
  that has one of those names gets a `_`.
- A constructor that has the name of an enum of its project, its own included, takes the name of its own enum in front,
  `ShapeCircle`, and a number after that if that is taken too. An enum keeps its name, since fields refer to it. *Its
  own included* was added while it was built, since `enum Holder = Holder(Nat), Empty` has an enum and a `struct` of
  the one name.
- A type with a name that the generated code adds, `Codec`, `ReadValue`, `WriteValue` or `DefaultValue`, gets a `_`.
  These are reserved from the first version, before the codec writes them, so that adding the codec renames nothing.
- A module whose name is taken, by another module or a type of the root, is numbered, and the other keeps its name.

**A type parameter that no field holds is left out of the Rust type,** and out of every use of it, so
`enum Phantom<_T> = Phantom` is `struct Phantom`, and `Phantom<Nat>` is `Phantom`. A parameter is kept if a field holds
it, directly, in an array, or as the argument of a kept parameter of another type, which is worked out as a fixed
point, since `enum Q<T> = Q(Phantom<T>)` then has no use for its `T` either. A parameter whose name does not start with
`_` and is left out is a warning. *Changed while it was planned,* from the draft's `PhantomData`.

**A type that holds itself with other arguments is refused,** with an error at its declaration, exit code 1, and
nothing written: `Rust cannot hold the enum Nested, since it holds itself with other arguments`. Each cycle of types is
followed from its first member with its own parameters, and a member that is reached with two sets of arguments is the
error. A type that holds itself with the same arguments, as `List<T>` holds a `List<T>`, is regular and is written.
*Changed while it was built:* the draft expected the types to compile and the codec's bounds to be the risk.

## Consequences

- The names of a program's types depend only on the declarations, except where two names clash, and then only for the
  later ones.
- The generated code is longer to read, with every path in full, but it is not meant to be read often, and the program
  writes `service_a::Shape`.
- A Rust type can have fewer parameters than its Constructo type, and an adoption of it (step 6) has to follow.
- A project that declares a type that holds itself with other arguments cannot be generated for Rust at all, though a
  target in another language can generate it. Such types are rare in data, and the error says why.

## Pros and Cons of Options

### A type parameter that no field holds

- **Left out.** The type has nothing that the declaration does not, no value differs, and nothing hidden is in the
  program's values or `match`es. The Rust type has fewer parameters than the Constructo one.
- **A `PhantomData`.** The parameters are the same, but every value that a program builds has the field, and an enum of
  several constructors needs a hidden variant, which every `match` has to name.

### A type that holds itself with other arguments

- **Refused.** Honest about the limit of Rust, and found before the program is compiled.
- **Written, with a warning.** The file is written, and the program fails to compile where it uses the type, with a
  message of Rust's that does not say why.
- **Erased at the recursion,** as an untyped tree. It compiles, but the field loses its type, and the codec needs a case
  of its own for it.
