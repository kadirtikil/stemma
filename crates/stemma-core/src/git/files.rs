//! The files that make up the paper.

use super::Git;
use crate::error::Result;

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
}
