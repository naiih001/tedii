# tedii

`tedii` is a terminal text editor built around modal editing, rope-backed buffers, syntax highlighting, and lightweight built-in file and git workflows.

It is designed for editing files in a terminal with a small, efficient command set:

- normal mode for navigation and operations
- insert mode for text entry
- command mode for save/quit and editor commands
- search, file explorer, fuzzy finder, and git status pickers

## What It Can Do

- Open and edit files from the terminal
- Create new files if the path does not already exist
- Navigate with modal keys
- Insert text, delete text, and edit in place
- Undo and redo edits
- Use syntax highlighting through Tree-sitter grammars
- Browse files, fuzzy-search project files, and inspect git status
- Customize theme colors and keybindings through TOML config

## Quick Start

Open a file:

```bash
tedii path/to/file.txt
```

Open a directory:

```bash
tedii path/to/project
```

If you pass a file path that does not exist, `tedii` will create it.

Start without arguments:

```bash
tedii
```

This opens the default welcome buffer.

## Command-Line Options

### Open a file or directory

```bash
tedii path/to/item
```

If the path is a file, the editor opens it directly.
If the path is a directory, `tedii` starts in that directory context.

### Initialize language config

```bash
tedii --init
```

Creates a default language configuration file if one does not already exist.

### Manage grammars

```bash
tedii --grammar fetch
tedii --grammar build
tedii --grammar update
```

- `fetch` downloads grammar sources
- `build` compiles grammar libraries
- `update` runs both steps in order

## Core Editing Model

`tedii` uses modal editing.

### Normal Mode

Normal mode is for movement and editor commands.

Common movement keys:

- `h` move left
- `j` move down
- `k` move up
- `l` move right
- `w` move forward by word
- `b` move backward by word
- `g` prefix for navigation commands such as `gg`, `gh`, `gl`, `ge`, `gj`, and `gk`

Common actions:

- `i` enter insert mode
- `A` move to end of line and enter insert mode
- `o` open a new line below and enter insert mode
- `O` open a new line above and enter insert mode
- `v` enter visual mode
- `x` select the current line into visual mode
- `p` paste from the editor clipboard
- `P` paste from the system clipboard
- `u` undo
- `Ctrl+U` redo

### Insert Mode

Insert mode is for typing text directly into the buffer.

Common keys:

- `Esc` return to normal mode
- `Enter` insert a newline
- `Backspace` delete the previous character
- `Tab` insert indentation, or split a bracket pair when the cursor is between matching brackets or braces

Tab behavior:

- outside a pair, `Tab` inserts four spaces
- between `()`, `[]`, or `{}`, `Tab` splits the pair into a multiline block

Example:

```text
function() {}
```

with the cursor between `{` and `}` becomes:

```text
function() {
    
}
```

### Command Mode

Command mode is entered with `:`.

Supported commands:

- `q` quit
- `w` save
- `wq` save and quit
- `git` open the git status picker if changes are available

## Saving and Quitting

Save from command mode with:

```text
:w
```

Save and quit with:

```text
:wq
```

Quit without saving with:

```text
:q
```

## Selection and Clipboard

Visual mode is used for selection-based editing.

Supported selection actions:

- enter visual mode with `v`
- select the current line with `x`
- extend selection up and down with `X` and `x` in visual mode
- yank selection with `y`
- yank selection to the system clipboard with `Y`
- delete selection with `d`

Pasted text can come from:

- the editor clipboard
- the system clipboard

## Search

The editor includes a search mode for finding text within the current buffer.

Search behavior is handled inside the editor UI and results are tracked as you type.

## File Explorer, Fuzzy Finder, and Git Picker

`tedii` includes a few built-in pickers that are useful for navigating a project quickly.

Open these from normal mode with the space-prefixed leader key:

- `Space` `e` toggle the file explorer
- `Space` `f` toggle the fuzzy finder
- `Space` `g` refresh and open the git picker when there are changes

### File Explorer

Use the file explorer to browse files and open them from within the editor.

### Fuzzy Finder

Use the fuzzy finder to jump to files by typing part of a name.

### Git Picker

Use the git picker to inspect and manage repository changes. `Space` `g` and
`:git` open it whenever the current file or project is inside a Git repository,
including when the working tree is clean.

Status-page keys:

- `j` / `k` or arrow keys move through staged and unstaged files
- `Space` stages an unstaged file or unstages a staged file
- `Enter` opens the selected file
- `c` opens a one-line commit message prompt for staged changes
- `l` opens the recent commit log
- `d` opens the selected file's complete `HEAD`-to-working-tree diff
- `Esc` closes the git popup

Commit prompt keys:

- type a single-line commit message
- `Enter` creates the commit
- `Backspace` edits the message
- `Esc` cancels and returns to status

Log and diff page keys:

- `j` / `k` or arrow keys scroll one line
- `Ctrl+d` / `Ctrl+u` or `PageDown` / `PageUp` scroll one page
- `Esc` returns to the status page

Git command failures are shown inside the popup. The popup remains open after
staging, unstaging, and committing so the refreshed repository state is
immediately visible.

## Releases

- Releases are managed from the `release` branch.
- `release-plz` opens and updates release PRs, generates changelog entries, and tags releases.
- Tagged releases build Linux and macOS release archives automatically.

## Syntax Highlighting

Syntax highlighting is powered by Tree-sitter grammars.

The editor will attempt to load a grammar and highlight query for the file type you open.

If no grammar is available for a file type, the file still opens and remains editable, just without language-specific highlighting.

## Grammar Setup

`tedii` stores language configuration in:

```text
~/.config/tedii/languages.toml
```

Use:

```bash
tedii --init
```

to create a default language config file.

The default config includes common languages such as:

- Rust
- Python
- JavaScript
- TypeScript
- Go
- C
- C++

After updating the config, fetch and build grammars:

```bash
tedii --grammar update
```

If you only want to refresh one stage:

```bash
tedii --grammar fetch
tedii --grammar build
```

## Theme and Keybindings

`tedii` reads editor configuration from:

```text
~/.config/tedii/config.toml
```

Two configuration areas are supported:

- `theme` for colors
- `keybindings` for editor behavior, including whether leader keys are enabled

Leader keys can be enabled or disabled through the keybindings config.

If no config file exists, the editor falls back to its built-in defaults.

## Runtime Files

The editor may create runtime data under:

```text
~/.config/tedii/runtime
```

This directory is used for fetched grammar sources and built grammar libraries.

## Troubleshooting

### The editor opens without syntax highlighting

- Check that the language is listed in `~/.config/tedii/languages.toml`
- Run `tedii --grammar update`
- Confirm the grammar source was fetched and built successfully

### Save does nothing

- Make sure the file path is writable
- Use command mode and run `:w`

### Grammar commands fail

- Make sure `git`, `tar`, and a C compiler are installed
- Re-run `tedii --grammar fetch` and `tedii --grammar build`

### Keybindings feel different than expected

- Check `~/.config/tedii/config.toml`
- Verify whether leader keys are enabled

## Project Docs

Detailed architecture notes live in the `docs/arch/` directory:

- `docs/arch/architecture.md`
- `docs/arch/data_structure.md`
- `docs/arch/interaction_model.md`
- `docs/arch/roadmap.md`

Those documents explain implementation details; this README focuses on using the editor.
