# Theme preset terminal palettes

The terminal palettes of Ghostex's twenty theme presets (Settings > Interface > Presets), embedded into the desktop app by `apps/desktop/build.rs` next to Ghostty's own theme collection. Each file is named `Ghostex <preset>` so it never replaces a palette of the same name in that collection.

- `palettes/`: twelve files copied byte for byte from the theme projects' own Ghostty themes (renamed only), and eight written in Ghostty's theme format for Ghostex from the projects' own colour definitions, because those projects ship no Ghostty theme. Each of the eight starts with a comment naming its source and licence.
- `licenses/`: the full licence text of every project a palette comes from. The same notices are reproduced in `THIRD_PARTY_NOTICES.md` at the repository root, which the app packages ship.

To update a palette: replace its file (or edit an adapted file from the new upstream values), update the table below and the licence if it changed, and rebuild; the build re-reads this folder.

## Copied unchanged

| File | Source (repository, commit, path) | SHA-256 | Licence |
|---|---|---|---|
| `Ghostex Catppuccin Mocha` | catppuccin/ghostty `b0b03ccee7ae8f16b13bd4fdfe267616defdb2b7` `themes/catppuccin-mocha.conf` | `fcdc0f045f68d7a8797ff5f1d3a76788bc17dd53b467c05b7bd65bd14a01331e` | MIT, `licenses/catppuccin-ghostty` |
| `Ghostex Catppuccin Macchiato` | catppuccin/ghostty `b0b03ccee7ae8f16b13bd4fdfe267616defdb2b7` `themes/catppuccin-macchiato.conf` | `b3c8cfbd5dc87250f69da03c52df66730c8f01e6983de05695993712d86c1421` | MIT, `licenses/catppuccin-ghostty` |
| `Ghostex Catppuccin Frappe` | catppuccin/ghostty `b0b03ccee7ae8f16b13bd4fdfe267616defdb2b7` `themes/catppuccin-frappe.conf` | `fb090c112aaac0cc07b456c88771ca7c53aae56adcd5b162da898def18f86574` | MIT, `licenses/catppuccin-ghostty` |
| `Ghostex Catppuccin Latte` | catppuccin/ghostty `b0b03ccee7ae8f16b13bd4fdfe267616defdb2b7` `themes/catppuccin-latte.conf` | `4be5760d759a90f007a70ca1819ae09ab1db0c849c462f3f3b44590f5110aef5` | MIT, `licenses/catppuccin-ghostty` |
| `Ghostex Dracula` | dracula/ghostty `b0e64590232331d9837c82cc1c5138e11ea39f76` `dracula` | `edc3e75c13daaeb3fa3ec837800a5172564cc091a125ac7a3d7c2941424a1be5` | MIT, `licenses/dracula-ghostty` |
| `Ghostex Tokyo Night Night` | folke/tokyonight.nvim `cdc07ac78467a233fd62c493de29a17e0cf2b2b6` `extras/ghostty/tokyonight_night` | `24ba9d48aa598db047005d3c3cd12ff6ea12bc87a539bdc5c3a84f404652a696` | Apache-2.0, `licenses/folke-tokyonight.nvim` |
| `Ghostex Tokyo Night Storm` | folke/tokyonight.nvim `cdc07ac78467a233fd62c493de29a17e0cf2b2b6` `extras/ghostty/tokyonight_storm` | `0573784079bae89ffa0f20b5dc36882ae8edd46eea0c13e1f733f6769f03c173` | Apache-2.0, `licenses/folke-tokyonight.nvim` |
| `Ghostex Tokyo Night Moon` | folke/tokyonight.nvim `cdc07ac78467a233fd62c493de29a17e0cf2b2b6` `extras/ghostty/tokyonight_moon` | `1e1b774c34aecfc662d42b5fa2b351a7076671c0bbda469b9bc808e2695559bf` | Apache-2.0, `licenses/folke-tokyonight.nvim` |
| `Ghostex Tokyo Night Day` | folke/tokyonight.nvim `cdc07ac78467a233fd62c493de29a17e0cf2b2b6` `extras/ghostty/tokyonight_day` | `299cbfbe4442e5ee4ff868862b3d55af85fcc85a0f4ca350365a98834b956083` | Apache-2.0, `licenses/folke-tokyonight.nvim` |
| `Ghostex Kanagawa Wave` | rebelot/kanagawa.nvim `bb85e4bfc8d89b0e62c8fa53ccdd13d12e2f77b3` `extras/ghostty/kanagawa-wave` | `f52da716fc190df8929184b9d545dab0d464036f7d0f189d5327f915a7174708` | MIT, `licenses/rebelot-kanagawa.nvim` |
| `Ghostex Kanagawa Dragon` | rebelot/kanagawa.nvim `bb85e4bfc8d89b0e62c8fa53ccdd13d12e2f77b3` `extras/ghostty/kanagawa-dragon` | `85d55962236a061b06fa8ca58490288563f25c1e006b86c11c6ab461cf584db8` | MIT, `licenses/rebelot-kanagawa.nvim` |
| `Ghostex Kanagawa Lotus` | rebelot/kanagawa.nvim `bb85e4bfc8d89b0e62c8fa53ccdd13d12e2f77b3` `extras/ghostty/kanagawa-lotus` | `522d21eabb45e6fc9a04463ce2ae6b7908f3107f627b8c2f53a055fb722b0d47` | MIT, `licenses/rebelot-kanagawa.nvim` |

