"""Writes the Finder layout of the Ghostex DMG window into a mounted read-write image: the window
size, the icon size and places, and the drag-to-Applications background.

Usage: python3 layout.py <mounted volume>

The volume must already hold `Ghostex.app`, the `Applications` link and
`.background/background.tiff` (macos.sh stages them). Uses the `ds_store` and `mac_alias`
packages (the ones dmgbuild uses), vendored unchanged in `vendor/` so the release needs no network.
macos.sh runs it with `python3 -I` and treats any failure as "no background" (best effort).

CDXC:Onboarding 2026-11-11 DECISION:
User, on the first-run page (step 1, install), picked "Add the drag picture to the DMG", with "make it
a nice looking picture matching ghostex.dev". The window is 660 by 400 points with 88-point icons,
Ghostex at 180,200 and Applications at 480,200, the places the art (background.tiff, built from the
first-run art thread's dmg-with-instruction 1x and 2x PNGs) leaves empty for them.
"""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent / "vendor"))

from ds_store import DSStore  # noqa: E402
from mac_alias import Alias  # noqa: E402

WINDOW_WIDTH = 660
WINDOW_HEIGHT = 400
ICON_SIZE = 88
WINDOW_ORIGIN = (200, 160)
ICONS = {"Ghostex.app": (180, 200), "Applications": (480, 200)}


def main(volume: Path) -> None:
    background = volume / ".background" / "background.tiff"
    if not background.is_file():
        raise SystemExit(f"Missing DMG background: {background}")
    for name in ICONS:
        if not (volume / name).exists():
            raise SystemExit(f"Missing DMG item: {volume / name}")
    alias = Alias.for_file(str(background))
    x, y = WINDOW_ORIGIN
    with DSStore.open(str(volume / ".DS_Store"), "w+") as store:
        store["."]["vSrn"] = ("long", 1)
        store["."]["bwsp"] = {
            "ShowStatusBar": False,
            "ShowToolbar": False,
            "ShowTabView": False,
            "ShowPathbar": False,
            "ShowSidebar": False,
            "ContainerShowSidebar": False,
            "PreviewPaneVisibility": False,
            "SidebarWidth": 0,
            "WindowBounds": f"{{{{{x}, {y}}}, {{{WINDOW_WIDTH}, {WINDOW_HEIGHT}}}}}",
        }
        store["."]["icvp"] = {
            "viewOptionsVersion": 1,
            "backgroundType": 2,
            "backgroundImageAlias": alias.to_bytes(),
            "backgroundColorRed": 1.0,
            "backgroundColorGreen": 1.0,
            "backgroundColorBlue": 1.0,
            "gridOffsetX": 0.0,
            "gridOffsetY": 0.0,
            "gridSpacing": 100.0,
            "arrangeBy": "none",
            "showIconPreview": False,
            "showItemInfo": False,
            "labelOnBottom": True,
            "textSize": 13.0,
            "iconSize": float(ICON_SIZE),
            "scrollPositionX": 0.0,
            "scrollPositionY": 0.0,
        }
        store["."]["icvl"] = ("type", b"icnv")
        store["."]["vstl"] = ("type", b"icnv")
        for name, place in ICONS.items():
            store[name]["Iloc"] = place


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit("usage: layout.py <mounted volume>")
    main(Path(sys.argv[1]))
