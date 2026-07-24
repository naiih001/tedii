# 09 — Dependency Graph

> Inter-module dependencies, crate dependency tree, external libraries.

## 1. Internal Module Dependency Graph

```
                        ┌──────────┐
                        │  main    │
                        └────┬─────┘
               ┌─────────────┼─────────────┐
               │             │             │
               ▼             ▼             ▼
          ┌────────┐  ┌──────────┐  ┌───────────┐
          │  tui   │  │  editor  │  │  config   │
          └────────┘  └────┬─────┘  └─────┬─────┘
                           │              │
               ┌───────────┼───────┐      │
               │           │       │      │
               ▼           ▼       ▼      ▼
          ┌────────┐ ┌────────┐ ┌──────┐ ┌────────┐
          │ syntax │ │  lsp   │ │ git  │ │ theme  │
          └────┬───┘ └───┬────┘ └──┬───┘ └───┬────┘
               │         │         │         │
               ▼         │         │         │
          ┌────────┐     │         │         │
          │ theme  │     │         │         │
          └────────┘     │         │         │
                         │         │         │
          ┌────────┐     │         │         │
          │ hover  │◄────┘         │         │
          └────────┘               │         │
                                   │         │
          ┌────────────┐           │         │
          │ completion │◄──────────┘         │
          └─────┬──────┘                     │
                │                           │
                ▼                           │
          ┌────────┐                        │
          │ fuzzy  │                        │
          └────────┘                        │
                                            │
          ┌────────────┐                    │
          │file_explorer│───────────────────┤
          └────────────┘                    │
                                            │
          ┌──────────────┐                  │
          │fuzzy_finder  │──────────────────┤
          └──────┬───────┘                  │
                 │                          │
                 ▼                          │
          ┌────────┐                        │
          │ fuzzy  │                        │
          └────────┘                        │
                                            │
          ┌────────────┐                    │
          │ git_picker │────────────────────┤
          └──────┬─────┘                    │
                 │                          │
                 ▼                          │
          ┌────────┐                        │
          │  git   │                        │
          └────────┘                        │
                                            │
          ┌──────────────────┐              │
          │grammar_commands  │──────────────┘
          └──────────────────┘
```

### Dependency Direction

```
A ──> B  means "A depends on B" (A uses types/functions from B)
```

### Direct Dependencies Summary

| Module | Depends On |
|--------|------------|
| `main` | `editor`, `tui`, `config`, `theme`, `file_explorer`, `fuzzy_finder`, `git_picker`, `completion`, `hover` |
| `editor` | `syntax`, `lsp`, `git`, `config`, `theme`, `hover`, `completion` |
| `syntax` | `theme` |
| `lsp` | `config` |
| `hover` | `lsp` |
| `completion` | `lsp`, `fuzzy` |
| `fuzzy_finder` | `fuzzy`, `theme` |
| `file_explorer` | `theme` |
| `git_picker` | `git`, `theme` |
| `grammar_commands` | `config` |
| `config` | (none beyond serde/toml) |
| `theme` | `config` |
| `tui` | (none beyond crossterm/ratatui) |
| `fuzzy` | (none) |
| `git` | (none beyond gix/imara_diff) |

---

## 2. External Crate Dependencies

```
Cargo.toml dependencies:
├── ratatui 0.29          TUI framework
├── crossterm 0.28        Terminal control (raw mode, events, cursor)
├── ropey 1.6             Rope data structure (text buffer)
├── tree-sitter 0.25      Parser generator runtime
├── gix 0.84              Gitoxide (pure Rust git)
├── imara-diff 0.2        Line diff engine
├── serde 1.0             Serialization framework
│   └── serde_derive
├── serde_json 1.0        JSON for LSP protocol
├── toml 0.8              TOML for config files
├── libloading 0.8        Dynamic library loading (grammars)
├── anyhow 1.0            Error handling
├── dirs 5.0              Platform config directories
├── reqwest 0.12          HTTP client (grammar fetching)
│   └── blocking feature
│   └── rustls-tls
└── arboard 3.0           System clipboard access
```

---

## 3. Runtime Dependency Flow

```
┌─────────────────────────────────────────────────────────┐
│                   Compile Time                            │
│  Cargo.toml → cargo build → binary (tedii)               │
└─────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────┐
│                    Runtime                                │
│                                                          │
│  tedii binary                                            │
│    ├── ~/.config/tedii/languages.toml  (config)          │
│    ├── ~/.config/tedii/config.toml     (theme/bindings)  │
│    ├── ~/.config/tedii/grammars/*.so   (tree-sitter)     │
│    ├── ~/.config/tedii/queries/*/*.scm (highlight files) │
│    ├── LSP server process (e.g., rust-analyzer)          │
│    │   └── Communicates via stdin/stdout JSON-RPC        │
│    ├── git CLI (for mutations)                           │
│    │   └── git add, git restore, git commit, etc.        │
│    └── System clipboard (via arboard)                    │
└─────────────────────────────────────────────────────────┘
```

---

## 4. Module Size Estimate (approx. lines)

```
Module              Lines (code)   Lines (tests)
────────────────────────────────────────────────
main.rs             900            40
editor.rs           1,100          500
lsp.rs              480            0
git.rs              400            0
git_picker.rs       470            0
syntax.rs           260            0
file_explorer.rs    200            40
fuzzy_finder.rs     230            40
hover.rs            200            80
completion.rs       230            120
theme.rs            150            30
config.rs           140            0
fuzzy.rs            70             0
tui.rs              30             0
grammar_commands.rs 230            0
────────────────────────────────────────────────
Total               ~5,100         ~850
```
