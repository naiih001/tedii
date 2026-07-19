# Base16 Eighties Tedii Theme Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Rewrite `~/.config/tedii/config.toml` so Tedii matches the installed Vim `base16-eighties` syntax colors and `Tomorrow_Night_Eighties` lightline UI palette.

**Architecture:** Keep the existing Tedii configuration format and replace only the `[theme]` section. Use exact colors from the installed Vim colorscheme for syntax and editor UI, then use the installed lightline palette for Tedii mode indicators. Populate all keys supported by Tedii so no Gruvbox or built-in fallback colors remain.

**Tech Stack:** TOML, Python `tomllib`, Rust/Tedii theme loader.

## Global Constraints

- Leave `~/.vimrc` unchanged.
- Modify only `~/.config/tedii/config.toml` during implementation.
- Use the exact Base16 Eighties colors from `~/.vim/colors/base16-eighties.vim`.
- Use the exact lightline mode colors from `~/.vim/plugged/lightline.vim/autoload/lightline/colorscheme/Tomorrow_Night_Eighties.vim`.
- Preserve italic comments.
- Do not change keybindings, grammars, or Tedii source code.

---

### Task 1: Replace Tedii Theme Configuration

**Files:**
- Modify: `/home/nate/.config/tedii/config.toml`
- Test: `/tmp/validate-tedii-theme.py`

**Interfaces:**
- Consumes: Existing Tedii theme keys from `src/theme.rs`, Base16 Eighties colors from the installed Vim colorscheme, and lightline mode colors from the installed lightline colorscheme.
- Produces: A parseable `[theme]` TOML section with complete syntax and UI mappings.

- [ ] **Step 1: Capture the current live config**

Run:

```bash
cp /home/nate/.config/tedii/config.toml /tmp/tedii-config.toml.before-base16-eighties
```

Expected: `/tmp/tedii-config.toml.before-base16-eighties` contains the current Gruvbox-based config.

- [ ] **Step 2: Replace the theme values**

Replace the current theme section with the following mapping:

```toml
[theme]

# Base16 Eighties
# Background: #2d2d2d  Foreground: #d3d0c8

[theme.syntax]
"keyword"                     = { fg = "#cc99cc" }
"keyword.control"             = { fg = "#cc99cc" }
"keyword.control.conditional" = { fg = "#cc99cc" }
"keyword.control.import"      = { fg = "#6699cc" }
"keyword.control.repeat"      = { fg = "#ffcc66" }
"function"                    = { fg = "#6699cc" }
"function.method"             = { fg = "#6699cc" }
"function.builtin"            = { fg = "#6699cc" }
"string"                      = { fg = "#99cc99" }
"string.quoted"               = { fg = "#99cc99" }
"string.special"              = { fg = "#d27b53" }
"comment"                     = { fg = "#747369", modifiers = ["italic"] }
"type"                        = { fg = "#ffcc66" }
"type.builtin"                = { fg = "#ffcc66" }
"constant"                    = { fg = "#f99157" }
"constant.builtin"            = { fg = "#f99157" }
"number"                      = { fg = "#f99157" }
"operator"                    = { fg = "#d3d0c8" }
"punctuation"                 = { fg = "#d27b53" }
"punctuation.delimiter"       = { fg = "#d27b53" }
"variable"                    = { fg = "#d3d0c8" }
"property"                    = { fg = "#f2777a" }
"constructor"                 = { fg = "#ffcc66" }
"attribute"                   = { fg = "#ffcc66" }
"label"                       = { fg = "#ffcc66" }
"embedded"                    = { fg = "#66cccc" }
"parameter"                   = { fg = "#d3d0c8" }

[theme.ui]
"mode_normal"           = { fg = "#2d2d2d", bg = "#99cccc" }
"mode_insert"           = { fg = "#2d2d2d", bg = "#99cc99" }
"mode_command"          = { fg = "#2d2d2d", bg = "#ffcc66" }
"mode_fuzzy"            = { fg = "#2d2d2d", bg = "#6699cc" }
"mode_visual"           = { fg = "#2d2d2d", bg = "#cc99cc" }
"status_bar_filename"   = { fg = "#d3d0c8", bg = "#393939" }
"status_bar_cursor_pos" = { fg = "#aaaaaa", bg = "#393939" }
"status_bar_branch"     = { fg = "#99cc99", bg = "#393939" }
"editor_bg"             = { fg = "#d3d0c8", bg = "#2d2d2d" }
"search_match"          = { fg = "#393939", bg = "#ffcc66" }
"visual_selection"      = { fg = "#d3d0c8", bg = "#515151" }
"gutter_line"           = { fg = "#747369", bg = "#2d2d2d" }
"gutter_current_line"   = { fg = "#a09f93", bg = "#393939" }
"gutter_diff_added"     = { fg = "#99cc99", bg = "#2d2d2d" }
"gutter_diff_modified"  = { fg = "#6699cc", bg = "#2d2d2d" }
"gutter_diff_deleted"   = { fg = "#f2777a", bg = "#2d2d2d" }
"explorer_border"       = { fg = "#515151" }
"explorer_filter"       = { fg = "#6699cc" }
"explorer_selected"     = { fg = "#393939", bg = "#d3d0c8" }
"explorer_dir"          = { fg = "#6699cc" }
"fuzzy_border"          = { fg = "#515151" }
"fuzzy_query"           = { fg = "#6699cc" }
"fuzzy_selected"        = { fg = "#393939", bg = "#d3d0c8" }
"fuzzy_dir"             = { fg = "#6699cc" }
"fuzzy_match"           = { fg = "#ffcc66" }
"git_border"            = { fg = "#515151" }
"git_query"             = { fg = "#6699cc" }
"git_selected"          = { fg = "#393939", bg = "#d3d0c8" }
"git_status_modified"   = { fg = "#ffcc66" }
"git_status_added"      = { fg = "#99cc99" }
"git_status_deleted"    = { fg = "#f2777a" }
"git_status_untracked"  = { fg = "#747369" }
"git_status_conflict"   = { fg = "#f2777a" }
"git_status_renamed"    = { fg = "#6699cc" }
"diagnostic_error"      = { fg = "#f2777a" }
"diagnostic_warning"    = { fg = "#ffcc66" }
"diagnostic_information" = { fg = "#6699cc" }
"diagnostic_hint"       = { fg = "#66cccc" }
"hover_border"          = { fg = "#515151" }
"hover_text"            = { fg = "#d3d0c8" }
"completion_border"     = { fg = "#515151" }
"completion_selected"   = { fg = "#393939", bg = "#d3d0c8" }
"completion_label"      = { fg = "#d3d0c8" }
"completion_detail"     = { fg = "#747369" }
```

