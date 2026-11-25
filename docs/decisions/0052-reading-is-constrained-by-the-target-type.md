# 0052 - Reading is constrained by the target type, 2026-11-25

## Issue

Data is often read from a party that is not trusted. A reader that lets the data name the type to build, as some
serialisation libraries do with a tag of the type, can be pointed at a type that was never meant to be read, which is
a well-known source of attacks. The codec of
[decision 51](0051-a-codec-for-each-project-holding-its-names-consts-and-settings.md) reads Constructo text into the
generated types, and the text names constructors and consts. The question was what the text can make the codec build,
and when the names and consts in it are resolved.

This is a **Leap**, from the draft of the codegen task, written after the code.
[Decision 39](0039-reading-json-typed-back-is-driven-by-a-type.md) says that reading JSON back is "driven by" a type,
which describes how the reader is written. This says what the data may do, and is the stronger statement.

## Factors

- The constructors of a run are known when the code is generated, and so are its consts.
- A name in a text can be written bare, through a group, or in full, and the codec has to read each form that `check`
  reads in the root project, and no other.
- A const can refer to another, and its value is read as the type that is expected where it is used, which a generic
  const such as `null` needs.
- An error is useful where it says where it is, in the text and in the value.

## Decision

**The program names the type, and the data can only be what the type permits.** `codec.read::<Drawing>(text)` reads a
`Drawing`, and nothing in the text can make it read anything else.

**The constructors that reading can build are a closed set, fixed when the code is generated.** Every one is a
constructor of the run, generated from the declarations, or adopted explicitly (step 6). There is no registry that can
be added to while the program runs, no lookup of a type by a name in the data, and no open type such as a trait
object. The reader of an enum is a `match` over the full names of its constructors, so a constructor of another type is
an error, `Expected Shape but found Point of type Point`, and a name that the run does not have is one too,
`Unknown constructor Hexagon`.

**The names and consts are resolved as the value is read,** and not in a pass of their own. At a constructor the reader
asks the codec for its full name, from a table of every name by which the root project reads it, which is worked out
by the resolver that `check` uses (decision 53). At a const it asks for the const's value, which is text with every name
in full, parsed once for each codec, and reads it as the type that it already expects there, at the same depth. An
error inside a const's value is at the reference, with the const named.

**An error has the position in the text and the path in the value,** as in
`tail.head: Expected Nat but found String (line 1, col 36)`. The path is built only when there is an error, as the
error passes out of each field, parameter and element. The messages are `check`'s where the error is one that `check`
finds.

**A const whose value Rust cannot hold is refused when the code is generated,** with an error at the const and exit
code 1, so that it is not left to fail on the first read.

## Consequences

- A text from anywhere can only build values of the type that the program asked for, through constructors that the
  declarations have.
- A name that `check` resolves in the root project is one that the codec reads, by construction, since the same
  resolver gives the table. The harness reads each form of a project with a group that renames and hides.
- `check` works out the types of a value as it goes, and the codec is given them, so they do not always find the same
  error first, and the codec reads some values that `check` refuses only for want of a type, such as `Cell("x")` read
  as a `Cell<Nat>`.
- The size of a const is fixed by the declarations, so the data can make the work larger by referring to one many
  times only by a factor that is known when the code is generated.
