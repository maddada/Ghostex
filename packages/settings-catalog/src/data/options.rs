use crate::json::{num_opt, opt, NumOpt, Opt};

/// `agentboxDefaultLocation` choices before any remote Docker server is registered.
pub const AGENTBOX_DEFAULT_LOCATION_OPTIONS: &[Opt] = &[
    opt("This computer", "local"),
    opt("Docker on this computer", "agentbox:docker"),
    opt("Hetzner", "agentbox:hetzner"),
    opt("Vercel", "agentbox:vercel"),
    opt("Daytona", "agentbox:daytona"),
    opt("E2B", "agentbox:e2b"),
    opt("DigitalOcean", "agentbox:digitalocean"),
];

pub const AUTO_SLEEP_IDLE_MINUTE_OPTIONS: &[NumOpt] = &[
    num_opt("Off", 0.0),
    num_opt("5 minutes", 5.0),
    num_opt("10 minutes", 10.0),
    num_opt("15 minutes", 15.0),
    num_opt("30 minutes", 30.0),
    num_opt("1 hour", 60.0),
    num_opt("2 hours", 120.0),
    num_opt("5 hours", 300.0),
];

pub const COMMANDS_PANEL_AUTO_MINIMIZE_DELAY_OPTIONS: &[NumOpt] = &[
    num_opt("15 seconds", 15.0),
    num_opt("30 seconds", 30.0),
    num_opt("1 minute", 60.0),
    num_opt("2 minutes", 120.0),
    num_opt("5 minutes", 300.0),
];

pub const COMMANDS_PANEL_SIDE_OPTIONS: &[Opt] = &[opt("Bottom", "bottom"), opt("Right", "right")];

pub const DEFAULT_EDITOR_COMMAND_OPTIONS: &[Opt] = &[
    opt("VS Code (code)", "code"),
    opt("VS Code Insiders (code-insiders)", "code-insiders"),
    opt("Zed (zed)", "zed"),
    opt("Zed alternate (zeditor)", "zeditor"),
    opt("Cursor (cursor)", "cursor"),
    opt("Windsurf (windsurf)", "windsurf"),
    opt("VSCodium (codium)", "codium"),
    opt("Sublime Text (subl)", "subl"),
    opt("Other", "other"),
];

pub const GHOSTTY_CONFIRM_CLOSE_SURFACE_OPTIONS: &[Opt] = &[
    opt("Smart confirmation", "true"),
    opt("Always confirm", "always"),
    opt("Do not confirm", "false"),
];

/// CDXC:Clipboard 2026-09-27 DECISION: User accepted Ghostty 1.4's macOS behaviour ("accept it and relabel the option on macOS"): Ghostty now reads the legacy `true` as `clipboard` on macOS, so "Selection clipboard" also copies to the system clipboard there. macOS shows the value by what it does; Linux and Windows keep the table above, where `true` still means the selection clipboard only.
pub const GHOSTTY_COPY_ON_SELECT_MAC_OPTIONS: &[Opt] = &[
    opt("Off", "false"),
    opt("Copy to clipboard", "true"),
    opt("System and selection clipboard", "clipboard"),
];

pub const GHOSTTY_COPY_ON_SELECT_OPTIONS: &[Opt] = &[
    opt("Off", "false"),
    opt("Selection clipboard", "true"),
    opt("System and selection clipboard", "clipboard"),
];

pub const GHOSTTY_SCROLLBAR_OPTIONS: &[Opt] = &[opt("System", "system"), opt("Never", "never")];

