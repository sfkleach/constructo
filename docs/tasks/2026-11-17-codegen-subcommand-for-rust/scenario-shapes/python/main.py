"""The program of the scenario: it reads a drawing, writes it back, and shows what is refused."""

from __future__ import annotations

import dataclasses
import sys
import typing
import uuid

import constructo_runtime as rt
import paint
import shapes
from shapes import Drawing

ID = 'ids::Uuid("67e55044-10b1-426f-9247-bb680e5fe0c8")'


def styled(item: str) -> str:
    return f"Styled {{ item: {item}, colour: null, offset: {{ dx: 0, dy: 0 }} }}"


def main() -> None:
    codec = shapes.codec()
    with open(sys.argv[1], encoding="utf-8") as f:
        text = f.read()

    drawing = codec.read(Drawing, text)
    print(f"--- Read\n{drawing}")

    written = codec.write(Drawing, drawing)
    print(f"--- Written\n{written}")
    if len(sys.argv) > 2:
        with open(sys.argv[2], "w", encoding="utf-8") as f:
            f.write(written)

    assert codec.read(Drawing, written) == drawing
    print("--- Read back: the same")

    other = 'com.example.ids::Uuid("00000000-0000-0000-0000-000000000001")'
    ids = codec.read(list[uuid.UUID], f"[{ID}, {other}]")
    print(f"--- A list of an adopted type\n{ids}")

    print("--- Refused when read")
    refused = [
        ("a constructor of another type", f'Drawing({ID}, "T", [{styled("Point(1.0, 2.0)")}])'),
        ("a name that is not in the run", f'Drawing({ID}, "T", [{styled("Hexagon")}])'),
        ("a bare name that the project does not have", 'Drawing(Uuid("67e55044-10b1-426f-9247-bb680e5fe0c8"), "T")'),
        ("a string that is not a UUID", 'Drawing(ids::Uuid("not-a-uuid"), "T")'),
        ("the id left out", 'Drawing(title: "T")'),
        ("a Float that does not fit float", f'Drawing({ID}, "T", [{styled("Circle { radius: 1.0e400 }")}])'),
        ("an unknown const", f'Drawing({ID}, "T", [{styled("unit_square")}])'),
        ("a missing field", f'Drawing({ID}, "T", [Styled {{ item: Dot, colour: null, offset: {{ dx: 0 }} }}])'),
        ("a syntax error", f'Drawing({ID}, "T"'),
    ]
    for label, text in refused:
        try:
            print(f"{label}: ACCEPTED {codec.read(Drawing, text)}")
        except rt.CodecError as error:
            print(f"{label}: {error}")

    big = codec.read(Drawing, f'Drawing({ID}, "T", [], 18446744073709551616000)')
    print(f"--- A Nat larger than 64 bits is read exactly: {big.layer}")
    assert codec.read(Drawing, codec.write(Drawing, big)).layer == big.layer

    print("--- Asked for a type that the codec does not have")
    for label, target in [("int", int), ("Styled without T", shapes.Styled), ("dict", dict)]:
        try:
            codec.read(target, "0")
        except rt.CodecError as error:
            print(f"{label}: {error}")

    print("--- Option<Option<Nat>> cannot be named in Python")
    print(f"Nat | None | None == Nat | None: {(rt.Nat | None | None) == (rt.Nat | None)}")

    huge = dataclasses.replace(drawing, shapes=[dataclasses.replace(s) for s in drawing.shapes])
    huge.shapes[0].item = shapes.Circle(1e300)
    huge.shapes[1].item = shapes.Circle(1e-7)
    assert codec.read(Drawing, codec.write(Drawing, huge)) == huge
    print("--- Large and small floats written and read back")

    print("--- Refused when written")
    bad: list[tuple[str, typing.Callable[[Drawing], None]]] = [
        ("a custom colour", lambda d: setattr(d.shapes[0], "colour", paint.Colour(0x336699))),
        ("a bool for a Nat", lambda d: setattr(d, "layer", True)),
        ("a negative Nat", lambda d: setattr(d, "layer", -1)),
        ("an int for a String", lambda d: setattr(d, "title", 5)),
        ("a NaN", lambda d: setattr(d.shapes[1], "item", shapes.Circle(float("nan")))),
        ("a str for a Shape", lambda d: setattr(d.shapes[1], "item", "Dot")),
    ]
    for label, change in bad:
        changed = codec.read(Drawing, written)
        change(changed)
        try:
            print(f"{label}: WRITTEN {codec.write(Drawing, changed)}")
        except rt.CodecError as error:
            print(f"{label}: {error}")


if __name__ == "__main__":
    main()
