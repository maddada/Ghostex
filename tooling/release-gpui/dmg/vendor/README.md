# Vendored for the DMG window layout

`../layout.py` writes the Finder layout of the Ghostex DMG (window size, icon places, background)
with these two pure-Python packages, the ones `dmgbuild` uses. They are copied here unchanged so the
macOS release needs no network for them.

| Folder | Package | Version | Licence | Source |
| --- | --- | --- | --- | --- |
| `ds_store/` | ds-store | 1.3.3 | MIT (`ds_store/LICENSE`) | https://pypi.org/project/ds-store/1.3.3/ |
| `mac_alias/` | mac-alias | 2.2.3 | MIT (`mac_alias/LICENSE`) | https://pypi.org/project/mac-alias/2.2.3/ |

They are build tooling only: nothing here ships inside Ghostex. To update, replace each folder with
the package's `src/<name>` folder from its sdist and keep its `LICENSE` beside it.
