use std::path::{Path, PathBuf};

use gix::bstr::ByteSlice;
use gix::diff::Rewrites;
use gix::dir::entry::Status;
use gix::objs::tree::EntryKind;
use gix::status::index_worktree::Item;
use gix::status::plumbing::index_as_worktree::{Change, EntryStatus};
use gix::status::UntrackedFiles;
use gix::{Repository, ThreadSafeRepository};

use imara_diff::{Algorithm, Diff, InternedInput};

#[derive(Debug, Clone)]
pub enum FileChange {
    Modified { path: PathBuf },
    Untracked { path: PathBuf },
    Deleted { path: PathBuf },
    Renamed { from_path: PathBuf, to_path: PathBuf },
    Conflict { path: PathBuf },
}

impl FileChange {
    pub fn path(&self) -> &Path {
        match self {
            Self::Modified { path }
            | Self::Untracked { path }
            | Self::Deleted { path }
            | Self::Conflict { path } => path,
            Self::Renamed { to_path, .. } => to_path,
        }
    }

    pub fn label(&self) -> &str {
        match self {
            Self::Modified { .. } => "M",
            Self::Untracked { .. } => "?",
            Self::Deleted { .. } => "D",
            Self::Renamed { .. } => "R",
            Self::Conflict { .. } => "C",
        }
    }
}

pub struct GitRepo {
    repo: Repository,
}

impl GitRepo {
    pub fn discover(file: &Path) -> Option<Self> {
        let dir = if file.is_dir() {
            file
        } else {
            file.parent()?
        };
        let repo = ThreadSafeRepository::discover(dir).ok()?;
        Some(Self {
            repo: repo.to_thread_local(),
        })
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

    pub fn status(&self) -> Vec<FileChange> {
        let work_dir = match self.repo.workdir() {
            Some(d) => d.to_path_buf(),
            None => return Vec::new(),
        };

        let platform = match self.repo.status(gix::progress::Discard) {
            Ok(p) => p
                .untracked_files(UntrackedFiles::Files)
                .index_worktree_rewrites(Some(Rewrites {
                    copies: None,
                    percentage: Some(0.5),
                    limit: 1000,
                    ..Default::default()
                })),
            Err(_) => return Vec::new(),
        };

        let empty_patterns = vec![];
        let iter = match platform.into_index_worktree_iter(empty_patterns) {
            Ok(i) => i,
            Err(_) => return Vec::new(),
        };

        let mut changes = Vec::new();
        for item in iter {
            let Ok(item) = item else { continue };
            let change = match item {
                Item::Modification {
                    rela_path, status, ..
                } => {
                    let path = match rela_path.to_path() {
                        Ok(p) => work_dir.join(p),
                        Err(_) => continue,
                    };
                    match status {
                        EntryStatus::Conflict { .. } => FileChange::Conflict { path },
                        EntryStatus::Change(Change::Removed) => FileChange::Deleted { path },
                        EntryStatus::Change(Change::Modification { .. }) => {
                            FileChange::Modified { path }
                        }
                        EntryStatus::IntentToAdd => FileChange::Untracked { path },
                        _ => continue,
                    }
                }
                Item::DirectoryContents { entry, .. } if entry.status == Status::Untracked => {
                    let path = match entry.rela_path.to_path() {
                        Ok(p) => work_dir.join(p),
                        Err(_) => continue,
                    };
                    FileChange::Untracked { path }
                }
                Item::Rewrite {
                    source,
                    dirwalk_entry,
                    ..
                } => {
                    let from_path = match source.rela_path().to_path() {
                        Ok(p) => work_dir.join(p),
                        Err(_) => continue,
                    };
                    let to_path = match dirwalk_entry.rela_path.to_path() {
                        Ok(p) => work_dir.join(p),
                        Err(_) => continue,
                    };
                    FileChange::Renamed { from_path, to_path }
                }
                _ => continue,
            };
            changes.push(change);
        }
        changes
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
