"""A stand-in for a Python `constructo-runtime`: the syntax layer, and helpers that need no generated type.

The real one would parse and print Constructo text itself. This one asks the `constructo` binary to do it, through
the json-exact notation, which keeps every kind of value but not the positions in the text. So an error of reading
says where it is by a path in the value, as an error of writing does.
"""

from __future__ import annotations

import contextlib
import dataclasses
import json
import os
import subprocess
import typing

BINARY = os.environ.get("CONSTRUCTO", "constructo")


# The types that Python does not have: a Nat and an Int are both `int` to Python, and to a type checker, but the
# codec needs to know which is meant, since `+5` is an Int and not a Nat.


class _Marker:
    def __init__(self, name: str) -> None:
        self.name = name

    def __repr__(self) -> str:
        return self.name


NAT = _Marker("Nat")
INT = _Marker("Int")
Nat = typing.Annotated[int, NAT]
Int = typing.Annotated[int, INT]


# Errors.


class CodecError(Exception):
    """An error of reading or writing, with the path in the value where it is."""

    def __init__(self, message: str) -> None:
        super().__init__(message)
        self.message = message
        self.path: list[str | int] = []

    def __str__(self) -> str:
        if not self.path:
            return self.message
        text = ""
        for step in self.path:
            text += f"[{step}]" if isinstance(step, int) else (f".{step}" if text else step)
        return f"{text}: {self.message}"


class ReadError(CodecError):
    pass


class WriteError(CodecError):
    pass


@contextlib.contextmanager
def at(step: str | int) -> typing.Iterator[None]:
    """Adds a step to the path of an error that passes out of the block."""
    try:
        yield
    except CodecError as error:
        error.path.insert(0, step)
        raise


# The tree of a value.


@dataclasses.dataclass
class Number:
    kind: str  # "nat", "int" or "float"
    text: str


@dataclasses.dataclass
class String:
    text: str


@dataclasses.dataclass
class Array:
    items: list[Value]


@dataclasses.dataclass
class Record:
    members: list[tuple[str, Value]]


@dataclasses.dataclass
class Tagged:
    """A constructor, or a reference to a const, which has no body. The body is "record", "tuple", "array" or None,
    and the contents are the members, the arguments as (name or None, value), or the items."""

    name: str
    body: str | None = None
    contents: list[typing.Any] = dataclasses.field(default_factory=list)


Value = Number | String | Array | Record | Tagged


def is_const_name(name: str) -> bool:
    local = name.rsplit("::", 1)[-1]
    return local[:1].islower() or local[:1] == "_"


def bare(name: str) -> str:
    return name.rsplit("::", 1)[-1]


def found(value: Value) -> str:
    match value:
        case Number(text=text):
            return f"the number {text}"
        case String():
            return "a string"
        case Array():
            return "an array"
        case Record():
            return "a record"
        case Tagged(name=name):
            return bare(name)


def expected(what: str, value: Value) -> ReadError:
    return ReadError(f"Expected {what} but found {found(value)}")


def takes(name: str, body: str) -> ReadError:
    return ReadError(f"Constructor {bare(name)} takes {body}")


def fields(members: list[tuple[str, Value]], names: list[str], owner: str) -> list[Value]:
    """The fields of a record, in the order of `names`. Every field is required, and no other is allowed."""
    slots: dict[str, Value] = {}
    for name, value in members:
        if name not in names:
            raise ReadError(f"Unknown field {name} in {bare(owner)}")
        if name in slots:
            raise ReadError(f"Duplicate field {name}")
        slots[name] = value
    for name in names:
        if name not in slots:
            raise ReadError(f"Missing field {name} in {bare(owner)}")
    return [slots[name] for name in names]


def arguments(args: list[tuple[str | None, Value]], params: list[str | None], owner: str) -> list[Value | None]:
    """The arguments of a tuple constructor, in the order of its parameters. Positional arguments bind a prefix, and
    named ones follow. A parameter with a name may be left out, and is then None; one without a name may not."""
    slots: list[Value | None] = [None] * len(params)
    count = f"Constructor {bare(owner)} takes {len(params)} value(s) but has {len(args)}"
    position = 0
    for name, value in args:
        if name is None:
            i = position
            position += 1
        elif name in params:
            i = params.index(name)
        else:
            raise ReadError(f"Unknown argument {name} in {bare(owner)}")
        if i >= len(params):
            raise ReadError(count)
        if slots[i] is not None:
            raise ReadError(f"Argument {params[i]} is given more than once in {bare(owner)}")
        slots[i] = value
    if any(param is None and slot is None for param, slot in zip(params, slots)):
        raise ReadError(count)
    return slots