pub const GHOSTTY_THEME_SETTING_OPTIONS: &[Opt] = &[
    opt(
        "Use terminal config file",
        "__ghostex_ghostty_theme_unmanaged__",
    ),
    opt("0x96f", "0x96f"),
    opt("12-bit Rainbow", "12-bit Rainbow"),
    opt("3024 Day", "3024 Day"),
    opt("3024 Night", "3024 Night"),
    opt("Aardvark Blue", "Aardvark Blue"),
    opt("Abernathy", "Abernathy"),
    opt("Adventure", "Adventure"),
    opt("Adventure Time", "Adventure Time"),
    opt("Adwaita", "Adwaita"),
    opt("Adwaita Dark", "Adwaita Dark"),
    opt("Afterglow", "Afterglow"),
    opt("Aizen Dark", "Aizen Dark"),
    opt("Aizen Light", "Aizen Light"),
    opt("Alabaster", "Alabaster"),
    opt("Alien Blood", "Alien Blood"),
    opt("Andromeda", "Andromeda"),
    opt("Apple Classic", "Apple Classic"),
    opt("Apple System Colors", "Apple System Colors"),
    opt("Apple System Colors Light", "Apple System Colors Light"),
    opt("Arcoiris", "Arcoiris"),
    opt("Ardoise", "Ardoise"),
    opt("Argonaut", "Argonaut"),
    opt("Arthur", "Arthur"),
    opt("Atelier Sulphurpool", "Atelier Sulphurpool"),
    opt("Atom", "Atom"),
    opt("Atom One Dark", "Atom One Dark"),
    opt("Atom One Light", "Atom One Light"),
    opt("Aura", "Aura"),
    opt("Aurora", "Aurora"),
    opt("Ayu", "Ayu"),
    opt("Ayu Light", "Ayu Light"),
    opt("Ayu Mirage", "Ayu Mirage"),
    opt("Banana Blueberry", "Banana Blueberry"),
    opt("Batman", "Batman"),
    opt("Belafonte Day", "Belafonte Day"),
    opt("Belafonte Night", "Belafonte Night"),
    opt("Birds Of Paradise", "Birds Of Paradise"),
    opt("Black Metal", "Black Metal"),
    opt("Black Metal (Bathory)", "Black Metal (Bathory)"),
    opt("Black Metal (Burzum)", "Black Metal (Burzum)"),
    opt("Black Metal (Dark Funeral)", "Black Metal (Dark Funeral)"),
    opt("Black Metal (Gorgoroth)", "Black Metal (Gorgoroth)"),
    opt("Black Metal (Immortal)", "Black Metal (Immortal)"),
    opt("Black Metal (Khold)", "Black Metal (Khold)"),
    opt("Black Metal (Marduk)", "Black Metal (Marduk)"),
    opt("Black Metal (Mayhem)", "Black Metal (Mayhem)"),
    opt("Black Metal (Nile)", "Black Metal (Nile)"),
    opt("Black Metal (Venom)", "Black Metal (Venom)"),
    opt("Blazer", "Blazer"),
    opt("Blue Berry Pie", "Blue Berry Pie"),
    opt("Blue Dolphin", "Blue Dolphin"),
    opt("Blue Matrix", "Blue Matrix"),
    opt("Bluloco Dark", "Bluloco Dark"),
    opt("Bluloco Light", "Bluloco Light"),
    opt("Borland", "Borland"),
    opt("Box", "Box"),
    opt("Breadog", "Breadog"),
    opt("Breeze", "Breeze"),
    opt("Bright Lights", "Bright Lights"),
    opt("Broadcast", "Broadcast"),
    opt("Brogrammer", "Brogrammer"),
    opt("Builtin Dark", "Builtin Dark"),
    opt("Builtin Light", "Builtin Light"),
    opt("Builtin Pastel Dark", "Builtin Pastel Dark"),
    opt("Builtin Tango Dark", "Builtin Tango Dark"),
    opt("Builtin Tango Light", "Builtin Tango Light"),
    opt("C64", "C64"),
    opt("CGA", "CGA"),
    opt("CLRS", "CLRS"),
    opt("Calamity", "Calamity"),
    opt("Carbonfox", "Carbonfox"),
    opt("Catppuccin Frappe", "Catppuccin Frappe"),
    opt("Catppuccin Latte", "Catppuccin Latte"),
    opt("Catppuccin Macchiato", "Catppuccin Macchiato"),
    opt("Catppuccin Mocha", "Catppuccin Mocha"),
    opt("Chalk", "Chalk"),
    opt("Chalkboard", "Chalkboard"),
    opt("Challenger Deep", "Challenger Deep"),
    opt("Chester", "Chester"),
    opt("Ciapre", "Ciapre"),
    opt("Citruszest", "Citruszest"),
    opt("Cobalt Neon", "Cobalt Neon"),
    opt("Cobalt Next", "Cobalt Next"),
    opt("Cobalt Next Dark", "Cobalt Next Dark"),
    opt("Cobalt Next Minimal", "Cobalt Next Minimal"),
    opt("Cobalt2", "Cobalt2"),
    opt("Coffee Theme", "Coffee Theme"),
    opt("Crayon Pony Fish", "Crayon Pony Fish"),
    opt("Cursor Dark", "Cursor Dark"),
    opt("Cursor Light", "Cursor Light"),
    opt("Cutie Pro", "Cutie Pro"),
    opt("Cyberdyne", "Cyberdyne"),
    opt("Cyberpunk", "Cyberpunk"),
    opt("Cyberpunk Scarlet Protocol", "Cyberpunk Scarlet Protocol"),
    opt("Dalton Dark", "Dalton Dark"),
    opt("Dark Modern", "Dark Modern"),
    opt("Dark Pastel", "Dark Pastel"),
    opt("Dark+", "Dark+"),
    opt("Darkermatrix", "Darkermatrix"),
    opt("Darkmatrix", "Darkmatrix"),
    opt("Darkside", "Darkside"),
    opt("Dawnfox", "Dawnfox"),
    opt("Dayfox", "Dayfox"),
    opt("Deep", "Deep"),
    opt("Desert", "Desert"),
    opt("Detuned", "Detuned"),
    opt("Dimidium", "Dimidium"),
    opt("Dimmed Monokai", "Dimmed Monokai"),
    opt("Django", "Django"),
    opt("Django Reborn Again", "Django Reborn Again"),
    opt("Django Smooth", "Django Smooth"),
    opt("Dogxi Misty", "Dogxi Misty"),
    opt("Doom One", "Doom One"),
    opt("Doom Peacock", "Doom Peacock"),
    opt("Dot Gov", "Dot Gov"),
    opt("Dracula", "Dracula"),
    opt("Dracula+", "Dracula+"),
    opt("Duckbones", "Duckbones"),
    opt("Duotone Dark", "Duotone Dark"),
    opt("Duskfox", "Duskfox"),
    opt("ENCOM", "ENCOM"),
    opt("Earthsong", "Earthsong"),
    opt("Electron Highlighter", "Electron Highlighter"),
    opt("Elegant", "Elegant"),
    opt("Elemental", "Elemental"),
    opt("Elementary", "Elementary"),
    opt("Embark", "Embark"),
    opt("Embers Dark", "Embers Dark"),
    opt("Espresso", "Espresso"),
    opt("Espresso Libre", "Espresso Libre"),
    opt("Everblush", "Everblush"),
    opt("Everforest Dark Hard", "Everforest Dark Hard"),
    opt("Everforest Light Med", "Everforest Light Med"),
    opt("Fahrenheit", "Fahrenheit"),
    opt("Fairyfloss", "Fairyfloss"),
    opt("Farmhouse Dark", "Farmhouse Dark"),
    opt("Farmhouse Light", "Farmhouse Light"),
    opt("Fideloper", "Fideloper"),
    opt("Firefly Traditional", "Firefly Traditional"),
    opt("Firefox Dev", "Firefox Dev"),
    opt("Firewatch", "Firewatch"),
    opt("Fish Tank", "Fish Tank"),
    opt("Flat", "Flat"),
    opt("Flatland", "Flatland"),
    opt("Flexoki Dark", "Flexoki Dark"),
    opt("Flexoki Light", "Flexoki Light"),
    opt("Floraverse", "Floraverse"),
    opt("Forest Blue", "Forest Blue"),
    opt("Framer", "Framer"),
    opt("Front End Delight", "Front End Delight"),
    opt("Fun Forrest", "Fun Forrest"),
    opt("Galaxy", "Galaxy"),
    opt("Galizur", "Galizur"),
    opt("Ghostex Alucard", "Ghostex Alucard"),
    opt("Ghostex Catppuccin Frappé", "Ghostex Catppuccin Frappe"),
    opt("Ghostex Catppuccin Latte", "Ghostex Catppuccin Latte"),
    opt(
        "Ghostex Catppuccin Macchiato",
        "Ghostex Catppuccin Macchiato",
    ),
    opt("Ghostex Catppuccin Mocha", "Ghostex Catppuccin Mocha"),
    opt("Ghostex Dracula", "Ghostex Dracula"),
    opt("Ghostex Everforest Dark", "Ghostex Everforest Dark"),
    opt("Ghostex Everforest Light", "Ghostex Everforest Light"),
    opt(
        "Ghostex Gruvbox Material Dark",
        "Ghostex Gruvbox Material Dark",
    ),
    opt(
        "Ghostex Gruvbox Material Light",
        "Ghostex Gruvbox Material Light",
    ),
    opt("Ghostex Kanagawa Dragon", "Ghostex Kanagawa Dragon"),
    opt("Ghostex Kanagawa Lotus", "Ghostex Kanagawa Lotus"),
    opt("Ghostex Kanagawa Wave", "Ghostex Kanagawa Wave"),
    opt("Ghostex Nord", "Ghostex Nord"),
    opt("Ghostex Solarized Dark", "Ghostex Solarized Dark"),
    opt("Ghostex Solarized Light", "Ghostex Solarized Light"),
    opt("Ghostex Tokyo Night Day", "Ghostex Tokyo Night Day"),
    opt("Ghostex Tokyo Night Moon", "Ghostex Tokyo Night Moon"),
    opt("Ghostex Tokyo Night Night", "Ghostex Tokyo Night Night"),
    opt("Ghostex Tokyo Night Storm", "Ghostex Tokyo Night Storm"),
    opt("Ghostty Default Style Dark", "Ghostty Default Style Dark"),
    opt("GitHub", "GitHub"),
    opt("GitHub Dark", "GitHub Dark"),
    opt("GitHub Dark Colorblind", "GitHub Dark Colorblind"),
    opt("GitHub Dark Default", "GitHub Dark Default"),
    opt("GitHub Dark Dimmed", "GitHub Dark Dimmed"),
    opt("GitHub Dark High Contrast", "GitHub Dark High Contrast"),
    opt("GitHub Light Colorblind", "GitHub Light Colorblind"),
    opt("GitHub Light", "GitHub Light Default"),
    opt("GitHub Light High Contrast", "GitHub Light High Contrast"),
    opt("GitLab Dark", "GitLab Dark"),
    opt("GitLab Dark Grey", "GitLab Dark Grey"),
    opt("GitLab Light", "GitLab Light"),
    opt("Glacier", "Glacier"),
    opt("Grape", "Grape"),
    opt("Grass", "Grass"),
    opt("Grey Green", "Grey Green"),
    opt("Gruber Darker", "Gruber Darker"),
    opt("Gruvbox Dark", "Gruvbox Dark"),
    opt("Gruvbox Dark Hard", "Gruvbox Dark Hard"),
    opt("Gruvbox Light", "Gruvbox Light"),
    opt("Gruvbox Light Hard", "Gruvbox Light Hard"),
    opt("Gruvbox Material", "Gruvbox Material"),
    opt("Gruvbox Material Dark", "Gruvbox Material Dark"),
    opt("Gruvbox Material Light", "Gruvbox Material Light"),
    opt("Guezwhoz", "Guezwhoz"),
    opt("HaX0R Blue", "HaX0R Blue"),
    opt("HaX0R Gr33N", "HaX0R Gr33N"),
    opt("HaX0R R3D", "HaX0R R3D"),
    opt("Hacktober", "Hacktober"),
    opt("Hardcore", "Hardcore"),
    opt("Harper", "Harper"),
    opt("Havn Daggry", "Havn Daggry"),
    opt("Havn Skumring", "Havn Skumring"),
    opt("Heeler", "Heeler"),
    opt("Highway", "Highway"),
    opt("Hipster Green", "Hipster Green"),
    opt("Hivacruz", "Hivacruz"),
    opt("Homebrew", "Homebrew"),
    opt("Hopscotch", "Hopscotch"),
    opt("Hopscotch.256", "Hopscotch.256"),
    opt("Horizon", "Horizon"),
    opt("Horizon Bright", "Horizon Bright"),
    opt("Hot Dog Stand", "Hot Dog Stand"),
    opt("Hot Dog Stand (Mustard)", "Hot Dog Stand (Mustard)"),
    opt("Hurtado", "Hurtado"),
    opt("Hybrid", "Hybrid"),
    opt("IBM 5153 CGA", "IBM 5153 CGA"),
    opt("IBM 5153 CGA (Black)", "IBM 5153 CGA (Black)"),
    opt("IC Green PPL", "IC Green PPL"),
    opt("IC Orange PPL", "IC Orange PPL"),
    opt("IR Black", "IR Black"),
    opt("IRIX Console", "IRIX Console"),
    opt("IRIX Terminal", "IRIX Terminal"),
    opt("Iceberg Dark", "Iceberg Dark"),
    opt("Iceberg Light", "Iceberg Light"),
    opt("Idea", "Idea"),
    opt("Idle Toes", "Idle Toes"),
    opt("Jackie Brown", "Jackie Brown"),
    opt("Japanesque", "Japanesque"),
    opt("Jellybeans", "Jellybeans"),
    opt("JetBrains Darcula", "JetBrains Darcula"),
    opt("Jubi", "Jubi"),
    opt("Kanagawa Dragon", "Kanagawa Dragon"),
    opt("Kanagawa Lotus", "Kanagawa Lotus"),
    opt("Kanagawa Wave", "Kanagawa Wave"),
    opt("Kanagawabones", "Kanagawabones"),
    opt("Kanso Ink", "Kanso Ink"),
    opt("Kanso Mist", "Kanso Mist"),
    opt("Kanso Pearl", "Kanso Pearl"),
    opt("Kanso Zen", "Kanso Zen"),
    opt("Kibble", "Kibble"),
    opt("Kitty Default", "Kitty Default"),
    opt("Kitty Low Contrast", "Kitty Low Contrast"),
    opt("Kolorit", "Kolorit"),
    opt("Konsolas", "Konsolas"),
    opt("Kurokula", "Kurokula"),
    opt("Lab Fox", "Lab Fox"),
    opt("Laser", "Laser"),
    opt("Later This Evening", "Later This Evening"),
    opt("Lavandula", "Lavandula"),
    opt("Light Owl", "Light Owl"),
    opt("Liquid Carbon", "Liquid Carbon"),
    opt("Liquid Carbon Transparent", "Liquid Carbon Transparent"),
    opt("Lovelace", "Lovelace"),
    opt("Man Page", "Man Page"),
    opt("Mariana", "Mariana"),
    opt("Material", "Material"),
    opt("Material Dark", "Material Dark"),
    opt("Material Darker", "Material Darker"),
    opt("Material Design Colors", "Material Design Colors"),
    opt("Material Ocean", "Material Ocean"),
    opt("Mathias", "Mathias"),
    opt("Matrix", "Matrix"),
    opt("Matte Black", "Matte Black"),
    opt("Medallion", "Medallion"),
    opt("Melange Dark", "Melange Dark"),
    opt("Melange Light", "Melange Light"),
    opt("Mellifluous", "Mellifluous"),
    opt("Mellow", "Mellow"),
    opt("Miasma", "Miasma"),
    opt("Midnight In Mojave", "Midnight In Mojave"),
    opt("Mirage", "Mirage"),
    opt("Misterioso", "Misterioso"),
    opt("Modus Operandi", "Modus Operandi"),
    opt("Modus Operandi Tinted", "Modus Operandi Tinted"),
    opt("Modus Vivendi", "Modus Vivendi"),
    opt("Modus Vivendi Tinted", "Modus Vivendi Tinted"),
    opt("Molokai", "Molokai"),
    opt("Mona Lisa", "Mona Lisa"),
    opt("Monokai Classic", "Monokai Classic"),
    opt("Monokai Pro", "Monokai Pro"),
    opt("Monokai Pro Light", "Monokai Pro Light"),
    opt("Monokai Pro Light Sun", "Monokai Pro Light Sun"),
    opt("Monokai Pro Machine", "Monokai Pro Machine"),
    opt("Monokai Pro Octagon", "Monokai Pro Octagon"),
    opt("Monokai Pro Ristretto", "Monokai Pro Ristretto"),
    opt("Monokai Pro Spectrum", "Monokai Pro Spectrum"),
    opt("Monokai Remastered", "Monokai Remastered"),
    opt("Monokai Soda", "Monokai Soda"),
    opt("Monokai Vivid", "Monokai Vivid"),
    opt("Monospace Dark", "Monospace Dark"),
    opt("Monospace Light", "Monospace Light"),
    opt("Moonfly", "Moonfly"),
    opt("N0Tch2K", "N0Tch2K"),
    opt("Neobones Dark", "Neobones Dark"),
    opt("Neobones Light", "Neobones Light"),
    opt("Neon", "Neon"),
    opt("Neopolitan", "Neopolitan"),
    opt("Neutron", "Neutron"),
    opt("Night Lion V1", "Night Lion V1"),
    opt("Night Lion V2", "Night Lion V2"),
    opt("Night Owl", "Night Owl"),
    opt("Night Owlish Light", "Night Owlish Light"),
    opt("Nightfox", "Nightfox"),
    opt("Niji", "Niji"),
    opt("No Clown Fiesta", "No Clown Fiesta"),
    opt("No Clown Fiesta Light", "No Clown Fiesta Light"),
    opt("Nocturnal Winter", "Nocturnal Winter"),
    opt("Nord", "Nord"),
    opt("Nord Light", "Nord Light"),
    opt("Nord Wave", "Nord Wave"),
    opt("Nordfox", "Nordfox"),
    opt("Novel", "Novel"),
    opt("Nvim Dark", "Nvim Dark"),
    opt("Nvim Light", "Nvim Light"),
    opt("Obsidian", "Obsidian"),
    opt("Ocean", "Ocean"),
    opt("Oceanic Material", "Oceanic Material"),
    opt("Oceanic Next", "Oceanic Next"),
    opt("Ollie", "Ollie"),
    opt("One Dark Two", "One Dark Two"),
    opt("One Double Dark", "One Double Dark"),
    opt("One Double Light", "One Double Light"),
    opt("One Half Dark", "One Half Dark"),
    opt("One Half Light", "One Half Light"),
    opt("Onenord", "Onenord"),
    opt("Onenord Light", "Onenord Light"),
    opt("Operator Mono Dark", "Operator Mono Dark"),
    opt("Overnight Slumber", "Overnight Slumber"),
    opt("Oxocarbon", "Oxocarbon"),
    opt("Pale Night Hc", "Pale Night Hc"),
    opt("Pandora", "Pandora"),
    opt("Paraiso Dark", "Paraiso Dark"),
    opt("Paul Millr", "Paul Millr"),
    opt("Pencil Dark", "Pencil Dark"),
    opt("Pencil Light", "Pencil Light"),
    opt("Peppermint", "Peppermint"),
    opt("Phala Green Dark", "Phala Green Dark"),
    opt("Piatto Light", "Piatto Light"),
    opt("Pierre Dark", "Pierre Dark"),
    opt("Pierre Light", "Pierre Light"),
    opt("Pnevma", "Pnevma"),
    opt("Poimandres", "Poimandres"),
    opt("Poimandres Darker", "Poimandres Darker"),
    opt("Poimandres Storm", "Poimandres Storm"),
    opt("Poimandres White", "Poimandres White"),
    opt("Popping And Locking", "Popping And Locking"),
    opt("Powershell", "Powershell"),
    opt("Primary", "Primary"),
    opt("Pro", "Pro"),
    opt("Pro Light", "Pro Light"),
    opt("Purple Rain", "Purple Rain"),
    opt("Purplepeter", "Purplepeter"),
    opt("Rapture", "Rapture"),
    opt("Raycast Dark", "Raycast Dark"),
    opt("Raycast Light", "Raycast Light"),
    opt("Rebecca", "Rebecca"),
    opt("Red Alert", "Red Alert"),
    opt("Red Planet", "Red Planet"),
    opt("Red Sands", "Red Sands"),
    opt("Relaxed", "Relaxed"),
    opt("Retro", "Retro"),
    opt("Retro Legends", "Retro Legends"),
    opt("Rippedcasts", "Rippedcasts"),
    opt("Rose Pine", "Rose Pine"),
    opt("Rose Pine Dawn", "Rose Pine Dawn"),
    opt("Rose Pine Moon", "Rose Pine Moon"),
    opt("Rouge 2", "Rouge 2"),
    opt("Royal", "Royal"),
    opt("Ryuuko", "Ryuuko"),
    opt("Sakura", "Sakura"),
    opt("Scarlet Protocol", "Scarlet Protocol"),
    opt("Sea Shells", "Sea Shells"),
    opt("Seafoam Pastel", "Seafoam Pastel"),
    opt("SeedFlip Abyss", "SeedFlip Abyss"),
    opt("SeedFlip Amethyst", "SeedFlip Amethyst"),
    opt("SeedFlip Canopy", "SeedFlip Canopy"),
    opt("SeedFlip Carbon", "SeedFlip Carbon"),
    opt("SeedFlip Coral", "SeedFlip Coral"),
    opt("SeedFlip Ember", "SeedFlip Ember"),
    opt("SeedFlip Glacier", "SeedFlip Glacier"),
    opt("SeedFlip Inkwell", "SeedFlip Inkwell"),
    opt("SeedFlip Ivory", "SeedFlip Ivory"),
    opt("SeedFlip Nightfall", "SeedFlip Nightfall"),
    opt("SeedFlip Phosphor", "SeedFlip Phosphor"),
    opt("SeedFlip Pulse", "SeedFlip Pulse"),
    opt("SeedFlip Ultraviolet", "SeedFlip Ultraviolet"),
    opt("SeedFlip Voltage", "SeedFlip Voltage"),
    opt("SeedFlip Wavelength", "SeedFlip Wavelength"),
    opt("Selenized Black", "Selenized Black"),
    opt("Selenized Dark", "Selenized Dark"),
    opt("Selenized Light", "Selenized Light"),
    opt("Seoulbones Dark", "Seoulbones Dark"),
    opt("Seoulbones Light", "Seoulbones Light"),
    opt("Seti", "Seti"),
    opt("Shades Of Purple", "Shades Of Purple"),
    opt("Shaman", "Shaman"),
    opt("Slate", "Slate"),
    opt("Sleepy Hollow", "Sleepy Hollow"),
    opt("Smyck", "Smyck"),
    opt("Snazzy", "Snazzy"),
    opt("Snazzy Soft", "Snazzy Soft"),
    opt("Soft Server", "Soft Server"),
    opt("Solarized Darcula", "Solarized Darcula"),
    opt(
        "Solarized Dark Higher Contrast",
        "Solarized Dark Higher Contrast",
    ),
    opt("Solarized Dark Patched", "Solarized Dark Patched"),
    opt("Solarized Osaka Night", "Solarized Osaka Night"),
    opt("Sonokai", "Sonokai"),
    opt("Spacedust", "Spacedust"),
    opt("Spacegray", "Spacegray"),
    opt("Spacegray Bright", "Spacegray Bright"),
    opt("Spacegray Eighties", "Spacegray Eighties"),
    opt("Spacegray Eighties Dull", "Spacegray Eighties Dull"),
    opt("Spiderman", "Spiderman"),
    opt("Spring", "Spring"),
    opt("Square", "Square"),
    opt("Squirrelsong Dark", "Squirrelsong Dark"),
    opt("Srcery", "Srcery"),
    opt("Starlight", "Starlight"),
    opt("Sublette", "Sublette"),
    opt("Subliminal", "Subliminal"),
    opt("Sugarplum", "Sugarplum"),
    opt("Sundried", "Sundried"),
    opt("Sunset Drive", "Sunset Drive"),
    opt("Symfonic", "Symfonic"),
    opt("Synthwave", "Synthwave"),
    opt("Synthwave Alpha", "Synthwave Alpha"),
    opt("Synthwave Everything", "Synthwave Everything"),
    opt("Tango Adapted", "Tango Adapted"),
    opt("Tango Half Adapted", "Tango Half Adapted"),
    opt("Tearout", "Tearout"),
    opt("Teerb", "Teerb"),
    opt("Terafox", "Terafox"),
    opt("Terminal Basic", "Terminal Basic"),
    opt("Terminal Basic Dark", "Terminal Basic Dark"),
    opt("Thayer Bright", "Thayer Bright"),
    opt("The Hulk", "The Hulk"),
    opt("Tinacious Design Dark", "Tinacious Design Dark"),
    opt("Tinacious Design Light", "Tinacious Design Light"),
    opt("TokyoNight", "TokyoNight"),
    opt("TokyoNight Day", "TokyoNight Day"),
    opt("TokyoNight Moon", "TokyoNight Moon"),
    opt("TokyoNight Night", "TokyoNight Night"),
    opt("TokyoNight Storm", "TokyoNight Storm"),
    opt("Tomorrow", "Tomorrow"),
    opt("Tomorrow Night", "Tomorrow Night"),
    opt("Tomorrow Night Blue", "Tomorrow Night Blue"),
    opt("Tomorrow Night Bright", "Tomorrow Night Bright"),
    opt("Tomorrow Night Burns", "Tomorrow Night Burns"),
    opt("Tomorrow Night Eighties", "Tomorrow Night Eighties"),
    opt("Toy Chest", "Toy Chest"),
    opt("Treehouse", "Treehouse"),
    opt("Twilight", "Twilight"),
    opt("Ubuntu", "Ubuntu"),
    opt("Ultra Dark", "Ultra Dark"),
    opt("Ultra Violent", "Ultra Violent"),
    opt("Under The Sea", "Under The Sea"),
    opt("Unikitty", "Unikitty"),
    opt("Urple", "Urple"),
    opt("Vague", "Vague"),
    opt("Vaughn", "Vaughn"),
    opt("Vercel", "Vercel"),
    opt("Vesper", "Vesper"),
    opt("Vibrant Ink", "Vibrant Ink"),
    opt("Vimbones", "Vimbones"),
    opt("Violet Dark", "Violet Dark"),
    opt("Violet Light", "Violet Light"),
    opt("Violite", "Violite"),
    opt("Warm Neon", "Warm Neon"),
    opt("Wez", "Wez"),
    opt("Whimsy", "Whimsy"),
    opt("Wild Cherry", "Wild Cherry"),
    opt("Wilmersdorf", "Wilmersdorf"),
    opt("Wombat", "Wombat"),
    opt("Wryan", "Wryan"),
    opt("Xcode Dark", "Xcode Dark"),
    opt("Xcode Dark hc", "Xcode Dark hc"),
    opt("Xcode Light", "Xcode Light"),
    opt("Xcode Light hc", "Xcode Light hc"),
    opt("Xcode WWDC", "Xcode WWDC"),
    opt("Zenbones", "Zenbones"),
    opt("Zenbones Dark", "Zenbones Dark"),
    opt("Zenbones Light", "Zenbones Light"),
    opt("Zenburn", "Zenburn"),
    opt("Zenburned", "Zenburned"),
    opt("Zenwritten Dark", "Zenwritten Dark"),
    opt("Zenwritten Light", "Zenwritten Light"),
    opt("branch", "branch"),
    opt("hazyland", "hazyland"),
    opt("iTerm2 Dark Background", "iTerm2 Dark Background"),
    opt("iTerm2 Default", "iTerm2 Default"),
    opt("iTerm2 Light Background", "iTerm2 Light Background"),
    opt(
        "iTerm2 Pastel Dark Background",
        "iTerm2 Pastel Dark Background",
    ),
    opt("iTerm2 Smoooooth", "iTerm2 Smoooooth"),
    opt("iTerm2 Solarized Dark", "iTerm2 Solarized Dark"),
    opt("iTerm2 Solarized Light", "iTerm2 Solarized Light"),
    opt("iTerm2 Tango Dark", "iTerm2 Tango Dark"),
    opt("iTerm2 Tango Light", "iTerm2 Tango Light"),
    opt("novmbr", "novmbr"),
    opt("owl", "owl"),
    opt("traffic", "traffic"),
    opt("urban", "urban"),
];

