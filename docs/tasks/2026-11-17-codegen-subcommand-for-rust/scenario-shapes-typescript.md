# Scenario: the drawing program in TypeScript

The thought experiment of the [Rust scenario](scenario-shapes.md) and the
[Python scenario](scenario-shapes-python.md), done again for TypeScript, with the
same projects, the same adoptions and the same data. TypeScript was chosen for
what it does not share with the other two: its numbers are all doubles, its types
are gone when the program runs, its unions are of plain objects, and it has two
values for nothing.

As before, the generated code is written by hand, as `codegen --lang=typescript`
would write it, and it runs. It is in [scenario-shapes/typescript](scenario-shapes/typescript/),
laid out as `--out-dir=shapes` would write it: a folder `shapes/` with
`index.ts` for the root project and `com_example_ids.ts` for the other, beside a
stand-in runtime. It applies every decision so far, including names and consts
resolved as the value is read. `tsc` 7.0.2 checks it, in strict mode, and Node 24
runs it as it is, since it removes the types itself.

What it writes is accepted by `check`, and is the same text, byte for byte, as the
Python spike writes.

## What the generated code looks like

The types ([shapes/index.ts](scenario-shapes/typescript/shapes/index.ts)):

```typescript
export type Circle = { $kind: "Circle"; radius: number };
export type Box = { $kind: "Box"; width: number; height: number };
export type Dot = { $kind: "Dot" };
export type Polygon = { $kind: "Polygon"; items: Point[] };
export type Shape = Circle | Box | Dot | Polygon;

export type Point = { x: number; y: number };             // one constructor: no $kind

export type Colour$Constructors = "Red" | "Green" | "Blue"; // the step in between
export type Colour = paint.Colour;                        // adopted whole

export type Styled<T> = { item: T; colour: Colour | null; offset: { dx: number; dy: number } };
export type Drawing = { id: com_example_ids.Uuid; title: string; shapes: Styled<Shape>[]; layer: number };
```

and, for each type, a value of the same name, which is how a codec reads, writes
and defaults it:

```typescript
export const Shape: rt.Type<Shape> = { read(c, v) { ... }, write(c, x) { ... }, default(c) { ... } };
export function Styled<T>(item: rt.Type<T>): rt.Type<Styled<T>> { ... }
export const Drawing: rt.Type<Drawing> = { ... };
```

The program names the type by its value, and the result is typed:

```typescript
const codec = shapes.codec();
const drawing: Drawing = codec.read(Drawing, text);
const text2 = codec.write(Drawing, drawing);
const ids: string[] = codec.read(rt.array(Uuid), "[ids::Uuid(...), ...]");
```

The stand-in runtime ([constructo-runtime/index.ts](scenario-shapes/typescript/constructo-runtime/index.ts))
has the tree of a value, `Type<T>`, the helpers, and the basic types and those of
the prelude (`nat`, `int`, `float`, `string`, `bool`, `array`, `option`). It
parses and prints by running the binary, as the Python one does.

## What happens when it runs

The drawing is read, written and read back the same, by `deepStrictEqual`, and
`check` accepts what is written. The refusals:

| The text or the value has              | The error                                                                  |
|----------------------------------------|----------------------------------------------------------------------------|
| `Point(1.0, 2.0)` for a `Shape`        | `shapes[0].item: Expected Shape but found Point`                           |
| `Hexagon`, not in the run              | `shapes[0].item: Unknown constructor Hexagon`                              |
| a bare `Uuid`                          | `id: Unknown constructor Uuid`                                             |
| `ids::Uuid("not-a-uuid")`              | `id: "not-a-uuid" is not a UUID`, from the adopted function                |
| `Drawing(title: "T")`                  | `id: Argument id of Drawing is left out, and its type has no default: ...` |
| a `Nat` of 2^53 + 1                    | `layer: The number 9007199254740993 does not fit Nat`                      |
| a `Float` of `1.0e400`                 | `shapes[0].item.radius: The number 1.0e400 does not fit Float`             |
| `unit_square`, not a const             | `shapes[0].item: Unknown const unit_square`                                |
| `{ dx: 0 }` for `{ dx: Int, dy: Int }` | `shapes[0].offset: Missing field dy in record`                             |
| `paint.Colour(0x336699)`, written      | `shapes[0].colour: The colour #336699 has no Constructo form`              |
| `layer = 1.5`, written                 | `layer: Expected a Nat but found number 1.5`                               |
| `layer = -1`, written                  | `layer: Expected a Nat but found number -1`                                |
| `layer = 2 ** 60`, written             | `layer: Expected a Nat but found number 1152921504606847000`               |
| a NaN, written                         | `shapes[0].item.radius: The Float NaN cannot be written`                   |
| `title = 5`, from untyped code         | `title: Expected a String but found number 5`                              |

The message for `2 ** 60` shows the problem itself: JavaScript prints the number
as `1152921504606847000`, and 2^60 is `1152921504606846976`.

And what the type checker refuses before the program runs, each line marked
with `@ts-expect-error`, so that `tsc` fails if it is not an error:

- `codec.write(Drawing, 5)`: a number is not a `Drawing`. Python's type checker
  let this through.
- `const s: Shape = { radius: 1 }`: a `Shape` needs its `$kind`.
- `codec.read(Styled, text)`: `Styled` needs the type of its item.

## What carries over unchanged

- The closed set: a `switch` on full names, and only the types that have a value
  in the generated code can be named.
- The codec of a project, its tables, the full name as the key, the names written
  bare, else through a group, else in full.
