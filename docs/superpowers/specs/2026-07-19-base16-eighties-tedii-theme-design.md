# Base16 Eighties Theme for Tedii

## Goal

Rewrite `~/.config/tedii/config.toml` so Tedii visually matches the
`base16-eighties` Vim colorscheme and the configured
`Tomorrow_Night_Eighties` lightline theme. Leave `~/.vimrc` unchanged.

## Source of Truth

Use the installed files as the authoritative color definitions:

- `~/.vim/colors/base16-eighties.vim`
- `~/.vim/plugged/lightline.vim/autoload/lightline/colorscheme/Tomorrow_Night_Eighties.vim`

The Base16 Eighties palette is:

| Name | Color |
| --- | --- |
| base00 | `#2d2d2d` |
| base01 | `#393939` |
| base02 | `#515151` |
| base03 | `#747369` |
| base04 | `#a09f93` |
| base05 | `#d3d0c8` |
| base06 | `#e8e6df` |
| base07 | `#f2f0ec` |
| base08 | `#f2777a` |
| base09 | `#f99157` |
| base0A | `#ffcc66` |
| base0B | `#99cc99` |
| base0C | `#66cccc` |
| base0D | `#6699cc` |
| base0E | `#cc99cc` |
| base0F | `#d27b53` |

## Syntax Mapping

Map Tedii's Tree-sitter captures to the closest Vim highlight group:

| Tedii captures | Vim role | Color |
| --- | --- | --- |
| `keyword`, `keyword.control`, `keyword.control.conditional` | Keyword / Conditional | `#cc99cc` |
| `keyword.control.import` | Include | `#6699cc` |
| `keyword.control.repeat` | Repeat | `#ffcc66` |
| `function`, `function.method`, `function.builtin` | Function | `#6699cc` |
| `string`, `string.quoted` | String | `#99cc99` |
| `string.special` | SpecialChar | `#d27b53` |
| `comment` | Comment | `#747369`, italic |
| `type`, `type.builtin`, `constructor` | Type | `#ffcc66` |
| `constant`, `constant.builtin`, `number` | Constant / Number | `#f99157` |
| `operator` | Operator | `#d3d0c8` |
| `punctuation`, `punctuation.delimiter` | Delimiter | `#d27b53` |
| `variable`, `parameter` | Normal | `#d3d0c8` |
| `property` | Identifier | `#f2777a` |
| `attribute`, `label` | PreProc / Label | `#ffcc66` |
| `embedded` | Special | `#66cccc` |

Only comments retain italics because that is the modifier used by the
installed Vim colorscheme for the corresponding role.

## UI Mapping

Use `#2d2d2d` as the editor background and `#d3d0c8` as the normal
foreground. Match Vim's search, visual selection, gutters, completion
menu, directories, borders, and git gutter roles directly.

Use the configured lightline theme for Tedii modes:

- Normal: dark foreground on `#99cccc`
- Insert: dark foreground on `#99cc99`
- Command: dark foreground on `#ffcc66`
- Fuzzy: dark foreground on `#6699cc`
- Visual: dark foreground on `#cc99cc`

Populate every UI key supported by Tedii, including diagnostics, hover,
completion, file explorer, fuzzy finder, and git picker. This prevents
unconfigured keys from falling back to Tedii's built-in theme.

## Validation

After replacing the config:

1. Parse the TOML with Python's `tomllib`.
2. Confirm all syntax and UI keys used by Tedii are present.
3. Confirm no previous Gruvbox palette values remain.
4. Start Tedii briefly against a temporary file to catch configuration
   loading errors, if the binary is available and supports a safe
   non-interactive check.

## Scope

This change modifies only `~/.config/tedii/config.toml`. It does not
change Vim, Tedii source code, keybindings, grammars, or repository
runtime behavior.
