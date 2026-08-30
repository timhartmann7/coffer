#!/usr/bin/env python3
"""Draws a brand SVG as one square of the macOS application icon.

The brand files are the source, and they hold nothing but a filled polygon and
rounded rectangles, so a full SVG renderer is not needed to read them. The
output is drawn at four times the size and averaged down, which is what gives
the rounded corners their edges.

`assets/appicon.sh` calls this once for every size the icon set needs.
"""

import re
import sys

from PIL import Image, ImageDraw

SUPERSAMPLE = 4

# macOS masks nothing and scales an application icon to whatever size the file
# gives it, so the margin that keeps Coffer the same width as its neighbours in
# the Dock has to be in the file. Apple's own grid draws on the middle 824 of
# 1024, measured off the icons already on this machine.
GROUND = 824 / 1024


def colour(value):
    value = value.lstrip("#")
    return tuple(int(value[index : index + 2], 16) for index in (0, 2, 4)) + (255,)


def render(svg, size):
    box = re.search(r'viewBox="0 0 ([\d.]+) ([\d.]+)"', svg)
    width = float(box.group(1))

    side = size * SUPERSAMPLE
    scale = side * GROUND / width
    inset = side * (1 - GROUND) / 2

    canvas = Image.new("RGBA", (side,) * 2, (0, 0, 0, 0))
    draw = ImageDraw.Draw(canvas)

    for element in re.finditer(r"<(path|rect|g)\b[^>]*>", svg):
        text = element.group(0)
        fill = re.search(r'fill="([^"]+)"', text)
        if element.group(1) == "g":
            inherited = colour(fill.group(1)) if fill else (0, 0, 0, 255)
            continue

        paint = colour(fill.group(1)) if fill else inherited

        if element.group(1) == "path":
            points = [
                (float(x) * scale + inset, float(y) * scale + inset)
                for x, y in re.findall(r"[ML]\s*([\d.-]+)[ ,]([\d.-]+)", text)
            ]
            draw.polygon(points, fill=paint)
        else:
            def number(name):
                found = re.search(rf'{name}="([\d.-]+)"', text)
                return float(found.group(1)) if found else 0.0

            x = number("x") * scale + inset
            y = number("y") * scale + inset
            draw.rounded_rectangle(
                (x, y, x + number("width") * scale, y + number("height") * scale),
                radius=number("rx") * scale,
                fill=paint,
            )

    # An average over the supersampled square. A filter with a negative lobe
    # rings around the mark's dots and smears them together at the sizes the
    # Dock actually draws.
    return canvas.resize((size, size), Image.BOX)


def main():
    source, target, size = sys.argv[1], sys.argv[2], int(sys.argv[3])
    render(open(source).read(), size).save(target)


if __name__ == "__main__":
    main()