- Names and consts resolved as the value is read. The paths of the errors are
  those of the reader, `shapes[0].item`, as decision 4 meant; the Python spike,
  which still has a pass of its own, says `[2][0].item` for the same error.
- Adoption of a constructor and of a whole enum, with the generated constructors
  as the step in between, an enum with one constructor as one type, writing that
  can fail with a path, and `Option<T>` as `T | null` where no value of `T` is
  `null`.

## What is different for TypeScript alone

1. **The types are values too.** Types are gone when the program runs, so the
   codec cannot be given a type: it is given a value, `Drawing`, which knows how
   to read, write and default it, and has the type's name. TypeScript keeps the
   names of types and of values apart, as Constructo does, so the two can share a
   name. A generic type is a function of the values of its parameters,
   `Styled(Shape)`. And unlike in Python, the result is typed for every type,
   since `Type<T>` carries `T`.
2. **A name that the generator adds has a `$`,** such as `$kind`,
   `Colour$Constructors` and `offset$Record`. A Constructo name cannot have a
   `$`, so these never clash with one, and the rule for Rust and Python, that a
   Constructo name yields to a generated one, is not needed.
3. **A global is written in full,** as `globalThis.Map`, since a Constructo type
   called `Map`, `Error`, `Date` or `Record` would hide it in the module, as the
   Rust code writes the std items in full.
4. **A sum type is a union of plain objects** with a tag, `$kind`. An enum whose
   constructors all have no body is a union of strings. An enum with one
   constructor has no tag.
5. **A record type without a name needs no name.** TypeScript's types are
   structural, as Constructo's records are, so `{ dx: number; dy: number }` is
   the type, and the rule that names a record by its fields is for Rust and
   Python. The other side of it is that a type with one constructor, which has no
   tag, is the same type to TypeScript as any record with the same fields:
   `Point` is any `{ x: number; y: number }`. The codec is not confused, since it
   reads and writes by the value that the program names, but a program can be.
6. **`Nat`, `Int` and `Float` are all `number`,** so the value of the type says
   which is meant, and the writer writes `2` as `2.0` for a `Float` and `+2` for
   an `Int`.
7. **There are no traits and no orphan rule,** so `Type<T>` and the basic types
   are in the runtime, shared by every codec, and the codec is given to them as a
   parameter, `c`, which is the `self` of the Rust and Python codecs.
8. **An adopted function refuses by throwing `Refused`,** a class of the runtime,
   and any other error is a fault that passes through. It is named in the
   adoption file as a module and an export.
9. **The adoptions import the generated package for its types only**
   (`import type`), so there is no cycle when the program starts, and the lazy
   import that Python needed is not.
10. **Values have no equality,** so a program compares them with something such
   as `deepStrictEqual`, as the spike does. An adopted class compares by
   identity unless it says otherwise.
11. **A program builds a value as an object literal,** which has no defaults, so
   the decision on defaults for a class does not apply. A function to build each
   type, with defaults, could be generated, and is not.

## What changes the common design

1. **A number that does not fit is refused, as for Rust, or held exactly, as for
   Python.** `number` holds a whole number exactly only up to 2^53 - 1. The
   spike refuses a larger one, loudly, when it is read and when it is written,
   as Rust does with `u64`. The exact alternative is `bigint`, which is awkward
   to use. The draft's principle of a faithful mapping first was written about
   values that become the same value without a word, such as `Some(None)` and
   `None`. A number that is refused is not that. The principle should say so: a
   mapping may refuse what it cannot hold, but never change it without saying.
2. **A target has settings of its own.** The generated modules import each other
   with an extension: `.ts`, which Node and the bundlers accept, or `.js`, which
   is what `tsc` writes out. Which is right depends on how the program is built,
   and the generator cannot know. Whether `Nat` is `number` or `bigint` is
   another such choice. The command needs a way to give a target its settings,
   such as an option for each, or a table for the target in a file.
   *Decided since:* a setting that changes the generated code, such as these two,
   is given to the command for the target; one that changes only what a codec
   does, such as the limits and the form of names, is a property of the codec,
   which a program can change, and which is cheap and never shared by default.

3. **Where the reading of a type lives is the target's choice.** The draft says
   the traits are generated with each codec, because of Rust's orphan rule.
   TypeScript has no such rule, and keeps `Type<T>` and the basic types in its
   runtime. What is common is only that the codec's names, consts and limits are
   what the readers use. The draft should say that, and leave where the code is
   to the target.
4. **How generated names avoid Constructo names is the target's choice:** a name
   that yields in Rust and Python, a `$` in TypeScript. The draft gives the Rust
   rule as if it were the rule.

## How to run it

With the binary built, Node 24 and npm. TypeScript and the types of Node are put
in `_build/`, so that nothing is installed in `docs/`:

```
npm install --prefix _build/scenario/ts typescript@7.0.2 @types/node@24
cd docs/tasks/2026-11-17-codegen-subcommand-for-rust/scenario-shapes/typescript
../../../../../_build/scenario/ts/node_modules/.bin/tsc -p .
cd ../projects/shapes
CONSTRUCTO=../../../../../../_build/target/debug/constructo \
    node ../../typescript/main.ts values/drawing.values ../../../../../../_build/scenario/ts-written.values
```

`package.json` lists the two as dev dependencies, for the record. Running
`npm install` in the folder would put a `node_modules` in `docs/`.

## What is not real

- There is no `codegen --lang=typescript`. The generated package is written by
  hand.
- The runtime runs the binary to parse and to print, so it has no positions in
  the text except for a syntax error, and a record cannot have a field twice.
- A tagged union with one `$kind` for every enum is one choice of several; a
  class for each constructor, with `instanceof`, is another, and is not tried.
