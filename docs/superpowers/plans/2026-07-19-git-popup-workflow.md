# Git Popup Workflow Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn the git status picker into a compact workflow for staging, unstaging, committing, viewing history, and inspecting file diffs.

**Architecture:** `src/git.rs` becomes the single subprocess-backed Git service and exposes repository-relative status entries and action results. `src/git_picker.rs` owns popup pages, selection, scrolling, commit input, and feedback; `src/main.rs` only maps keys to those operations and opens selected files.

**Tech Stack:** Rust 2021, Ratatui, Crossterm, the installed Git CLI, inline Rust unit and temporary-repository integration tests

## Global Constraints

- Preserve unrelated worktree changes and existing popup/theme conventions.
- Use file-level staging only; hunk staging is out of scope.
- Show separate staged and unstaged sections, including the same path in both when applicable.
- `Space` toggles the selected entry's staged state.
- `c` accepts a required one-line commit message and commits staged changes only.
- `l` opens a scrollable summary log; `d` opens the selected path's full `HEAD`-to-worktree diff.
- Git popup commands open in clean repositories and the popup remains open after actions.

---

### Task 1: Git Command Service

**Files:**
- Modify: `src/git.rs`

**Interfaces:**
- Produces: `ChangeSection`, `FileChange`, `GitLogEntry`, `GitRepo::status`, `GitRepo::stage`, `GitRepo::unstage`, `GitRepo::commit`, `GitRepo::log`, and `GitRepo::diff_from_head`

- [ ] Add parser tests for staged, unstaged, dual-state, untracked, deleted, renamed, and conflicted porcelain records.
- [ ] Run the focused parser tests and confirm they fail because the richer status model is absent.
- [ ] Implement NUL-delimited porcelain parsing and repository-relative command execution.
- [ ] Add temporary-repository tests for stage, unstage, commit, log, tracked diff, untracked diff, and an unborn repository.
- [ ] Run `cargo test git::tests -- --nocapture` until the backend tests pass.

### Task 2: Picker State and Pages

**Files:**
- Modify: `src/git_picker.rs`

**Interfaces:**
- Consumes: Git service types and operations from Task 1
- Produces: popup open/refresh methods, selected-path access, stage toggle, commit input, page navigation, log loading, diff loading, and rendering

- [ ] Add failing tests for grouped selection, navigation, stage action selection, commit prompt transitions, page return, and scroll bounds.
- [ ] Run `cargo test git_picker::tests -- --nocapture` and confirm the new tests fail.
- [ ] Implement status rows, page state, command feedback, commit input, and page scrolling.
- [ ] Render status, commit, log, and diff pages with contextual key hints and clean/error states.
- [ ] Run the focused picker tests until they pass.

### Task 3: Event Routing and Documentation

**Files:**
- Modify: `src/main.rs`
- Modify: `src/theme.rs`
- Modify: `README.md`

**Interfaces:**
- Consumes: `GitPicker` operations from Task 2

- [ ] Route popup-specific keys before editor modes: `Space`, `c`, `l`, `d`, text entry, movement, paging, `Enter`, and `Esc`.
- [ ] Open the popup whenever a repository is discovered, including when clean.
- [ ] Add only the theme tokens needed for section headers, page text, success messages, and errors.
- [ ] Document all Git popup commands and page navigation.
- [ ] Run focused tests and compile checks.

### Task 4: Verification

**Files:**
- Review: all modified files

- [ ] Run `cargo test`.
- [ ] Run `cargo fmt --check`.
- [ ] Run `cargo clippy --all-targets --all-features -- -D warnings`.
- [ ] Run `git diff --check`.
- [ ] Review `git diff` against every global constraint and confirm unrelated files are untouched.
