#!/usr/bin/env python3
"""Regenerate the Google Play listing graphics.

Phone screenshots are an Android-styled composition of the real app UI already
captured for the iOS App Store listing. This environment has no Android
emulator paired to a Ghostex computer, so the pixels inside the phone are
those captures with the iOS status bar and home-indicator band cropped off,
then framed with a drawn Android status bar and gesture bar. They are not
photographs of the Android app.

Outputs (24-bit PNG, no alpha, except the 512 icon which is 32-bit):

  graphics/icon-512.png            512 x 512
  graphics/feature-graphic.png     1024 x 500
  graphics/phone/NN-*.png          1080 x 1920

Usage, from the repository root or from this directory:

  python3 tooling/release-mobile/play-store/graphics/compose.py
  python3 tooling/release-mobile/play-store/graphics/compose.py --check

Requires Pillow (pip install pillow).
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

from PIL import Image, ImageCms, ImageDraw, ImageFilter, ImageFont

HERE = Path(__file__).resolve().parent
PLAY = HERE.parent
ROOT = PLAY.parents[2]
IOS_PHONE = ROOT / "tooling/release-mobile/app-store/screenshots/iphone-6.9"
CAPTIONS = ROOT / "tooling/release-mobile/app-store/screenshots/captions.json"
ICON_SOURCE = HERE / "source/icon.png"

PHONE_SIZE = (1080, 1920)
# The rounded phone bitmap inside each 1320x2868 iOS store image, from compose.py.
IOS_FRAME = (107, 426, 1213, 2831)
# Drop the iOS status bar (clock and icons end near y=98) and the home-indicator curve.
CROP_TOP = 108
CROP_BOTTOM = 28
STATUS_H = 56
NAV_H = 36

LIMITS = {
    "name": 30,
    "shortDescription": 80,
    "fullDescription": 4000,
    "releaseNotes": 500,
}

SRGB = ImageCms.ImageCmsProfile(ImageCms.createProfile("sRGB")).tobytes()


def font_path(weight: str) -> Path:
    candidates = [
        Path(f"/usr/share/fonts/truetype/macos/Inter-{weight}.ttf"),
        Path(f"/Library/Fonts/Inter-{weight}.ttf"),
        Path(f"/System/Library/Fonts/Supplemental/Inter-{weight}.ttf"),
    ]
    fallback = {
        "Regular": "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "Medium": "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "SemiBold": "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
        "Bold": "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
    }
    for path in candidates:
        if path.is_file():
            return path
    return Path(fallback[weight])


def font(weight: str, size: int) -> ImageFont.FreeTypeFont:
    return ImageFont.truetype(str(font_path(weight)), size)


def save_rgb(image: Image.Image, path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    rgb = image.convert("RGB")
    rgb.save(path, "PNG", optimize=True, compress_level=9, icc_profile=SRGB)


def save_rgba(image: Image.Image, path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    rgba = image.convert("RGBA")
    if rgba.getextrema()[3][0] == 0 and rgba.getextrema()[3][1] == 0:
        raise SystemExit(f"{path.name} has an empty alpha channel")
    rgba.save(path, "PNG", optimize=True, compress_level=9, icc_profile=SRGB)


def vertical_gradient(size: tuple[int, int], top: tuple[int, int, int], bottom: tuple[int, int, int]) -> Image.Image:
    width, height = size
    image = Image.new("RGB", size)
    draw = ImageDraw.Draw(image)
    for y in range(height):
        t = y / max(height - 1, 1)
        color = tuple(int(top[i] + (bottom[i] - top[i]) * t) for i in range(3))
        draw.line((0, y, width, y), fill=color)
    return image


def store_background(size: tuple[int, int]) -> Image.Image:
    """Same near-black field as the iOS store screenshots."""
    width, height = size
    image = Image.new("RGB", size)
    draw = ImageDraw.Draw(image)
    for y in range(height):
        t = y / height
        draw.line((0, y, width, y), fill=(int(18 + 5 * t), int(24 + 4 * t), int(25 + 3 * t)))
    return image


def tracked_text(draw: ImageDraw.ImageDraw, origin: tuple[int, int], text: str, face: ImageFont.FreeTypeFont,
                 fill: str, tracking: int) -> None:
    x, y = origin
    for character in text:
        draw.text((x, y), character, font=face, fill=fill)
        x += draw.textbbox((0, 0), character, font=face)[2] + tracking


def render_icon() -> Path:
    with Image.open(ICON_SOURCE) as opened:
        source = opened.convert("RGB")
    icon = source.resize((512, 512), Image.Resampling.LANCZOS).convert("RGBA")
    # Play's hi-res icon is 32-bit. The source tile is opaque, so the alpha is solid.
    output = HERE / "icon-512.png"
    save_rgba(icon, output)
    return output


def render_feature() -> Path:
    image = vertical_gradient((1024, 500), (43, 96, 190), (10, 36, 92))
    with Image.open(ICON_SOURCE) as opened:
        tile = opened.convert("RGB").resize((392, 392), Image.Resampling.LANCZOS)
    mask = Image.new("L", tile.size, 0)
    ImageDraw.Draw(mask).rounded_rectangle((0, 0, tile.width - 1, tile.height - 1), radius=86, fill=255)
    shadow = Image.new("RGBA", image.size, (0, 0, 0, 0))
    shadow_draw = ImageDraw.Draw(shadow)
    left, top = 48, 54
    shadow_draw.rounded_rectangle((left, top + 8, left + tile.width, top + tile.height + 8), radius=86, fill=(0, 0, 0, 90))
    shadow = shadow.filter(ImageFilter.GaussianBlur(16))
    image = Image.alpha_composite(image.convert("RGBA"), shadow).convert("RGB")
    image.paste(tile, (left, top), mask)

    draw = ImageDraw.Draw(image)
    label = font("Bold", 22)
    title = font("SemiBold", 64)
    line = font("Regular", 30)
    text_x = 480
    tracked_text(draw, (text_x, 132), "GHOSTEX", label, "#9ee8d6", 3)
    draw.text((text_x, 176), "AI coding", font=title, fill="#f4f7fb")
    draw.text((text_x, 252), "agents", font=title, fill="#f4f7fb")
    draw.text((text_x, 348), "Your agents, from your pocket.", font=line, fill="#d5e2f2")
    output = HERE / "feature-graphic.png"
    save_rgb(image, output)
    return output


def app_pixels(path: Path) -> Image.Image:
    with Image.open(path) as opened:
        image = opened.convert("RGB")
    if image.size != (1320, 2868):
        raise SystemExit(f"{path.name} is {image.size}, expected 1320x2868")
    framed = image.crop(IOS_FRAME)
    height = framed.height - CROP_TOP - CROP_BOTTOM
    return framed.crop((0, CROP_TOP, framed.width, CROP_TOP + height))


def chrome_color(content: Image.Image, top: bool) -> tuple[int, int, int]:
    y = 2 if top else content.height - 3
    pixels = [content.getpixel((x, y)) for x in range(20, content.width, 40)]
    pixels.sort()
    return pixels[len(pixels) // 2]


def draw_status(canvas: Image.Image, color: tuple[int, int, int]) -> None:
    draw = ImageDraw.Draw(canvas)
    draw.rectangle((0, 0, canvas.width, STATUS_H), fill=color)
    face = font("Medium", 22)
    draw.text((28, 14), "9:41", font=face, fill="#f2f2f2")
    right = canvas.width - 28
    # Battery.
    battery = (right - 34, 18, right, 36)
    draw.rounded_rectangle(battery, radius=3, outline="#f2f2f2", width=2)
    draw.rectangle((right, 23, right + 3, 31), fill="#f2f2f2")
    draw.rectangle((battery[0] + 3, 21, battery[0] + 22, 33), fill="#f2f2f2")
    # Wi-Fi: three arcs.
    cx = right - 62
    for radius, width in ((14, 3), (9, 3), (4, 3)):
        draw.arc((cx - radius, 34 - radius, cx + radius, 34 + radius), 225, 315, fill="#f2f2f2", width=width)
    # Signal bars.
    base = 38
    x = right - 108
    for index, level in enumerate((8, 13, 18, 22)):
        draw.rounded_rectangle((x + index * 8, base - level, x + index * 8 + 5, base), radius=1, fill="#f2f2f2")


def draw_nav(canvas: Image.Image, y: int, color: tuple[int, int, int]) -> None:
    draw = ImageDraw.Draw(canvas)
    draw.rectangle((0, y, canvas.width, canvas.height), fill=color)
    pill_w = 118
    left = (canvas.width - pill_w) // 2
    draw.rounded_rectangle((left, y + 16, left + pill_w, y + 22), radius=3, fill="#9a9a9a")


def frame_phone(content: Image.Image, width: int) -> Image.Image:
    scale = width / content.width
    height = round(content.height * scale)
    scaled = content.resize((width, height), Image.Resampling.LANCZOS)
    phone = Image.new("RGB", (width, STATUS_H + height + NAV_H), chrome_color(scaled, True))
    phone.paste(scaled, (0, STATUS_H))
    draw_status(phone, chrome_color(scaled, True))
    draw_nav(phone, STATUS_H + height, chrome_color(scaled, False))
    return phone


def wrap_headline(draw: ImageDraw.ImageDraw, text: str, face: ImageFont.FreeTypeFont, limit: int) -> list[str]:
    if draw.textbbox((0, 0), text, font=face)[2] <= limit:
        return [text]
    words = text.split()
    pairs = [(" ".join(words[:split]), " ".join(words[split:])) for split in range(1, len(words))]
    return list(min(pairs, key=lambda pair: max(draw.textbbox((0, 0), line, font=face)[2] for line in pair)))


def render_phone(row: dict[str, str]) -> Path:
    content = app_pixels(IOS_PHONE / row["file"])
    canvas = store_background(PHONE_SIZE)
    draw = ImageDraw.Draw(canvas)
    left = 72
    label = font("Bold", 20)
    headline_face = font("SemiBold", 54)
    subline_face = font("Regular", 28)
    tracked_text(draw, (left, 48), "GHOSTEX", label, "#71ddc6", 2)
    lines = wrap_headline(draw, row["headline"], headline_face, PHONE_SIZE[0] - 2 * left)
    y = 92
    for line in lines:
        draw.text((left, y), line, font=headline_face, fill="#f5f8f6")
        y += 64
    draw.text((left, y + 6), row["subline"], font=subline_face, fill="#acb9b6")

    top = y + 78
    max_height = PHONE_SIZE[1] - top - 36
    # Width follows the height budget so the tall UI stays inside 9:16.
    width_budget = PHONE_SIZE[0] - 120
    phone_width = min(width_budget, round(content.width * (max_height - STATUS_H - NAV_H) / content.height))
    phone = frame_phone(content, phone_width)
    if phone.height > max_height:
        raise SystemExit(f"{row['file']} phone is {phone.size}, taller than {max_height}")
    x = (PHONE_SIZE[0] - phone.width) // 2
    shadow = Image.new("RGBA", canvas.size, (0, 0, 0, 0))
    ImageDraw.Draw(shadow).rounded_rectangle(
        (x - 4, top - 4, x + phone.width + 4, top + phone.height + 4), radius=36, fill=(0, 0, 0, 100)
    )
    shadow = shadow.filter(ImageFilter.GaussianBlur(18))
    canvas = Image.alpha_composite(canvas.convert("RGBA"), shadow).convert("RGB")
    mask = Image.new("L", phone.size, 0)
    ImageDraw.Draw(mask).rounded_rectangle((0, 0, phone.width - 1, phone.height - 1), radius=32, fill=255)
    canvas.paste(phone, (x, top), mask)
    border = ImageDraw.Draw(canvas)
    border.rounded_rectangle((x - 1, top - 1, x + phone.width, top + phone.height), radius=33, outline="#495450", width=2)
    output = HERE / "phone" / row["file"]
    save_rgb(canvas, output)
    return output


def text_fields() -> dict[str, str]:
    listing = json.loads((PLAY / "listing.json").read_text())
    full = (PLAY / "description.en-US.txt").read_text().strip("\n")
    notes = (PLAY / "release-notes.en-US.txt").read_text().strip("\n")
    return {
        "name": listing["name"],
        "shortDescription": listing["shortDescription"],
        "fullDescription": full,
        "releaseNotes": notes,
    }


def check_text() -> list[str]:
    problems = []
    for name, value in text_fields().items():
        limit = LIMITS[name]
        if len(value) > limit:
            problems.append(f"{name} is {len(value)} characters; Play allows {limit}")
        if value != value.strip():
            problems.append(f"{name} has leading or trailing whitespace")
    return problems


def check_images() -> list[str]:
    problems = []
    expected = {
        HERE / "icon-512.png": (512, 512, "RGBA"),
        HERE / "feature-graphic.png": (1024, 500, "RGB"),
    }
    captions = json.loads(CAPTIONS.read_text())
    for row in captions["iphone-6.9"]:
        expected[HERE / "phone" / row["file"]] = (*PHONE_SIZE, "RGB")
    for path, (width, height, mode) in expected.items():
        if not path.is_file():
            problems.append(f"missing {path.relative_to(PLAY)}")
            continue
        with Image.open(path) as image:
            if image.size != (width, height):
                problems.append(f"{path.name} is {image.size}, expected {width}x{height}")
            if image.mode != mode:
                problems.append(f"{path.name} mode is {image.mode}, expected {mode}")
            if mode == "RGB" and "A" in image.mode:
                problems.append(f"{path.name} has an alpha channel")
            if path.stat().st_size > 8 * 1024 * 1024:
                problems.append(f"{path.name} is over Play's 8 MB screenshot limit")
    icon = HERE / "icon-512.png"
    if icon.is_file() and icon.stat().st_size > 1024 * 1024:
        problems.append("icon-512.png is over Play's 1024 KB icon limit")
    return problems


def report() -> None:
    print("Text fields (characters, Play limits in parentheses):")
    for name, value in text_fields().items():
        print(f"  {name}: {len(value)} ({LIMITS[name]})")
    print("Graphics:")
    for path in [HERE / "icon-512.png", HERE / "feature-graphic.png", *sorted((HERE / "phone").glob("*.png"))]:
        if not path.is_file():
            continue
        with Image.open(path) as image:
            print(f"  {path.relative_to(PLAY)}  {image.size[0]}x{image.size[1]}  {image.mode}  {path.stat().st_size} bytes")


def main() -> None:
    check_only = "--check" in sys.argv
    if not check_only:
        if not ICON_SOURCE.is_file():
            raise SystemExit(f"missing icon source {ICON_SOURCE}")
        if not IOS_PHONE.is_dir():
            raise SystemExit(f"missing iOS screenshots {IOS_PHONE}")
        render_icon()
        render_feature()
        captions = json.loads(CAPTIONS.read_text())
        for row in captions["iphone-6.9"]:
            render_phone(row)
    problems = check_text() + check_images()
    report()
    if problems:
        print("Problems:")
        for problem in problems:
            print(f"  {problem}")
        raise SystemExit(1)


if __name__ == "__main__":
    main()
