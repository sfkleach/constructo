"""The functions that the adoption file names. They are the program's own code. A function refuses a value by
raising ValueError."""

from __future__ import annotations

import uuid

import paint
import shapes


def uuid_from_string(text: str) -> uuid.UUID:
    try:
        return uuid.UUID(text)
    except ValueError as why:
        raise ValueError(f"{text!r} is not a UUID: {why}") from why


def uuid_to_string(id: uuid.UUID) -> str:
    return str(id)


_COLOURS = {shapes.Colour.Red: paint.RED, shapes.Colour.Green: paint.GREEN, shapes.Colour.Blue: paint.BLUE}


def colour_from_generated(colour: shapes.Colour) -> paint.Colour:
    return _COLOURS[colour]


def colour_to_generated(colour: paint.Colour) -> shapes.Colour:
    for generated, own in _COLOURS.items():
        if own == colour:
            return generated
    raise ValueError(f"The colour #{colour.rgb:06x} has no Constructo form")