pub const KEEP_AWAKE_DURATION_OPTIONS: &[NumOpt] = &[
    num_opt("Until turned off", 0.0),
    num_opt("2 hours", 120.0),
    num_opt("5 hours", 300.0),
];

/// Where Review all and Session details' Changes card open.
pub const FILE_CHANGES_OPEN_TARGET_OPTIONS: &[Opt] =
    &[opt("Changes view", "changes"), opt("VS Code", "code")];

pub const MEDIA_FILE_OPEN_TARGET_OPTIONS: &[Opt] =
    &[opt("Files", "files"), opt("System App", "system-app")];

pub const PANEL_ANIMATION_SPEED_OPTIONS: &[Opt] = &[
    opt("Off", "off"),
    opt("Slow", "slow"),
    opt("Normal", "normal"),
    opt("Fast", "fast"),
];

/// Select value used by the per-agent Default Agent View control for "no
/// override". It is never persisted: inherit is stored as an absent key in
/// `preferredAgentInterfaceOverrides`.
pub const PREFERRED_AGENT_INTERFACE_INHERIT_VALUE: &str = "inherit";

pub const PREFERRED_AGENT_INTERFACE_OPTIONS: &[Opt] =
    &[opt("Terminal", "terminal"), opt("Chat", "chat")];

