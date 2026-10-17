//! Thin async wrapper around the `git` CLI.

mod commit;
mod diff;
mod files;
mod log;
mod refs;
mod status;
mod worktree;

pub use diff::{FileDiff, Hunk, Segment};
pub use log::Commit;
pub use refs::{Branch, Tag};
pub use status::{Change, FileStatus, Status};
pub use worktree::{Checkout, Worktree};

use std::path::{Path, PathBuf};
use std::process::Stdio;

use tokio::process::Command;

use crate::error::{GitError, Result};

/// A handle to one repository, identified by its work-tree root.
#[derive(Debug, Clone)]
pub struct Git {
    repo: PathBuf,
    bin: PathBuf,
}

impl Git {
    /// Opens the repository containing `path`.
    pub async fn discover(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let out = run_in(Path::new("git"), path, &["rev-parse", "--show-toplevel"])
            .await
            .map_err(|e| match e {
                GitError::Command { .. } => GitError::NotARepo(path.to_owned()),
                e => e,
            })?;
        let root = String::from_utf8(out)
            .map_err(|e| GitError::Parse(e.to_string()))?
            .trim_end()
            .into();
        Ok(Self::at(root))
    }

    /// Creates a new repository at `path` with `main` as the initial branch.
    pub async fn init(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        tokio::fs::create_dir_all(path).await?;
        run_in(Path::new("git"), path, &["init", "-q", "-b", "main"]).await?;
        Self::discover(path).await
    }

    /// Returns the installed git version string, e.g. `git version 2.54.0`.
    pub async fn version() -> Result<String> {
        let out = run_in(Path::new("git"), Path::new("."), &["--version"]).await?;
        Ok(String::from_utf8_lossy(&out).trim_end().to_owned())
    }

    fn at(repo: PathBuf) -> Self {
        Self {
            repo,
            bin: PathBuf::from("git"),
        }
    }

    pub fn root(&self) -> &Path {
        &self.repo
    }

    /// Runs `git <args>` in the repository and returns stdout.
    pub async fn run(&self, args: &[&str]) -> Result<Vec<u8>> {
        run_in(&self.bin, &self.repo, args).await
    }
}

async fn run_in(bin: &Path, dir: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let output = Command::new(bin)
        .args([
            "--no-pager",
            "-c",
            "core.quotepath=off",
            "-c",
            "color.ui=never",
        ])
        .args(args)
        .current_dir(dir)
        .env("LC_ALL", "C")
        // Never block on an interactive credential prompt.
        .env("GIT_TERMINAL_PROMPT", "0")
        // Don't contend with the user's own git for index.lock on reads.
        .env("GIT_OPTIONAL_LOCKS", "0")
        .stdin(Stdio::null())
        .kill_on_drop(true)
        .output()
        .await
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => GitError::NotInstalled,
            _ => GitError::Io(e),
        })?;

    if !output.status.success() {
        return Err(GitError::Command {
            args: args.join(" "),
            code: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr)
                .trim_end()
                .to_owned(),
        });
    }
    Ok(output.stdout)
}