## Written for Ghostex from the project's colour definitions

| File | Source | Licence |
|---|---|---|
| `Ghostex Alucard` | dracula/cursor 21e4edd687bdd5f38e1607c69f19f30cbde22c36 dev/src/alucard.yml (background, foreground, cursor, ANSI 0-15); selection background is the Alucard "Selection" colour from dracula/dracula-theme 8d33e5492b56740c4857c5ebe7c5d5ab4040caf1 README.md, because the terminal's own selection is translucent. | MIT, `licenses/dracula-cursor`, `licenses/dracula-dracula-theme` |
| `Ghostex Solarized Dark` | altercation/solarized 62f656a02f93c5190a8753159e34b385588d5ff3 README.md colour table (sRGB values and 16-colour slots); background, foreground, cursor and selection are the slots the project's iterm2-colors-solarized/Solarized Dark.itermcolors assigns to those roles. | MIT, `licenses/altercation-solarized` |
| `Ghostex Solarized Light` | altercation/solarized 62f656a02f93c5190a8753159e34b385588d5ff3 README.md colour table (sRGB values and 16-colour slots); background, foreground, cursor and selection are the slots the project's iterm2-colors-solarized/Solarized Light.itermcolors assigns to those roles. | MIT, `licenses/altercation-solarized` |
| `Ghostex Nord` | nordtheme/alacritty 9949642f3903e8fcb62bfc03f09410e3d78440c2 src/nord.yaml. | MIT, `licenses/nordtheme-alacritty` |
| `Ghostex Everforest Dark` | sainnhe/everforest 85a86eb62409e3ec88713bff3d1b9d7374e112e4, the terminal colours colors/everforest.vim sets with its default options (background medium), Normal text and Visual selection; cursor is the text colour. | MIT, `licenses/sainnhe-everforest` |
| `Ghostex Everforest Light` | sainnhe/everforest 85a86eb62409e3ec88713bff3d1b9d7374e112e4, the terminal colours colors/everforest.vim sets with its default options (background medium), Normal text and Visual selection; cursor is the text colour. | MIT, `licenses/sainnhe-everforest` |
| `Ghostex Gruvbox Material Dark` | sainnhe/gruvbox-material 11d779b26a9ab2b3db8c22c6ac9fb6e8ed4fea79, the terminal colours colors/gruvbox-material.vim sets with its default options (background medium, foreground material, grey visual background), Normal text and Visual selection; cursor is the text colour. | MIT, `licenses/sainnhe-gruvbox-material` |
| `Ghostex Gruvbox Material Light` | sainnhe/gruvbox-material 11d779b26a9ab2b3db8c22c6ac9fb6e8ed4fea79, the terminal colours colors/gruvbox-material.vim sets with its default options (background medium, foreground material, grey visual background), Normal text and Visual selection; cursor is the text colour. | MIT, `licenses/sainnhe-gruvbox-material` |

Two values are choices rather than literal terminal colours, because the projects define none: Alucard's selection background uses the theme's own Selection colour (its terminal selection is a translucent purple), and the Everforest and Gruvbox Material cursors use the text colour (their default draws the cursor in reverse video). The adapted files carry only background, foreground, cursor, selection background and the sixteen ANSI colours; other optional terminal roles are left unset.