def rewrite(value: Value, f: typing.Callable[[str], str | Value]) -> Value:
    """A copy of a value with each name, of a constructor or a const, rewritten. If `f` gives a value and not a
    name, the value takes the place of the reference."""
    match value:
        case Array(items):
            return Array([_step(i, item, f) for i, item in enumerate(items)])
        case Record(members):
            return Record([(name, _step(name, item, f)) for name, item in members])
        case Tagged(name, body, contents):
            new = f(name)
            if not isinstance(new, str):
                return new
            if body in ("tuple", "record"):
                params = [n for n, _ in contents]
                contents = [(n, _step(n if n is not None else params[:i].count(None), item, f))
                            for i, (n, item) in enumerate(contents)]
            elif body == "array":
                contents = [_step(i, item, f) for i, item in enumerate(contents)]
            return Tagged(new, body, contents)
        case _:
            return value


def _step(step: str | int, value: Value, f: typing.Callable[[str], str | Value]) -> Value:
    with at(step):
        return rewrite(value, f)


# Text, through the binary.


def _run(args: list[str], text: str) -> str:
    done = subprocess.run([BINARY, *args, "--stdin"], input=text, capture_output=True, text=True)
    if done.returncode != 0:
        raise ReadError(done.stderr.strip().removeprefix("ERROR: "))
    return done.stdout


def parse(text: str) -> list[Value]:
    return [_from_exact(item) for item in json.loads(_run(["convert", "--to=json-exact", "--to-array"], text))]


def parse_one(text: str) -> Value:
    values = parse(text)
    if len(values) != 1:
        raise ReadError(f"Expected one value but found {len(values)}")
    return values[0]


def render(value: Value) -> str:
    return _run(["convert", "--from=json-exact"], json.dumps(_to_exact(value)))


def _from_exact(j: dict[str, typing.Any]) -> Value:
    for kind in ("nat", "int", "float"):
        if kind in j:
            return Number(kind, j[kind])
    if "string" in j:
        return String(j["string"])
    if "const" in j:
        return Tagged(j["const"])
    if "constructor" in j:
        if "record" in j:
            return Tagged(j["constructor"], "record", [(k, _from_exact(v)) for k, v in j["record"].items()])
        if "array" in j:
            return Tagged(j["constructor"], "array", [_from_exact(v) for v in j["array"]])
        if "tuple" in j:
            args = [(None, _from_exact(v)) for v in j["tuple"]]
            args += [(k, _from_exact(v)) for k, v in j.get("named", {}).items()]
            return Tagged(j["constructor"], "tuple", args)
        return Tagged(j["constructor"])
    if "array" in j:
        return Array([_from_exact(v) for v in j["array"]])
    if "record" in j:
        return Record([(k, _from_exact(v)) for k, v in j["record"].items()])
    raise ReadError(f"Not a value: {j}")


def _to_exact(value: Value) -> dict[str, typing.Any]:
    match value:
        case Number(kind, text):
            return {kind: text}
        case String(text):
            return {"string": text}
        case Array(items):
            return {"array": [_to_exact(v) for v in items]}
        case Record(members):
            return {"record": {k: _to_exact(v) for k, v in members}}
        case Tagged(name, None, _):
            return {"const": name} if is_const_name(name) else {"constructor": name}
        case Tagged(name, "record", members):
            return {"constructor": name, "record": {k: _to_exact(v) for k, v in members}}
        case Tagged(name, "array", items):
            return {"constructor": name, "array": [_to_exact(v) for v in items]}
        case Tagged(name, "tuple", args):
            out: dict[str, typing.Any] = {"constructor": name, "tuple": [_to_exact(v) for n, v in args if n is None]}
            named = {n: _to_exact(v) for n, v in args if n is not None}
            if named:
                out["named"] = named
            return out
    raise WriteError(f"Not a value: {value!r}")


# Builders, for writing.


def unit(name: str) -> Tagged:
    return Tagged(name)


def tagged_record(name: str, members: list[tuple[str, Value]]) -> Tagged:
    return Tagged(name, "record", members)


def tagged_tuple(name: str, args: list[tuple[str | None, Value]]) -> Tagged:
    return Tagged(name, "tuple", args)


def tagged_array(name: str, items: list[Value]) -> Tagged:
    return Tagged(name, "array", items)


def type_name(t: typing.Any) -> str:
    """The qualified name of a class, so that an adopted type can be known without importing its module."""
    return f"{getattr(t, '__module__', '')}.{getattr(t, '__qualname__', repr(t))}"
