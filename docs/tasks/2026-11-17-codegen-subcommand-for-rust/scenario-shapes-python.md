# Scenario: the drawing program in Python

This is the thought experiment of the [Rust scenario](scenario-shapes.md) done
again for Python, with the same projects, the same adoptions and the same data,
to find what the design takes for granted about Rust before any of it is
implemented ([the task](codegen-subcommand-for-rust.md), "Steps").

As for Rust, the generated code is written by hand, as `codegen --lang=python`
would write it, and it runs. It is in [scenario-shapes/python](scenario-shapes/python/),
with the program that uses it. It applies the decisions made since the Rust
scenario: the full name as the key, a name written bare, else through a group,
else in full, writing that can fail with a path in the value, and so on.

What it writes is accepted by `check`. It is the same text as the Rust spike
writes, except that the id is written `ids::Uuid` and not
`com.example.ids::Uuid`, which is the decision about groups.

## What the generated code looks like

The types ([shapes.py](scenario-shapes/python/shapes.py)):

```python
@dataclasses.dataclass
class Circle:
    radius: float

@dataclasses.dataclass
class Dot:
    """The constructor `Dot` of `Shape`, which has no body."""

@dataclasses.dataclass
class Polygon:
    items: list[Point]

type Shape = Circle | Box | Dot | Polygon

@dataclasses.dataclass
class Point:                      # one constructor of the same name, so one class
    x: float
    y: float

class Colour(enum.Enum):          # adopted whole as paint.Colour; the step in between
    Red = "Red"
    Green = "Green"
    Blue = "Blue"

@dataclasses.dataclass
class Styled[T]:
    item: T
    colour: paint.Colour | None
    offset: dx_dy

@dataclasses.dataclass
class dx_dy:
    dx: _rt.Int
    dy: _rt.Int

@dataclasses.dataclass
class Drawing:
    id: uuid.UUID
    title: str
    shapes: list[Styled[Shape]]
    layer: _rt.Nat
```

`_rt.Nat` and `_rt.Int` are `typing.Annotated[int, ...]`: a type checker sees an
`int`, and the codec sees which of the two is meant.

The program uses the codec, and names the type both to read and to write:

```python
codec = shapes.codec()
drawing = codec.read(Drawing, text)
text = codec.write(Drawing, drawing)
ids = codec.read(list[uuid.UUID], "[ids::Uuid(...), ...]")
```

Reading is a `match` on the full names, as in Rust:

```python
def _read_shape(v: _rt.Value) -> Shape:
    match v:
        case _rt.Tagged("com.example.shapes::Circle", "record", members):
            [radius] = _rt.fields(members, ["radius"], "Circle")
            with _rt.at("radius"):
                return Circle(_read_float(radius))
        case _rt.Tagged("com.example.shapes::Dot", None):
            return Dot()
        ...
    raise _rt.expected("Shape", v)
```

The type that the caller names is turned into a reader by a table of the types of
the codec, which is the closed set: `codec.read(dict, ...)` is refused, and
`Styled` without its `T` is refused. A generic type is read by a reader that is
given the reader of its parameter, `_read_styled(_read_shape)`.

The stand-in runtime ([constructo_runtime.py](scenario-shapes/python/constructo_runtime.py))
has a tree of its own and the helpers that need no generated type, as the Rust
one does. It parses and prints by running the `constructo` binary with the
json-exact notation, which keeps every kind of value but not the positions in
the text, so an error of reading says where it is by a path in the value.

## What happens when it runs

The drawing is read, written and read back the same, and `check` accepts what is
written. The refusals:

| The text or the value has              | The error                                                                  |
|----------------------------------------|----------------------------------------------------------------------------|
| `Point(1.0, 2.0)` where a `Shape` is   | `shapes[0].item: Expected Shape but found Point`                           |
| `Hexagon`, which is not in the run     | `[2][0].item: Unknown constructor Hexagon`                                 |
| `ids::Uuid("not-a-uuid")`              | `id: 'not-a-uuid' is not a UUID: ...`, from the adopted function           |
| `Drawing(title: "T")`, with no id      | `id: Argument id of Drawing is left out, and its type has no default: ...` |
| a `Float` of `1.0e400`                 | `shapes[0].item.radius: The number 1.0e400 does not fit Float`             |
| `{ dx: 0 }` for `{ dx: Int, dy: Int }` | `shapes[0].offset: Missing field dy in record`                             |
| a syntax error                         | `Expected comma or ')' (<stdin>, line 1, col 63)`, from the parser         |
| the target `int`                       | `int is a Nat or an Int: name ...Nat or ...Int`                            |
| the target `Styled`, with no `T`       | `Styled needs a type for T, as in Styled[Shape]`                           |
| the target `dict`                      | `<class 'dict'> is not a type of the codec of ...`                         |
| `paint.Colour(0x336699)`, written      | `shapes[0].colour: The colour #336699 has no Constructo form`              |
| `layer = True`, written                | `layer: Expected a Nat but found bool True`                                |
| `layer = -1`, written                  | `layer: Expected a Nat but found int -1`                                   |
| `title = 5`, written                   | `title: Expected a String but found int 5`                                 |
| a NaN, written                         | `shapes[1].item.radius: The Float nan cannot be written`                   |
| `"Dot"` as a shape, written            | `shapes[1].item: Expected a Shape but found str 'Dot'`                     |

A `Nat` of `18446744073709551616000`, which Rust refuses, is read and written
exactly.

## What carries over from Rust unchanged

- The closed set, as a `match` on full names and a table of the types of the
  codec. Nothing is built from a name in the data, and the caller can only name
  a type that the codec has.
- The codec of a project, with its tables of names and consts, and the full name
  as the key between the codec and the readers.
- Adoption of a constructor and of a whole enum, with two functions, and the
  generated enum as the step in between.
- An enum with one constructor as one class, and the names of records without a
  name (`dx_dy`).
- Defaults by the rules of decision 18, with an adopted type's default through
  its function.
- Writing that can fail, with a path in the value. In Python it fails far more
  often, since nothing but the writer checks the types of the values that a
  program builds.

## What is different for Python alone

These are the generator's for Python, and do not change the common design.

1. **Numbers are exact.** `Nat` and `Int` are `int`, which has no limit, and
   `Float` is `float`. As was said when `u64` was chosen for Rust, the choice is
   made for each language.
2. **Python's `int` is both `Nat` and `Int`,** and `bool` is a kind of `int`. A
   type checker cannot tell them apart, so the codec does: the fields are
   annotated `Nat` or `Int`, which are `int` to a type checker, the target `int`
   on its own is refused, and the writer refuses a `bool` for a number.
3. **Writing needs the type,** `codec.write(Drawing, drawing)`, because a value
   does not say whether its `int` is a `Nat` or an `Int`, and so whether to write
   `5` or `+5`. In Rust the type is known to the compiler.
4. **A constructor with no body is a class with no fields,** `Dot()`, since a
   `Shape` is a union of classes and Python has no variant without data. An enum
   whose constructors all have no body is an `enum.Enum`. This is the opposite
   of the Rust decision (finding 3), and each is what its language would write.
5. **An adopted function refuses by raising `ValueError`,** which the codec turns
   into an error with a path, and does not return a result. It is named as
   `module:name` in the adoption file.
6. **The adopted functions are imported when they are first called.** The
   module of the adoptions imports the generated module, for `shapes.Colour`, and
   the generated module calls the adoptions, so importing them at the top would
   be a cycle.
7. **There is no orphan rule**, and no traits: the codec's tables of readers and
   writers do what the traits do in Rust.
8. **Python has no raw names.** A field that is a keyword, such as `from`, gets a
   `_` at the end, which is what the Rust rule does for the names that cannot be
   raw. A record type named by a field that is a builtin, such as `list`, would
   hide it, so the generated code should name the builtins in full,
   `builtins.list`, as the Rust code names the std items in full. The spike does
   not do this.
9. **Python's typing cannot name a type expression as a parameter.** The
   codec takes the type to read or write as a value, and a class or a generic
   alias such as `list[uuid.UUID]` is a `type[X]`, so `codec.read(Drawing, text)`
   is typed `Drawing` and `codec.read(list[uuid.UUID], text)` is typed
   `list[UUID]`. A special form is not: `Shape`, which is a `type` alias, and
   `X | None` give `Any`. The codec has overloads for the two, and a run of mypy
   shows the cost: `codec.write(Drawing, 5)` passes, since the second overload
   takes anything. PEP 747 (`TypeForm`) would close this when it is settled.
10. **A type checker sees a `Nat` as an `int`.** mypy reads `_rt.Nat` as `int`,
   as intended, so `Drawing(..., layer=-1)` and `layer=True` pass it, and only
   the writer refuses them, when the value is written. A Rust `u64` refuses the
   first when the program is compiled.