/// `newSessionAgentScope`: how the agent a project's new-session button starts is remembered
/// (gx-core `sidebar_menu/agent_choice.rs`, CDXC:AgentLauncher 2026-10-30).
pub const NEW_SESSION_AGENT_SCOPE_OPTIONS: &[Opt] = &[
    opt("Per project", "project"),
    opt("Per Space", "space"),
    opt("Everywhere", "everywhere"),
];

pub const PROMPT_EDITOR_BACKEND_OPTIONS: &[Opt] = &[
    opt("Ghostex editor", "monaco"),
    opt("Use default from this machine", "inherit"),
];

pub const SESSION_CHAT_THEME_OPTIONS: &[Opt] = &[
    opt("Follow app", "app"),
    opt("System", "system"),
    opt("Light", "light"),
    opt("Dark", "dark"),
];

/// CDXC:SessionTitles 2026-09-11 WHY:
/// Pi and Antigravity CLI are title-generation agents because both ship a non-interactive print mode (`pi -p`, `agy -p`) that was verified against the real binaries, and a machine with only those two CLIs used to be stuck with the Codex default, which is not installed there (GitHub issue #125).
/// The values are sidebar agent ids, not executables, because settings and gxserver resolve the command through the sidebar agent registry.
/// SEE-ALSO: server/src/server/title_generation/title_command.rs (normalize_title_generation_agent, build_title_generation_command).
pub const SESSION_TITLE_GENERATION_AGENT_OPTIONS: &[Opt] = &[
    opt("Codex", "codex"),
    opt("Cursor CLI", "cursor"),
    opt("Claude", "claude"),
    opt("Grok Build", "grok"),
    opt("Pi Agent", "pi"),
    opt("Antigravity CLI", "antigravity"),
    opt("Empryo", "empryo"),
    opt("Custom", "custom"),
];

