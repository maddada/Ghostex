#!/usr/bin/env python3
"""Write a macOS .icns from an AppIcon.appiconset of PNG files.

`iconutil` on current macOS reports "Invalid Iconset" for a complete, correctly
sized PNG set during `bun run start` packaging even though the same conversion
succeeds in isolation. Encode the ICNS container directly from the canonical
PNGs instead of asking iconutil to build a .iconset.
"""

from __future__ import annotations

import struct
import sys
from pathlib import Path

PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"

# Apple Icon Image format PNG-based types. Each source file is used for the
# point-size type and, where it is a @2x asset, the retina type.
ICON_ENTRIES: tuple[tuple[str, str, int], ...] = (
    ("icp4", "icon_16x16.png", 16),
    ("icp5", "icon_32x32.png", 32),
    ("icp6", "icon_32x32@2x.png", 64),
    ("ic07", "icon_128x128.png", 128),
    ("ic08", "icon_256x256.png", 256),
    ("ic09", "icon_512x512.png", 512),
    ("ic10", "icon_512x512@2x.png", 1024),
    ("ic11", "icon_16x16@2x.png", 32),
    ("ic12", "icon_32x32@2x.png", 64),
    ("ic13", "icon_128x128@2x.png", 256),
    ("ic14", "icon_256x256@2x.png", 512),
)


def png_size(data: bytes) -> tuple[int, int]:
    if data[:8] != PNG_SIGNATURE or len(data) < 24:
        raise ValueError("not a PNG")
    width, height = struct.unpack(">II", data[16:24])
    return width, height


def read_png(path: Path, expected_width: int) -> bytes:
    data = path.read_bytes()
    width, height = png_size(data)
    if width != expected_width or height != expected_width:
        raise ValueError(
            f"{path.name} is {width}x{height}; expected {expected_width}x{expected_width}"
        )
    return data


def write_icns(source_dir: Path, dest: Path) -> None:
    chunks: list[bytes] = []
    for ostype, filename, expected_width in ICON_ENTRIES:
        path = source_dir / filename
        if not path.is_file():
            raise FileNotFoundError(f"missing app icon asset: {path}")
        png = read_png(path, expected_width)
        chunks.append(ostype.encode("ascii") + struct.pack(">I", 8 + len(png)) + png)
    body = b"".join(chunks)
    dest.parent.mkdir(parents=True, exist_ok=True)
    dest.write_bytes(b"icns" + struct.pack(">I", 8 + len(body)) + body)


def main(argv: list[str]) -> int:
    if len(argv) != 3:
        print("usage: write-macos-icns.py <AppIcon.appiconset> <AppIcon.icns>", file=sys.stderr)
        return 2
    source_dir = Path(argv[1])
    dest = Path(argv[2])
    if not source_dir.is_dir():
        print(f"missing app icon asset set: {source_dir}", file=sys.stderr)
        return 1
    try:
        write_icns(source_dir, dest)
    except (OSError, ValueError) as error:
        print(error, file=sys.stderr)
        return 1
    if dest.stat().st_size <= 8:
        print(f"did not write a usable icns at {dest}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
