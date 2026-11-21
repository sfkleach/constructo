"""The program's own colours, which existed before Constructo was used. A colour is any RGB value, and only three of
them have names in the drawing format."""

from __future__ import annotations

import dataclasses


@dataclasses.dataclass(frozen=True)
class Colour:
    rgb: int


RED = Colour(0xFF0000)
GREEN = Colour(0x00FF00)
BLUE = Colour(0x0000FF)