11. **Generic classes need Python 3.12,** for `class Styled[T]` and `type Shape =
   ...`, whose parameters are local to the class. With the older `TypeVar`, a
   parameter `T` would be a name of the module, and could clash with an enum
   called `T`.

## What changes the common design

These need a decision before the implementation, since they change what is
common to every target, or what the draft says of all of them.

1. **The runtime is not one parser for every language.** The draft says the
   runtime crate is the code that `check` uses, so that there is one parser to
   audit. That is true for Rust only. Python needs a parser of its own, written
   in Python and tested against the examples in `syntax/examples`, or a binding
   to the Rust one, as a native module that has to be built for each platform.
   The spike's way, running the binary, is a stand-in and not a choice.
   *Decided since:* each target has a runtime of its own, held to one syntax by
   a conformance suite whose expected results are Constructo text, and a native
   runtime can be added later. Every runtime also makes the tree of a value
   public, for reading and writing Constructo without types.
2. **The output of a target is not always one file.** Rust has modules inside a
   file, so a run's projects are one file on the standard output. Python has a
   module for each file, so the projects of a run are a package, a folder, or
   their names are joined into one module. The command needs a way to write a
   folder, such as `--out=FOLDER`, for the targets that need one.
   *Decided since:* `--out-file=FILE_PATH` for a target that writes one file,
   such as Rust, and `--out-dir=FOLDER` for one that writes a package, such as
   Python. The wrong one is a usage error that names the right one, and the
   folder belongs to the generator, guarded by a marker file.
3. **`Option<Option<T>>` cannot be named in Python.** `Nat | None | None` is
   `Nat | None`, so `Some(None)` and `None` would be the same value. A declared
   type such as `Nat??` must be refused for Python when the code is generated,
   or have a `Some` class of its own, and a generic type whose parameter is an
   `Option`, which only the caller knows, must refuse `Some(None)` when it is
   read, as typed JSON does (decision 37). The model of step 2 has to let a
   target see where an `Option` is inside another.
   *Decided since:* the faithful mapping is `Some[X] | None`, with a `Some` box
   in the runtime, and `X | None` is used only where no value of `X` can be
   `None`: the basic types, arrays, records, generated classes and enums, and
   adopted types, whose functions the codec forbids to return `None`. An
   `Option` inside an `Option`, and an `Option` of a type parameter, are boxed.
   The spike has no type that needs the box, and it does not yet refuse a
   `None` from an adopted function.
   *Decided since:* names and consts are resolved as the value is read and
   written, and the readers and writers are the codec's methods, so `self` has
   the names, the consts and the limits. The spike still has a pass.
5. **An adopted function refuses by raising `ValueError`,** which the codec turns
   into an error with a path, and does not return a result. It is named as
   `module:name` in the adoption file.
6. **The adopted functions are imported when they are first called.** The
   module of the adoptions imports the generated module, for `shapes.Colour`, and
   the generated module calls the adoptions, so importing them at the top would
   be a cycle.
7. **There is no orphan rule**, and no traits: the codec's tables of readers and
   writers do what the traits do in Rust.
8. **Python has no raw names.** A field that is a keyword, such as `from`, gets a
   `_` at the end, which is what the Rust rule does for the names that cannot be
   raw. A record type named by a field that is a builtin, such as `list`, would
   hide it, so the generated code should name the builtins in full,
   `builtins.list`, as the Rust code names the std items in full. The spike does
   not do this.
9. **Python's typing cannot name a type expression as a parameter.** The
   codec takes the type to read or write as a value, and a class or a generic
   alias such as `list[uuid.UUID]` is a `type[X]`, so `codec.read(Drawing, text)`
   is typed `Drawing` and `codec.read(list[uuid.UUID], text)` is typed
   `list[UUID]`. A special form is not: `Shape`, which is a `type` alias, and
   `X | None` give `Any`. The codec has overloads for the two, and a run of mypy
   shows the cost: `codec.write(Drawing, 5)` passes, since the second overload
   takes anything. PEP 747 (`TypeForm`) would close this when it is settled.
10. **A type checker sees a `Nat` as an `int`.** mypy reads `_rt.Nat` as `int`,
   as intended, so `Drawing(..., layer=-1)` and `layer=True` pass it, and only
   the writer refuses them, when the value is written. A Rust `u64` refuses the
   first when the program is compiled.