Expected: The config contains no Gruvbox color values and every key in
`default_ui()` in `src/theme.rs` has an explicit override.

- [ ] **Step 3: Validate TOML and required keys**

Create `/tmp/validate-tedii-theme.py` with:

```python
import tomllib
from pathlib import Path

config_path = Path("/home/nate/.config/tedii/config.toml")
with config_path.open("rb") as config_file:
    config = tomllib.load(config_file)

assert config["theme"]["syntax"]["comment"] == {
    "fg": "#747369",
    "modifiers": ["italic"],
}
assert config["theme"]["ui"]["editor_bg"] == {
    "fg": "#d3d0c8",
    "bg": "#2d2d2d",
}
assert config["theme"]["ui"]["mode_insert"]["bg"] == "#99cc99"

source = config_path.read_text()
for old_color in (
    "#32302f", "#fb4934", "#8ec07c", "#b8bb26", "#fe8019",
    "#a89984", "#fabd2f", "#d3869b", "#d5c4a1", "#ebdbb2",
    "#83a598",
):
    assert old_color not in source, old_color

print("Tedii Base16 Eighties theme TOML is valid")
```

Run:

```bash
python3 /tmp/validate-tedii-theme.py
```

Expected:

```text
Tedii Base16 Eighties theme TOML is valid
```

- [ ] **Step 4: Validate the complete key set against Tedii source**

Run:

```bash
python3 - <<'PY'
import re
from pathlib import Path
import tomllib

source = Path("src/theme.rs").read_text()
default_ui = source.split("fn default_ui()", 1)[1].split("#[derive(Clone)]", 1)[0]
config = tomllib.loads(Path("/home/nate/.config/tedii/config.toml").read_text())
configured = set(config["theme"]["ui"])
expected = set(re.findall(r'\(\s*"([^"]+)"\s*,', default_ui))
missing = expected - configured
assert not missing, sorted(missing)
print(f"All {len(expected)} Tedii default UI keys are explicitly configured")
PY
```

Expected: A success line with zero missing keys.

- [ ] **Step 5: Check the final diff**

Run:

```bash
diff -u /tmp/tedii-config.toml.before-base16-eighties /home/nate/.config/tedii/config.toml
```

Expected: Only the theme values differ; there are no changes to keybindings or non-theme sections.

- [ ] **Step 6: Commit the repository plan**

Run:

```bash
git add docs/superpowers/plans/2026-07-19-base16-eighties-tedii-theme.md
git commit -m "docs: plan Base16 Eighties Tedii theme"
```

Expected: A commit containing only the implementation plan.

## Completion Checklist

- [ ] `~/.vimrc` is unchanged.
- [ ] `~/.config/tedii/config.toml` parses with Python `tomllib`.
- [ ] All syntax and UI mappings use the Base16 Eighties/lightline palette.
- [ ] Comments remain italic.
- [ ] No Gruvbox palette values remain.
- [ ] All Tedii default UI keys are explicitly configured.