/// CDXC:Spaces 2026-09-11 DECISION:
/// User: the default, "Restore the Space's projects", is the first entry so the dropdown opens on it.
pub const SIDEBAR_SPACE_SWITCH_BEHAVIOR_OPTIONS: &[Opt] = &[
    opt("Restore the Space's projects", "restore"),
    opt("Don't switch projects", "keep"),
];

/// CDXC:Sidebar 2026-11-07 DECISION in gx-core sidebar_view/inputs.rs: Manual (the default, first)
/// keeps the dragged order; Most recently updated orders pinned sessions by when they came back.
pub const PINNED_SESSION_ORDER_OPTIONS: &[Opt] = &[
    opt("Manual (drag to rearrange)", "manual"),
    opt("Most recently updated", "recent"),
];

/// CDXC:Sidebar 2026-10-30 DECISION:
/// User: keep today's sidebar exactly as it is and call it "Project sidebar"; the new one is "Unified sidebar". They switch only in Settings › Sidebar, and the Project sidebar is the default.
pub const SIDEBAR_VIEW_OPTIONS: &[Opt] = &[
    opt("Project sidebar", "project"),
    opt("Unified sidebar", "unified"),
];

pub const SIDEBAR_THEME_SETTING_OPTIONS: &[Opt] = &[
    opt("Dark", "dark-2"),
    opt("Light", "plain-light"),
    opt("System", "system"),
];

