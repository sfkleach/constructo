# 0056 - A generated class may give a named parameter its default, 2026-11-26

## Issue

A named parameter of a tuple constructor may be left out of a text, and the argument then takes the default of its
type ([decision 18](0018-an-omitted-named-argument-takes-the-default-of-its-type.md)). A generated codec fills it in
when it reads the text. A program that builds a value in code has no such help unless the generated type gives it one,
and in a language with default arguments it can: `Point(x=4.0)` in Python can be what `Point(x: 4.0)` reads as. The
question was whether a generated type gives a parameter its default, and where the default comes from.

This is a **Leap**, from the draft of the codegen task and its scenario for Python, written before any target that it
applies to. Rust has no default arguments, so the Rust target has none. It is written now so that it is not lost when
the next target is built.

## Factors

- The binding of decision 17, unnamed parameters by position and then named ones by position or by name, is the binding
  of a Python class whose fields have defaults after those that do not.
- The codec already has a function for the default of each type, which it calls for an argument that is left out of a
  text, with the limit on its size.
- A default can fail: a type whose default would expand for ever has none, one can be too large, and the function of an
  adopted type can refuse the default of its arguments.
- A type checker refuses a field whose default is not of its type, so `None` cannot stand in for the default of a
  `Float`, as a run of mypy on the scenario's spike showed.

## Decision

**A target whose language has default arguments may give a named parameter of a generated class its default,** so that
a program can leave it out as a text can.

- **Only a named parameter has a default.** An unnamed parameter and the field of a record are required, as they are in
  the text.
- **The default is the codec's.** The class's default is the same generated function that the codec calls when an
  argument is left out of a text, so the two cannot differ: there is only one of them.
- **No `None` stands in for a default.** A field's default is `None` only when its type is an `Option`, which is when
  decision 18 says that the default is `None`.
- **A default that fails, fails where it is left out,** as in the text. The adopted `Uuid` of the scenario has no
  default, so `Drawing(title="T")` raises the refusal of the adopted function, as `Drawing(title: "T")` is refused when
  it is read.
- **A parameter of the type `T` has no default,** since the type is not known when the program runs. One whose default
  does not depend on `T`, such as `[T]` or `T?`, has one.
- **A test holds them together:** for each type, a value built with arguments left out equals the value that the codec
  reads from the text with the same arguments left out.

**Rust has none.** `impl Default`, with `..Default::default()`, is the nearest, but it cannot fail and it covers every
field, so the Rust target writes no defaults for a program, and the codec's `DefaultValue` is used only when a text is
read.

## Consequences

- A program in such a language builds a value as it would write the text, and gets the same value.
- The default of a class can fail when the class is built, which a program in Python does not expect of a default. It
  is the same failure as reading the text, and the alternative, a default that cannot fail, would differ from it.
- A Rust program gives every argument. An `impl Default` for a type whose defaults all succeed could be added later.
