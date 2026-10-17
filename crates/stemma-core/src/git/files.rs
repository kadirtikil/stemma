//! The files that make up the paper.

use std::path::{Component, Path, PathBuf};

use super::Git;
use crate::error::{GitError, Result};

impl Git {
    /// Tracked and untracked files, sorted, honoring `.gitignore`.
    /// Paths are relative to the repo root.
    pub async fn files(&self) -> Result<Vec<String>> {
        let out = self
            .run(&[
                "ls-files",
                "-z",
                "--cached",
                "--others",
                "--exclude-standard",
                "--deduplicate",
            ])
            .await?;
        let mut files: Vec<_> = out
            .split(|&b| b == 0)
            .filter(|p| !p.is_empty())
            .map(|p| String::from_utf8_lossy(p).into_owned())
            .collect();
        files.sort();
        files.dedup();
        Ok(files)
    }

    /// Reads a work-tree file, or `None` if it isn't UTF-8 text (an image,
    /// say). `path` is relative to the repo root.
    pub async fn read(&self, path: &str) -> Result<Option<String>> {
        let bytes = tokio::fs::read(self.resolve(path)?).await?;
        Ok(String::from_utf8(bytes).ok())
    }

    /// Writes a work-tree file, creating it if needed. `path` is relative to
    /// the repo root.
    pub async fn write(&self, path: &str, contents: &str) -> Result<()> {
        Ok(tokio::fs::write(self.resolve(path)?, contents).await?)
    }

    /// Joins `path` onto the root, refusing paths that could escape it.
    fn resolve(&self, path: &str) -> Result<PathBuf> {
        let inside = Path::new(path)
            .components()
            .all(|c| matches!(c, Component::Normal(_)));
        if path.is_empty() || !inside {
            return Err(GitError::OutsideRepo(path.to_owned()));
        }
        Ok(self.root().join(path))
    }
}