11. **Generic classes need Python 3.12,** for `class Styled[T]` and `type Shape =
   ...`, whose parameters are local to the class. With the older `TypeVar`, a
   parameter `T` would be a name of the module, and could clash with an enum
   called `T`.

## What changes the common design

These need a decision before the implementation, since they change what is
common to every target, or what the draft says of all of them.

1. **The runtime is not one parser for every language.** The draft says the
   runtime crate is the code that `check` uses, so that there is one parser to
   audit. That is true for Rust only. Python needs a parser of its own, written
   in Python and tested against the examples in `syntax/examples`, or a binding
   to the Rust one, as a native module that has to be built for each platform.
   The spike's way, running the binary, is a stand-in and not a choice.
   *Decided since:* each target has a runtime of its own, held to one syntax by
   a conformance suite whose expected results are Constructo text, and a native
   runtime can be added later. Every runtime also makes the tree of a value
   public, for reading and writing Constructo without types.
2. **The output of a target is not always one file.** Rust has modules inside a
   file, so a run's projects are one file on the standard output. Python has a
   module for each file, so the projects of a run are a package, a folder, or
   their names are joined into one module. The command needs a way to write a
   folder, such as `--out=FOLDER`, for the targets that need one.
   *Decided since:* `--out-file=FILE_PATH` for a target that writes one file,
   such as Rust, and `--out-dir=FOLDER` for one that writes a package, such as
   Python. The wrong one is a usage error that names the right one, and the
   folder belongs to the generator, guarded by a marker file.
3. **`Option<Option<T>>` cannot be named in Python.** `Nat | None | None` is
   `Nat | None`, so `Some(None)` and `None` would be the same value. A declared
   type such as `Nat??` must be refused for Python when the code is generated,
   or have a `Some` class of its own, and a generic type whose parameter is an
   `Option`, which only the caller knows, must refuse `Some(None)` when it is
   read, as typed JSON does (decision 37). The model of step 2 has to let a
   target see where an `Option` is inside another.
   *Decided since:* the faithful mapping is `Some[X] | None`, with a `Some` box
   in the runtime, and `X | None` is used only where no value of `X` can be
   `None`: the basic types, arrays, records, generated classes and enums, and
   adopted types, whose functions the codec forbids to return `None`. An
   `Option` inside an `Option`, and an `Option` of a type parameter, are boxed.
   The spike has no type that needs the box, and it does not yet refuse a
   `None` from an adopted function.
4. **Names should be resolved as the value is read,** and not in a pass of their
   own. The pass does not know the names of the parameters, so its errors say
   `[2][0].item` where the reader's say `shapes[0].item`. It is the Rust
   scenario's finding 8, and Python makes it plain.
5. **Defaults could be the defaults of the class.** Decision 17 puts unnamed
   parameters before named ones, which is Python's rule for the fields of a
   dataclass that have defaults, so `Point(x=4.0)` could take the default of `y`
   as reading does. The default of an adopted type can fail, so it is not always
   possible. It is a choice for each target, and not done in the spike.
   *Decided since:* a named parameter has its default in the class, from the
   function that the codec uses, so the two cannot differ; a default is `None`
   only for an `Option`; and a default that fails raises where it is left out.
   The spike does this for `Point` and `Drawing`, and checks that a value built
   with arguments left out equals the value read from the text without them.

## How to run it

From the folder of the shapes project, with the binary built and Python 3.12:

```
cd docs/tasks/2026-11-17-codegen-subcommand-for-rust/scenario-shapes/projects/shapes
CONSTRUCTO=../../../../../../_build/target/debug/constructo PYTHONPATH=../../python PYTHONDONTWRITEBYTECODE=1 \
    python3 ../../python/main.py values/drawing.values ../../../../../../_build/scenario/py-written.values
```

To check the types, from the `python` folder, with `uv`, which installs mypy from
the `pyproject.toml` (the environment and the cache are kept under `_build/`):

```
UV_PROJECT_ENVIRONMENT=../../../../../_build/scenario/venv uv run --frozen mypy
```

It reports no issues, in strict mode. `uv.lock` is uv's file, and its lines are
longer than the house style allows.

## What is not real

- There is no `codegen --lang=python`. The generated module is written by hand.
- The runtime runs the binary to parse and to print, so it has no positions in
  the text except for a syntax error, and a record cannot have a field twice.
- Whether the fields of a dataclass should be frozen, and arrays tuples, so that
  values can be hashed, is not considered. The spike uses lists.
