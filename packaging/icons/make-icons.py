#!/usr/bin/env python3
"""Build every platform's app icon from the owl artwork.

    python3 -m venv /tmp/iconenv && /tmp/iconenv/bin/pip install pillow numpy
    /tmp/iconenv/bin/python packaging/icons/make-icons.py

The source is white line art on a dark ground. Its luminance becomes an alpha
mask, so the owl can be redrawn at any size on a background of our own rather
than carrying the screenshot's flat grey with it.

macOS follows Apple's icon grid: an 824 px rounded square on a 1024 canvas
with a soft shadow, because macOS draws exactly what the file contains. The
Windows and Linux icons fill their canvas, as those platforms expect.
"""

from pathlib import Path

import numpy as np
from PIL import Image, ImageFilter

ROOT = Path(__file__).resolve().parents[2]
SOURCE = Path(__file__).with_name("owl-source.png")
MACOS = ROOT / "app/macos/Runner/Assets.xcassets/AppIcon.appiconset"
WINDOWS = ROOT / "app/windows/runner/resources/app_icon.ico"
LINUX = Path(__file__).with_name("crawlstat.png")

TOP = (52, 52, 58)  # graphite, lit from above
BOTTOM = (20, 20, 23)
EMBER = (255, 106, 20)  # the app's accent


def owl_mask(width: int, weight: int) -> Image.Image:
    """The owl's line work, `width` px wide, as a crisp alpha mask.

    `weight` thickens the lines (a max filter, in px at the output size) so
    they survive being shrunk to a dock or taskbar icon.
    """
    luma = np.asarray(Image.open(SOURCE).convert("L")).astype(float)
    ground = np.median(luma)
    alpha = np.clip((luma - ground) / (luma.max() - ground), 0, 1)
    ys, xs = np.where(alpha > 0.35)
    alpha = alpha[ys.min() - 2 : ys.max() + 3, xs.min() - 2 : xs.max() + 3]

    small = Image.fromarray((alpha * 255).astype(np.uint8))
    height = round(small.height * width / small.width)
    # Upscale well past the target, sharpen the edges there, then come back
    # down: a straight upscale of 1-2 px lines is visibly soft.
    big = small.resize((width * 4, height * 4), Image.LANCZOS)
    if weight > 0:
        big = big.filter(ImageFilter.MaxFilter(weight * 4 + 1))
    m = np.asarray(big).astype(float) / 255
    m = np.clip((m - 0.3) / 0.35, 0, 1)
    m = m * m * (3 - 2 * m)
    big = Image.fromarray((m * 255).astype(np.uint8))
    return big.resize((width, height), Image.LANCZOS)


def squircle(size: int, inset: int, radius_ratio: float = 0.225) -> Image.Image:
    """Apple's continuous-corner square, approximated by a superellipse."""
    scale = 4
    n = size * scale
    side = (size - 2 * inset) * scale
    y, x = np.mgrid[0:n, 0:n].astype(float)
    cx = cy = n / 2
    half = side / 2
    # A superellipse of degree 5 matches the continuous corner closely.
    d = (np.abs(x - cx) / half) ** 5 + (np.abs(y - cy) / half) ** 5
    mask = Image.fromarray(((d <= 1) * 255).astype(np.uint8))
    return mask.resize((size, size), Image.LANCZOS)


def vertical(height: int, top, bottom, width: int | None = None) -> Image.Image:
    t = np.linspace(0, 1, height)[:, None, None]
    rows = np.array(top) * (1 - t) + np.array(bottom) * t
    pixels = np.repeat(rows, width or height, axis=1).astype(np.uint8)
    return Image.fromarray(pixels, "RGB")


def glow(size: int, centre, radius: float, colour, strength: float) -> Image.Image:
    y, x = np.mgrid[0:size, 0:size].astype(float)
    d = np.hypot(x - centre[0], y - centre[1]) / radius
    a = np.clip(1 - d, 0, 1) ** 2 * strength
    layer = np.zeros((size, size, 4))
    layer[..., :3] = colour
    layer[..., 3] = a * 255
    return Image.fromarray(layer.astype(np.uint8), "RGBA")


def icon(size: int, inset: int, owl_ratio: float, weight: int, shadow: bool) -> Image.Image:
    canvas = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    shape = squircle(size, inset)

    if shadow:
        blur = size * 0.028
        drop = Image.new("RGBA", (size, size), (0, 0, 0, 0))
        drop.putalpha(shape.point(lambda v: v * 0.45))
        drop = drop.transform(
            drop.size, Image.AFFINE, (1, 0, 0, 0, 1, -size * 0.012)
        ).filter(ImageFilter.GaussianBlur(blur))
        canvas.alpha_composite(drop)

    body = vertical(size, TOP, BOTTOM).convert("RGBA")
    # A faint ember behind the head ties the icon to the app's accent.
    body.alpha_composite(glow(size, (size / 2, size * 0.47), size * 0.42, EMBER, 0.16))

    side = size - 2 * inset
    owl = owl_mask(round(side * owl_ratio), weight)
    ox = (size - owl.width) // 2
    oy = round(size * 0.5 - owl.height * 0.54)
    ink = vertical(owl.height, (255, 255, 255), (214, 214, 222), owl.width).convert("RGBA")
    ink.putalpha(owl)
    # A soft halo lifts the line work off the dark face.
    halo = Image.new("RGBA", ink.size, (255, 255, 255, 0))
    halo.putalpha(owl.filter(ImageFilter.GaussianBlur(max(1, size * 0.006))).point(lambda v: v * 0.25))
    body.alpha_composite(halo, (ox, oy))
    body.alpha_composite(ink, (ox, oy))

    # Hairline highlight along the top edge, the way macOS icons catch light.
    rim = Image.new("RGBA", (size, size), (255, 255, 255, 0))
    inner = squircle(size, inset + max(1, size // 512))
    edge = np.asarray(shape).astype(float) - np.asarray(inner).astype(float)
    fade = np.linspace(0.5, 0, size)[:, None]
    rim.putalpha(Image.fromarray(np.clip(edge * fade, 0, 255).astype(np.uint8)))
    body.alpha_composite(rim)

    body.putalpha(shape)
    canvas.alpha_composite(body)
    return canvas


def main() -> None:
    # macOS: one master per size class, so small sizes get bolder lines
    # instead of a faithful but unreadable shrink of the 1024 artwork.
    masters = {
        1024: icon(1024, 100, 0.80, 2, True),
        256: icon(1024, 100, 0.82, 4, True),
        64: icon(1024, 100, 0.86, 8, True),
    }
    for px in (16, 32, 64, 128, 256, 512, 1024):
        master = masters[1024 if px >= 512 else 256 if px >= 128 else 64]
        master.resize((px, px), Image.LANCZOS).save(MACOS / f"app_icon_{px}.png")

    full = icon(1024, 24, 0.82, 4, False)
    full.save(
        WINDOWS,
        sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)],
    )
    full.resize((512, 512), Image.LANCZOS).save(LINUX)
    print("wrote", MACOS, WINDOWS, LINUX, sep="\n  ")


if __name__ == "__main__":
    main()
