# 08 — Git Integration

> Git repository operations, diff engine, popup UI workflows.

## 1. Architecture

```
┌──────────────┐     ┌──────────────┐     ┌──────────────┐
│   Editor     │────>│   GitRepo    │────>│ gitoxide     │
│              │     │              │     │ (read-only)  │
│  diff_hunks  │     │              │     │              │
│  git_branch  │     │──────────────│────>│ git CLI      │
│              │     │              │     │ (mutations)  │
│  GitPicker   │────>│ stage/       │     └──────────────┘
│  (UI popup)  │     │ unstage/     │
│              │     │ commit/log   │
└──────────────┘     └──────────────┘
```

---

## 2. Discovery & Initialization

```rust
// In Editor::new() for a given file path:
let repo = GitRepo::discover(file_path);
// Uses gix::ThreadSafeRepository::discover() which walks up directories

if let Some(repo) = &repo {
    git_branch = repo.current_branch();
    diff_base = repo.diff_base(file_path);
    // diff_base retrieves file content from HEAD tree
}
```

### GitRepo::discover()

1. Start from given file/directory path
2. Walk up parent directories looking for `.git`
3. Uses `gix::ThreadSafeRepository::discover()`
4. Returns `None` if no repository found

---

## 3. Read Operations (gitoxide)

| Operation | Method | Implementation |
|-----------|--------|----------------|
| Discover repo | `GitRepo::discover()` | `gix::ThreadSafeRepository::discover()` |
| Current branch | `GitRepo::current_branch()` | `repo.head_name()?.shorten()?` or fallback to `repo.head()?.id()` |
| Diff base content | `GitRepo::diff_base()` | Traverse HEAD tree → find blob by path → read bytes |

---

## 4. Write Operations (git CLI)

| Operation | Method | CLI Command |
|-----------|--------|-------------|
| Status | `GitRepo::status()` | `git status --porcelain=v1 -z --untracked-files=all` |
| Stage | `GitRepo::stage()` | `git add <file>` |
| Unstage | `GitRepo::unstage()` | `git restore --staged <file>` (or `git rm --cached` if no HEAD) |
| Commit | `GitRepo::commit()` | `git commit -m <message>` |
| Log | `GitRepo::log()` | `git log -n<limit> --date=short --pretty=format:%h%x1f%s%x1f%an%x1f%ad%x1e` |
| Diff from HEAD | `GitRepo::diff_from_head()` | `git diff --no-ext-diff --no-color HEAD -- <path>` |

---

## 5. Status Parser

The `parse_porcelain_v1()` function parses NUL-delimited output:

```rust
// Input format (NUL-delimited):
// XY filename\0
// XY old_name\0new_name\0  (for renames/copies)

// X = index status (staged)
// Y = worktree status (unstaged)

// Returns Vec<FileChange> grouped:
// 1. Staged entries (where index_status != ' ')
// 2. Unstaged entries (where worktree_status != ' ')

// Each 2-character code is mapped via status_from_code():
// M → Modified, A → Added, ? → Untracked, D → Deleted
// R → Renamed, C → Copied, U → Conflict (via is_conflict()), T → TypeChanged
```

---

## 6. Line-Level Diff (imara_diff)

```rust
pub fn compute_diff(base: &str, modified: &str) -> Vec<DiffHunk> {
    // Uses imara_diff with Histogram algorithm
    // Groups hunks by line type:
    //   Added    → lines only in modified (pure insertions)
    //   Removed  → lines only in base (pure deletions)
    //   Modified → all other changes
    // Sorts by line number, deduplicates
    // Returns 0-based line numbers in the modified text
}

pub struct DiffHunk {
    pub line: u32,    // 0-based line in modified text
    pub kind: DiffKind,  // Added | Removed | Modified
}
```

---

## 7. GitPicker UI

### Page Navigation

```
Status ──c──→ Commit
  │              │
  │    Esc       │ Enter
  ▼              ▼
Status ←────── Status
  │
  ├──l──→ Log ←──Esc── Status
  │
  └──d──→ Diff ←──Esc── Status
```

### Status Display

```
Staged (N)      ← Header row
M  file1.rs     ← Entry row
A  file2.rs
Unstaged (N)    ← Header row
M  file3.rs     ← Entry row (selected → highlighted)
?  file4.txt
D  file5.rs

Working tree clean.  ← Text row (when no changes)
```

### Key Bindings by Page

| Key | Status | Commit | Log | Diff |
|-----|--------|--------|-----|------|
| j/Down | navigate down | - | scroll down | scroll down |
| k/Up | navigate up | - | scroll up | scroll up |
| Enter | open file | submit commit | - | - |
| Space | stage/unstage | - | - | - |
| c | begin commit | - | - | - |
| l | open log | - | - | - |
| d | open diff | - | - | - |
| Ctrl+d | - | - | page down | page down |
| Ctrl+u | - | - | page up | page up |
| Esc | close | back | back | back |
| Backspace | - | remove char | - | - |

### Stage/Unstage Flow

```
1. User presses Space on a file entry
2. toggle_stage() determines action:
   - If entry.section == Staged → Unstage
   - If entry.section == Unstaged → Stage
3. Call GitRepo::stage() or GitRepo::unstage()
4. Refresh entries from repo.status()
5. Restore selection to the same file (now in opposite section)
```

### Commit Flow

```
1. User presses 'c' → GitPage::Commit
2. User types commit message (add_commit_char, remove_commit_char)
3. User presses Enter → submit_commit():
   a. Validate non-empty message
   b. GitRepo::commit(message)
   c. Return to Status page
   d. Refresh entries
   e. Show success/error message in feedback bar
```

### Diff View Coloring

```
@@ -1,4 +1,5 @@    → cyan (header)
- old line          → red
+ new line          → green
  unchanged line    → default
```

### Log View

- Shows last 100 commits: hash, subject, author, date
- Uniform styling via `git_page_text`
- Scrollable with j/k and PgDn/PgUp
