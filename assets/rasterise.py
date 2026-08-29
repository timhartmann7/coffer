#!/usr/bin/env python3
"""Turns a brand SVG into the PNG the application window needs.

The brand files are the source, and they hold nothing but a filled polygon and
rounded rectangles, so a full SVG renderer is not needed to read them. The
output is drawn at four times the size and scaled down, which is what gives the
rounded corners their edges.

    python3 assets/rasterise.py assets/brand/coffer-icon-dark-accent.svg \
        crates/vault-gui/icons/icon.png 1024
"""

import re
import sys

from PIL import Image, ImageDraw

SUPERSAMPLE = 4


def colour(value):
    value = value.lstrip("#")
    return tuple(int(value[index : index + 2], 16) for index in (0, 2, 4)) + (255,)


def render(svg, size):
    box = re.search(r'viewBox="0 0 ([\d.]+) ([\d.]+)"', svg)
    width = float(box.group(1))
    scale = size * SUPERSAMPLE / width

    canvas = Image.new("RGBA", (size * SUPERSAMPLE,) * 2, (0, 0, 0, 0))
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
                (float(x) * scale, float(y) * scale)
                for x, y in re.findall(r"[ML]\s*([\d.-]+)[ ,]([\d.-]+)", text)
            ]
            draw.polygon(points, fill=paint)
        else:
            def number(name):
                found = re.search(rf'{name}="([\d.-]+)"', text)
                return float(found.group(1)) if found else 0.0

            x, y = number("x") * scale, number("y") * scale
            draw.rounded_rectangle(
                (x, y, x + number("width") * scale, y + number("height") * scale),
                radius=number("rx") * scale,
                fill=paint,
            )

    return canvas.resize((size, size), Image.LANCZOS)


def main():
    source, target, size = sys.argv[1], sys.argv[2], int(sys.argv[3])
    render(open(source).read(), size).save(target)


if __name__ == "__main__":
    main()
