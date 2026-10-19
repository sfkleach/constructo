#!/usr/bin/env python3
"""Combine the railroad diagrams in a tabatkins2html.py page into one standalone SVG.

Markdown viewers can show an SVG inline, but not an HTML page of several inline SVGs.
The diagrams are stacked vertically and the page's stylesheet is embedded.
"""

import argparse
import re
import sys
import xml.etree.ElementTree as ET

GAP = 24
MARGIN = 8
SVG_NS = "http://www.w3.org/2000/svg"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", required=True, help="The HTML page produced by tabatkins2html.py.")
    args = parser.parse_args()

    with open(args.input, encoding="utf-8") as f:
        page = f.read()

    style = re.search(r"<style[^>]*>(.*?)</style>", page, re.S)
    diagrams = re.findall(r"<svg\b.*?</svg>", page, re.S)
    if style is None or not diagrams:
        sys.exit(f"{args.input}: no stylesheet or diagrams found")

    background = re.search(r"background-color:\s*([^;]+);", style.group(1)).group(1)
    pieces = []
    y = MARGIN
    width = 0.0
    for diagram in diagrams:
        # Parsing each diagram checks that it is well-formed XML before it is embedded.
        root = ET.fromstring(diagram)
        w, h = float(root.get("width")), float(root.get("height"))
        pieces.append(re.sub(r"<svg\b", f'<svg x="{MARGIN}" y="{y}"', diagram, count=1))
        width = max(width, w)
        y += h + GAP

    total_width = width + 2 * MARGIN
    total_height = y - GAP + MARGIN
    size = f'width="{total_width}" height="{total_height}" viewBox="0 0 {total_width} {total_height}"'
    print(f'<svg xmlns="{SVG_NS}" {size}>')
    print(f"<style>{style.group(1)}</style>")
    print(f'<rect width="100%" height="100%" fill="{background}"/>')
    for piece in pieces:
        print(piece)
    print("</svg>")


if __name__ == "__main__":
    main()
