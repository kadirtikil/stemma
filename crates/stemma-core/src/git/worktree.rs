//! Linked worktrees: several drafts checked out side by side.
//!
//! Each worktree is its own directory sharing one repository, so a writer
//! can keep the submitted version open next to the draft they're revising.

use std::path::{Path, PathBuf};

use super::Git;
use super::refs::check_name;
use crate::error::{GitError, Result};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Worktree {
    pub path: PathBuf,
    /// Checked-out commit, or `None` for a bare repository.
    pub head: Option<String>,
    /// Checked-out branch, or `None` when detached.
    pub branch: Option<String>,
    /// The repository's original work tree, which can't be removed.
    pub is_main: bool,
    pub is_bare: bool,
    pub is_locked: bool,
    /// The directory is gone and `prune_worktrees` will clean it up.
    pub is_prunable: bool,
}

/// What a new worktree checks out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Checkout {
    /// An existing branch.
    Branch(String),
    /// A new branch starting at `from`.
    NewBranch { name: String, from: String },
    /// A detached commit, for read-only views like a tagged submission.
    Detached(String),
}

impl Git {
    /// All worktrees, the main one first.
    pub async fn worktrees(&self) -> Result<Vec<Worktree>> {
        parse(&self.run(&["worktree", "list", "--porcelain", "-z"]).await?)
    }

    /// Creates a worktree at `path` and returns a handle to it.
    pub async fn add_worktree(&self, path: impl AsRef<Path>, checkout: &Checkout) -> Result<Git> {
        let path = path.as_ref();
        let path_str = utf8(path)?;
        let mut args = vec!["worktree", "add", "-q"];
        match checkout {
            Checkout::Branch(name) => {
                check_name(name)?;
                args.extend(["--", path_str, name]);
            }
            Checkout::NewBranch { name, from } => {
                check_name(name)?;
                check_name(from)?;
                args.extend(["-b", name, "--", path_str, from]);
            }
            Checkout::Detached(rev) => {
                check_name(rev)?;
                args.extend(["--detach", "--", path_str, rev]);
            }
        }
        self.run(&args).await?;
        Git::discover(path).await
    }

    /// Removes the worktree at `path`. Without `force`, git refuses if it
    /// has uncommitted changes, so unsaved writing is never thrown away.
    pub async fn remove_worktree(&self, path: impl AsRef<Path>, force: bool) -> Result<()> {
        let path = utf8(path.as_ref())?;
        let mut args = vec!["worktree", "remove"];
        if force {
            args.push("--force");
        }
        args.extend(["--", path]);
        self.run(&args).await.map(drop)
    }

    /// Forgets worktrees whose directories were deleted by hand.
    pub async fn prune_worktrees(&self) -> Result<()> {
        self.run(&["worktree", "prune"]).await.map(drop)
    }
}

fn utf8(path: &Path) -> Result<&str> {
    path.to_str()
        .ok_or_else(|| GitError::Parse(format!("path is not UTF-8: {}", path.display())))
}

/// Records are NUL-separated attribute lines, terminated by an empty one.
fn parse(out: &[u8]) -> Result<Vec<Worktree>> {
    let mut worktrees: Vec<Worktree> = Vec::new();
    let mut current: Option<Worktree> = None;

    for field in out.split(|&b| b == 0).map(String::from_utf8_lossy) {
        if field.is_empty() {
            worktrees.extend(current.take());
            continue;
        }
        let (key, value) = field.split_once(' ').unwrap_or((&field, ""));
        if key == "worktree" {
            worktrees.extend(current.take());
            current = Some(Worktree {
                path: value.into(),
                is_main: worktrees.is_empty(),
                ..Worktree::default()
            });
            continue;
        }
        let wt = current
            .as_mut()
            .ok_or_else(|| GitError::Parse(format!("bad worktree field {field:?}")))?;
        match key {
            "HEAD" => wt.head = Some(value.to_owned()),
            "branch" => {
                wt.branch = Some(
                    value
                        .strip_prefix("refs/heads/")
                        .unwrap_or(value)
                        .to_owned(),
                )
            }
            "bare" => wt.is_bare = true,
            "locked" => wt.is_locked = true,
            "prunable" => wt.is_prunable = true,
            _ => {}
        }
    }
    worktrees.extend(current);
    Ok(worktrees)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_porcelain() {
        let out = b"worktree /papers/main\0HEAD aaa\0branch refs/heads/main\0\0\
                    worktree /papers/response draft\0HEAD bbb\0branch refs/heads/reviewer-2\0locked moving disks\0\0\
                    worktree /papers/submitted\0HEAD ccc\0detached\0prunable gitdir file points to non-existent location\0\0";
        let wts = parse(out).unwrap();
        assert_eq!(
            wts,
            [
                Worktree {
                    path: "/papers/main".into(),
                    head: Some("aaa".into()),
                    branch: Some("main".into()),
                    is_main: true,
                    ..Worktree::default()
                },
                Worktree {
                    path: "/papers/response draft".into(),
                    head: Some("bbb".into()),
                    branch: Some("reviewer-2".into()),
                    is_locked: true,
                    ..Worktree::default()
                },
                Worktree {
                    path: "/papers/submitted".into(),
                    head: Some("ccc".into()),
                    is_prunable: true,
                    ..Worktree::default()
                },
            ]
        );
    }

    #[test]
    fn parses_bare() {
        let wts = parse(b"worktree /papers.git\0bare\0\0").unwrap();
        assert!(wts[0].is_bare && wts[0].is_main && wts[0].head.is_none());
    }
}
