use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use gix::objs::tree::EntryKind;
use gix::{Repository, ThreadSafeRepository};

use imara_diff::{Algorithm, Diff, InternedInput};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeSection {
    Staged,
    Unstaged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeStatus {
    Modified,
    Added,
    Untracked,
    Deleted,
    Renamed,
    Copied,
    Conflict,
    TypeChanged,
}

impl ChangeStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Modified => "M",
            Self::Added => "A",
            Self::Untracked => "?",
            Self::Deleted => "D",
            Self::Renamed => "R",
            Self::Copied => "C",
            Self::Conflict => "U",
            Self::TypeChanged => "T",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChange {
    pub path: PathBuf,
    pub original_path: Option<PathBuf>,
    pub section: ChangeSection,
    pub status: ChangeStatus,
}

impl FileChange {
    pub fn label(&self) -> &'static str {
        self.status.label()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitLogEntry {
    pub short_hash: String,
    pub subject: String,
    pub author: String,
    pub date: String,
}

pub struct GitRepo {
    repo: Repository,
    work_dir: PathBuf,
}

impl GitRepo {
    pub fn discover(file: &Path) -> Option<Self> {
        let dir = if file.is_dir() {
            file
        } else {
            file.parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."))
        };
        let repo = ThreadSafeRepository::discover(dir).ok()?;
        let repo = repo.to_thread_local();
        let work_dir = repo.workdir()?.to_path_buf();
        Some(Self { repo, work_dir })
    }

    pub fn work_dir(&self) -> &Path {
        &self.work_dir
    }

    pub fn current_branch(&self) -> Option<String> {
        let head_ref = self.repo.head_ref().ok()?;
        let head_commit = self.repo.head_commit().ok()?;
        Some(match head_ref {
            Some(reference) => reference.name().shorten().to_string(),
            None => head_commit.id.to_hex_with_len(8).to_string(),
        })
    }

    pub fn diff_base(&self, file: &Path) -> Option<Vec<u8>> {
        let work_dir = self.repo.workdir()?;
        let rela_path = file.strip_prefix(work_dir).ok()?;
        let head = self.repo.head_commit().ok()?;
        let tree = head.tree().ok()?;
        let entry = tree.lookup_entry_by_path(rela_path).ok()?;
        let tree_entry = entry?;
        match tree_entry.mode().kind() {
            EntryKind::Blob | EntryKind::BlobExecutable => {}
            _ => return None,
        }
        let oid = tree_entry.object_id();
        let obj = self.repo.find_object(oid).ok()?;
        let data = obj.detach().data;
        Some(data.to_vec())
    }

    pub fn status(&self) -> Result<Vec<FileChange>, String> {
        let output = self.run_git(&["status", "--porcelain=v1", "-z", "--untracked-files=all"])?;
        if !output.status.success() {
            return Err(command_error(&output));
        }
        Ok(parse_porcelain_v1(&output.stdout, &self.work_dir))
    }

    pub fn stage(&self, change: &FileChange) -> Result<(), String> {
        self.run_paths("add", change)
    }

    pub fn unstage(&self, change: &FileChange) -> Result<(), String> {
        let result = self.run_paths("restore", change);
        if result.is_err() && !self.has_head() {
            self.run_paths("rm", change)
        } else {
            result
        }
    }

    pub fn commit(&self, message: &str) -> Result<(), String> {
        let output = self.run_git_with_args(&["commit", "-m", message])?;
        if output.status.success() {
            Ok(())
        } else {
            Err(command_error(&output))
        }
    }

    pub fn log(&self, limit: usize) -> Result<Vec<GitLogEntry>, String> {
        let output = self.run_git_with_args(&[
            "log",
            &format!("-n{limit}"),
            "--date=short",
            "--pretty=format:%h%x1f%s%x1f%an%x1f%ad%x1e",
        ])?;
        if !output.status.success() {
            if !self.has_head() {
                return Ok(Vec::new());
            }
            return Err(command_error(&output));
        }
        Ok(parse_log(&output.stdout))
    }

    pub fn diff_from_head(&self, path: &Path) -> Result<String, String> {
        let relative = self.relative_path(path)?;
        if self.has_head() && !self.is_untracked(&relative) {
            let output = self.run_git_with_path(
                &["diff", "--no-ext-diff", "--no-color", "HEAD", "--"],
                &relative,
            )?;
            if !output.status.success() {
                return Err(command_error(&output));
            }
            return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
        }

        let output = Command::new("git")
            .arg("-C")
            .arg(&self.work_dir)
            .arg("diff")
            .arg("--no-ext-diff")
            .arg("--no-color")
            .arg("--no-index")
            .arg("--")
            .arg("/dev/null")
            .arg(&relative)
            .output()
            .map_err(|err| err.to_string())?;
        if output.status.success() || output.status.code() == Some(1) {
            Ok(String::from_utf8_lossy(&output.stdout).into_owned())
        } else {
            Err(command_error(&output))
        }
    }

    fn run_paths(&self, operation: &str, change: &FileChange) -> Result<(), String> {
        let mut paths = vec![self.relative_path(&change.path)?];
        if let Some(original) = &change.original_path {
            paths.push(self.relative_path(original)?);
        }
        let mut command = Command::new("git");
        command.arg("-C").arg(&self.work_dir);
        match operation {
            "add" => {
                command.arg("add");
                if paths.len() > 1 {
                    command.arg("-A");
                }
            }
            "restore" => {
                command.args(["restore", "--staged"]);
            }
            "rm" => {
                command.args(["rm", "--cached", "--ignore-unmatch"]);
            }
            _ => return Err(format!("unsupported git operation: {operation}")),
        }
        command.arg("--").args(paths);
        let output = command.output().map_err(|err| err.to_string())?;
        if output.status.success() {
            Ok(())
        } else {
            Err(command_error(&output))
        }
    }

    fn relative_path(&self, path: &Path) -> Result<PathBuf, String> {
        path.strip_prefix(&self.work_dir)
            .map(Path::to_path_buf)
            .map_err(|_| format!("path is outside repository: {}", path.display()))
    }

    fn run_git(&self, args: &[&str]) -> Result<Output, String> {
        self.run_git_with_args(args)
    }

    fn run_git_with_args(&self, args: &[&str]) -> Result<Output, String> {
        Command::new("git")
            .arg("-C")
            .arg(&self.work_dir)
            .args(args)
            .output()
            .map_err(|err| err.to_string())
    }

    fn run_git_with_path(&self, args: &[&str], path: &Path) -> Result<Output, String> {
        Command::new("git")
            .arg("-C")
            .arg(&self.work_dir)
            .args(args)
            .arg(path)
            .output()
            .map_err(|err| err.to_string())
    }

    fn has_head(&self) -> bool {
        self.run_git_with_args(&["rev-parse", "--verify", "HEAD"])
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    fn is_untracked(&self, path: &Path) -> bool {
        self.run_git_with_path(
            &["status", "--porcelain=v1", "--untracked-files=all", "--"],
            path,
        )
        .map(|output| output.status.success() && output.stdout.starts_with(b"?? "))
        .unwrap_or(false)
    }
}

fn parse_porcelain_v1(output: &[u8], root: &Path) -> Vec<FileChange> {
    let mut records = output
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty());
    let mut staged = Vec::new();
    let mut unstaged = Vec::new();

    while let Some(record) = records.next() {
        if record.len() < 4 || record[2] != b' ' {
            continue;
        }

        let index_status = record[0];
        let worktree_status = record[1];
        let path = root.join(String::from_utf8_lossy(&record[3..]).as_ref());
        let has_rename =
            matches!(index_status, b'R' | b'C') || matches!(worktree_status, b'R' | b'C');
        let original_path = if has_rename {
            records
                .next()
                .map(|path| root.join(String::from_utf8_lossy(path).as_ref()))
        } else {
            None
        };

        if is_conflict(index_status, worktree_status) {
            unstaged.push(FileChange {
                path,
                original_path,
                section: ChangeSection::Unstaged,
                status: ChangeStatus::Conflict,
            });
            continue;
        }

        if index_status != b' ' && index_status != b'?' {
            if let Some(status) = status_from_code(index_status) {
                staged.push(FileChange {
                    path: path.clone(),
                    original_path: original_path.clone(),
                    section: ChangeSection::Staged,
                    status,
                });
            }
        }

        if worktree_status != b' ' || index_status == b'?' {
            let code = if index_status == b'?' {
                b'?'
            } else {
                worktree_status
            };
            if let Some(status) = status_from_code(code) {
                unstaged.push(FileChange {
                    path,
                    original_path,
                    section: ChangeSection::Unstaged,
                    status,
                });
            }
        }
    }

    staged.extend(unstaged);
    staged
}

fn status_from_code(code: u8) -> Option<ChangeStatus> {
    match code {
        b'M' => Some(ChangeStatus::Modified),
        b'A' => Some(ChangeStatus::Added),
        b'?' => Some(ChangeStatus::Untracked),
        b'D' => Some(ChangeStatus::Deleted),
        b'R' => Some(ChangeStatus::Renamed),
        b'C' => Some(ChangeStatus::Copied),
        b'T' => Some(ChangeStatus::TypeChanged),
        _ => None,
    }
}

fn is_conflict(index: u8, worktree: u8) -> bool {
    matches!(
        (index, worktree),
        (b'D', b'D')
            | (b'A', b'U')
            | (b'U', b'D')
            | (b'U', b'A')
            | (b'D', b'U')
            | (b'A', b'A')
            | (b'U', b'U')
    )
}

fn parse_log(output: &[u8]) -> Vec<GitLogEntry> {
    String::from_utf8_lossy(output)
        .split('\x1e')
        .filter_map(|record| {
            let record = record.trim();
            if record.is_empty() {
                return None;
            }
            let mut fields = record.split('\x1f');
            Some(GitLogEntry {
                short_hash: fields.next()?.to_string(),
                subject: fields.next()?.to_string(),
                author: fields.next()?.to_string(),
                date: fields.next()?.to_string(),
            })
        })
        .collect()
}

fn command_error(output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if !stderr.is_empty() {
        return stderr;
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !stdout.is_empty() {
        stdout
    } else {
        format!("git command failed with {}", output.status)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffKind {
    Added,
    Removed,
    Modified,
}

#[derive(Debug, Clone)]
pub struct DiffHunk {
    pub line: u32,
    pub kind: DiffKind,
}

pub fn compute_diff(base: &str, modified: &str) -> Vec<DiffHunk> {
    let input = InternedInput::new(base, modified);
    let mut diff = Diff::compute(Algorithm::Histogram, &input);
    diff.postprocess_lines(&input);

    let mut hunks = Vec::new();
    for hunk in diff.hunks() {
        if hunk.is_pure_insertion() {
            for line in hunk.after.clone() {
                hunks.push(DiffHunk {
                    line,
                    kind: DiffKind::Added,
                });
            }
        } else if hunk.is_pure_removal() {
            for line in hunk.before.clone() {
                hunks.push(DiffHunk {
                    line,
                    kind: DiffKind::Removed,
                });
            }
        } else {
            for line in hunk.after.clone() {
                hunks.push(DiffHunk {
                    line,
                    kind: DiffKind::Modified,
                });
            }
        }
    }

    hunks.sort_by_key(|h| h.line);
    hunks.dedup_by_key(|h| h.line);
    hunks
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_repo() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("tedii-git-test-{}-{unique}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        run_git(&path, &["init", "-q"]);
        run_git(&path, &["config", "user.name", "Tedii Test"]);
        run_git(&path, &["config", "user.email", "tedii@example.com"]);
        path
    }

    fn run_git(repo: &Path, args: &[&str]) {
        let output = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn parses_staged_unstaged_and_dual_state_entries() {
        let root = Path::new("/repo");
        let entries = parse_porcelain_v1(
            b"M  staged.txt\0 M unstaged.txt\0MM both.txt\0?? new.txt\0",
            root,
        );

        assert_eq!(entries.len(), 5);
        assert_eq!(entries[0].section, ChangeSection::Staged);
        assert_eq!(entries[0].status, ChangeStatus::Modified);
        assert_eq!(entries[0].path, root.join("staged.txt"));
        assert_eq!(entries[1].section, ChangeSection::Staged);
        assert_eq!(entries[1].path, root.join("both.txt"));
        assert_eq!(entries[2].section, ChangeSection::Unstaged);
        assert_eq!(entries[2].path, root.join("unstaged.txt"));
        assert_eq!(entries[3].section, ChangeSection::Unstaged);
        assert_eq!(entries[3].path, root.join("both.txt"));
        assert_eq!(entries[4].status, ChangeStatus::Untracked);
    }

    #[test]
    fn parses_deleted_renamed_and_conflicted_entries() {
        let root = Path::new("/repo");
        let entries =
            parse_porcelain_v1(b"D  gone.txt\0R  new.txt\0old.txt\0UU conflict.txt\0", root);

        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].status, ChangeStatus::Deleted);
        assert_eq!(entries[1].status, ChangeStatus::Renamed);
        assert_eq!(entries[1].path, root.join("new.txt"));
        assert_eq!(entries[1].original_path, Some(root.join("old.txt")));
        assert_eq!(entries[2].section, ChangeSection::Unstaged);
        assert_eq!(entries[2].status, ChangeStatus::Conflict);
    }

    #[test]
    fn stages_and_unstages_files_in_an_unborn_repository() {
        let path = temp_repo();
        fs::write(path.join("new.txt"), "new\n").unwrap();
        let repo = GitRepo::discover(&path).unwrap();

        let initial = repo.status().unwrap();
        repo.stage(&initial[0]).unwrap();
        let staged = repo.status().unwrap();
        assert_eq!(staged.len(), 1);
        assert_eq!(staged[0].section, ChangeSection::Staged);

        repo.unstage(&staged[0]).unwrap();
        let unstaged = repo.status().unwrap();
        assert_eq!(unstaged.len(), 1);
        assert_eq!(unstaged[0].section, ChangeSection::Unstaged);
        assert_eq!(unstaged[0].status, ChangeStatus::Untracked);

        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn status_lists_files_inside_untracked_directories() {
        let path = temp_repo();
        fs::create_dir_all(path.join("nested")).unwrap();
        fs::write(path.join("nested/new.txt"), "new\n").unwrap();
        let repo = GitRepo::discover(&path).unwrap();

        let status = repo.status().unwrap();

        assert_eq!(status.len(), 1);
        assert_eq!(status[0].path, path.join("nested/new.txt"));
        assert_eq!(status[0].status, ChangeStatus::Untracked);

        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn commits_staged_changes_and_returns_log_entries() {
        let path = temp_repo();
        fs::write(path.join("tracked.txt"), "first\n").unwrap();
        let repo = GitRepo::discover(&path).unwrap();
        let change = repo.status().unwrap().remove(0);
        repo.stage(&change).unwrap();

        repo.commit("initial commit").unwrap();

        assert!(repo.status().unwrap().is_empty());
        let log = repo.log(10).unwrap();
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].subject, "initial commit");
        assert_eq!(log[0].author, "Tedii Test");
        assert!(!log[0].short_hash.is_empty());

        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn returns_head_to_worktree_and_untracked_diffs() {
        let path = temp_repo();
        fs::write(path.join("tracked.txt"), "first\n").unwrap();
        fs::write(path.join("deleted.txt"), "gone\n").unwrap();
        run_git(&path, &["add", "tracked.txt"]);
        run_git(&path, &["add", "deleted.txt"]);
        run_git(&path, &["commit", "-qm", "initial"]);
        fs::write(path.join("tracked.txt"), "second\n").unwrap();
        fs::write(path.join("new.txt"), "new\n").unwrap();
        fs::remove_file(path.join("deleted.txt")).unwrap();
        let repo = GitRepo::discover(&path).unwrap();

        let tracked = repo.diff_from_head(&path.join("tracked.txt")).unwrap();
        let untracked = repo.diff_from_head(&path.join("new.txt")).unwrap();
        let deleted = repo.diff_from_head(&path.join("deleted.txt")).unwrap();

        assert!(tracked.contains("-first"));
        assert!(tracked.contains("+second"));
        assert!(untracked.contains("+new"));
        assert!(untracked.contains("/dev/null"));
        assert!(deleted.contains("-gone"));

        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn unstages_both_sides_of_a_staged_rename() {
        let path = temp_repo();
        fs::write(path.join("old.txt"), "content\n").unwrap();
        run_git(&path, &["add", "old.txt"]);
        run_git(&path, &["commit", "-qm", "initial"]);
        run_git(&path, &["mv", "old.txt", "new.txt"]);
        let repo = GitRepo::discover(&path).unwrap();
        let rename = repo
            .status()
            .unwrap()
            .into_iter()
            .find(|change| change.status == ChangeStatus::Renamed)
            .unwrap();

        repo.unstage(&rename).unwrap();

        let status = repo.status().unwrap();
        assert!(status
            .iter()
            .all(|change| change.section == ChangeSection::Unstaged));
        assert!(status.iter().any(|change| {
            change.path == path.join("old.txt") && change.status == ChangeStatus::Deleted
        }));
        assert!(status.iter().any(|change| {
            change.path == path.join("new.txt") && change.status == ChangeStatus::Untracked
        }));

        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn discovers_repository_from_a_bare_relative_file_name() {
        assert!(GitRepo::discover(Path::new("Cargo.toml")).is_some());
    }
}