pub const SIDEBAR_VISIBILITY_MEMORY_OPTIONS: &[Opt] = &[
    opt("Same in every view", "shared"),
    opt("Remembered per view", "perView"),
];

pub const TERMINAL_VIEW_WIDTH_MODE_OPTIONS: &[Opt] = &[
    opt("Full", "full"),
    opt("Match Chat", "match-chat"),
    opt("Custom", "custom"),
];

/// How the chat colours the sections an agent marks (Marked handovers), in the Settings dropdown's order.
pub const MARKED_HANDOVERS_STYLE_OPTIONS: &[Opt] = &[
    opt("Coloured labels", "labels"),
    opt("Thin left rule", "rule"),
    opt("Coloured badge", "badge"),
];

/// Where a page an agent published in chat (`ghostex show`) opens when its card is clicked, in the Settings control's order.
pub const CHAT_PAGE_OPEN_TARGET_OPTIONS: &[Opt] = &[
    opt("Popup", "popup"),
    opt("Files", "files"),
    opt("System browser", "system-browser"),
];

/// Which agent alerts play the finished sound and show a system notification (`attentionAlertsFor`), in the Settings control's order.
pub const ATTENTION_ALERTS_FOR_OPTIONS: &[Opt] =
    &[opt("All", "all"), opt("Needs you only", "needsYou")];

