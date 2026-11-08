#!/usr/bin/env python3
"""Regenerate the Google Play listing graphics.

Phone screenshots combine the shared React Native app UI captured for the iOS
App Store listing with real Pixel emulator system chrome and Gboard pixels.
The app content is not a new Android capture. OS-only regions are replaced.

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
STATUS_SOURCE = HERE / "source/pixel-status.png"
NAV_SOURCE = HERE / "source/pixel-gesture-nav.png"
KEYBOARD_SOURCES = {
    "04-terminal.png": HERE / "source/pixel-gboard-enter.png",
    "07-find.png": HERE / "source/pixel-gboard-search.png",
}

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
    mac_fallback = {
        "Regular": "/Library/Fonts/SF-Compact-Display-Regular.otf",
        "Medium": "/Library/Fonts/SF-Compact-Display-Medium.otf",
        "SemiBold": "/Library/Fonts/SF-Compact-Display-Semibold.otf",
        "Bold": "/Library/Fonts/SF-Compact-Display-Bold.otf",
    }
    for path in candidates:
        if path.is_file():
            return path
    if Path(mac_fallback[weight]).is_file():
        return Path(mac_fallback[weight])
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
    content = framed.crop((0, CROP_TOP, framed.width, CROP_TOP + height))
    # The iPhone Dynamic Island can extend just below the cropped status band.
    # This region is blank app background on every source screen.
    draw = ImageDraw.Draw(content)
    draw.rectangle((0, 0, content.width, 35), fill=content.getpixel((20, 40)))
    if path.name == "07-find.png":
        # The source Find header has an iOS back-button capsule. Android uses
        # a plain back arrow beside the same screen title.
        draw.rectangle((0, 0, content.width, 174), fill="#000000")
        draw.line(((104, 75), (76, 102), (104, 129)), fill="#f1f1f1", width=6, joint="curve")
        draw.line(((78, 102), (130, 102)), fill="#f1f1f1", width=6)
        draw.text((192, 69), "Find Prompts", font=font("SemiBold", 43), fill="#f1f1f1")
    elif path.name == "06-docs.png":
        # The iOS stack header uses a circular chevron control here.
        draw.rectangle((38, 38, 170, 168), fill=content.getpixel((190, 100)))
        draw.line(((108, 76), (80, 104), (108, 132)), fill="#f1f1f1", width=6, joint="curve")
        draw.line(((82, 104), (136, 104)), fill="#f1f1f1", width=6)
    return content


def chrome_color(content: Image.Image, top: bool) -> tuple[int, int, int]:
    y = 2 if top else content.height - 3
    pixels = [content.getpixel((x, y)) for x in range(20, content.width, 40)]
    pixels.sort()
    return pixels[len(pixels) // 2]


def frame_phone(content: Image.Image, width: int, file_name: str) -> Image.Image:
    scale = width / content.width
    height = round(content.height * scale)
    scaled = content.resize((width, height), Image.Resampling.LANCZOS)
    phone = Image.new("RGB", (width, STATUS_H + height + NAV_H), chrome_color(scaled, True))
    phone.paste(scaled, (0, STATUS_H))
    with Image.open(STATUS_SOURCE) as opened:
        status = opened.convert("RGB").resize((width, STATUS_H), Image.Resampling.LANCZOS)
    phone.paste(status, (0, 0))
    with Image.open(NAV_SOURCE) as opened:
        nav = opened.convert("RGB").resize((width, NAV_H), Image.Resampling.LANCZOS)
    phone.paste(nav, (0, STATUS_H + height))
    if keyboard_source := KEYBOARD_SOURCES.get(file_name):
        with Image.open(keyboard_source) as opened:
            keyboard = opened.convert("RGB")
        keyboard_height = round(keyboard.height * width / keyboard.width)
        keyboard = keyboard.resize((width, keyboard_height), Image.Resampling.LANCZOS)
        phone.paste(keyboard, (0, phone.height - keyboard_height))
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
    phone = frame_phone(content, phone_width, row["file"])
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