/// What a coordinator chat's Board pill opens (`coordinatorBoardView`), in the Settings control's order.
pub const COORDINATOR_BOARD_VIEW_OPTIONS: &[Opt] =
    &[opt("Side list", "list"), opt("Columns", "columns")];

/// Message layout (`chatMessageLayout`), in the Settings control's order.
pub const CHAT_MESSAGE_LAYOUT_OPTIONS: &[Opt] =
    &[opt("Default", "default"), opt("Left aligned", "left")];

/// How the user's messages stand out (`chatUserMessageStyle`), in the Settings control's order.
pub const CHAT_USER_MESSAGE_STYLE_OPTIONS: &[Opt] =
    &[opt("Default", "default"), opt("Band", "band")];

/// How the agent's work shows in the transcript (`chatAgentWorkStyle`), in the Settings control's order.
pub const CHAT_AGENT_WORK_STYLE_OPTIONS: &[Opt] = &[
    opt("Default", "default"),
    opt("Reduced", "reduced"),
    opt("Quiet", "quiet"),
];

/// How close together the chat's messages sit (`chatMessageSpacing`), in the Settings control's order.
pub const CHAT_MESSAGE_SPACING_OPTIONS: &[Opt] =
    &[opt("Default", "default"), opt("Compact", "compact")];

/// Where agents open a page they built for the user (`agentPreviewBrowser`), in the Settings control's order.
pub const AGENT_PREVIEW_BROWSER_OPTIONS: &[Opt] = &[
    opt("Ask", "ask"),
    opt("Ghostex Browser", "internal-browser"),
    opt("My default browser", "system-default-browser"),
];

pub const WEB_LINK_OPEN_TARGET_OPTIONS: &[Opt] = &[
    opt("Internal Browser", "internal-browser"),
    opt("System Default Browser", "system-default-browser"),
];

/// What a sidebar row shows of its session's links, in the Settings control's order
/// (`SessionLinksDisplay` in gx-core).
pub const SIDEBAR_SESSION_LINKS_OPTIONS: &[Opt] = &[
    opt("Always", "always"),
    opt("Selected session", "selected"),
    opt("Badge", "badge"),
    opt("Open work only", "open"),
    opt("Off", "off"),
];

/// Where a session's PR, issue or Linear link opens (the header's chips and the links list).
pub const SESSION_LINK_OPEN_TARGET_OPTIONS: &[Opt] = &[
    opt("Internal Browser", "embedded"),
    opt("System Default Browser", "external"),
];

/// How Windows blurs Desktop and windows behind the glass; Windows draws that blur and has no radius to set.
pub const WINDOW_GLASS_WINDOWS_BLUR_OPTIONS: &[Opt] = &[
    opt("Acrylic", "acrylic"),
    opt("Plain blur", "plain"),
    opt("Off", "off"),
];

pub const WINDOW_GLASS_IMAGE_PLACEMENT_OPTIONS: &[Opt] = &[
    opt("Moves with the window", "static"),
    opt("Stays with the desktop", "desktop"),
];

/// The Live glass animations, in the order the Theme page shows them; `video` (the user's own file) follows them.
pub const WINDOW_GLASS_LIVE_STYLE_OPTIONS: &[Opt] = &[
    opt("Aurora", "aurora"),
    opt("Ink", "ink"),
    opt("Drift", "drift"),
    opt("Nebula", "nebula"),
    opt("Silk", "silk"),
    opt("Bokeh", "bokeh"),
    opt("Waves", "waves"),
    opt("Mesh", "mesh"),
];

pub const WINDOW_GLASS_OPTIONS: &[Opt] = &[
    opt("Dark only", "auto"),
    opt("Always", "frosted"),
    opt("Off", "opaque"),
];

pub const WINDOW_GLASS_SOURCE_OPTIONS: &[Opt] = &[
    opt("Desktop and windows", "desktopAndWindows"),
    opt("Wallpaper only", "wallpaper"),
    opt("Custom image", "customImage"),
    opt("Live", "live"),
];
