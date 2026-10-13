//! Staging and committing.

use super::Git;
use crate::error::Result;

impl Git {
    /// Stages `paths`, including deletions. Paths are relative to the repo root.
    pub async fn stage(&self, paths: &[&str]) -> Result<()> {
        let mut args = vec!["add", "-A", "--"];
        args.extend_from_slice(paths);
        self.run(&args).await.map(drop)
    }

    /// Stages every change in the work tree.
    pub async fn stage_all(&self) -> Result<()> {
        self.run(&["add", "-A"]).await.map(drop)
    }

    /// Commits the index and returns the new commit's hash.
    pub async fn commit(&self, message: &str) -> Result<String> {
        self.run(&["commit", "-q", "-m", message]).await?;
        self.rev_parse("HEAD").await
    }

    /// Resolves a revision to a full commit hash.
    pub async fn rev_parse(&self, rev: &str) -> Result<String> {
        let out = self
            .run(&["rev-parse", "--verify", "--end-of-options", rev])
            .await?;
        Ok(String::from_utf8_lossy(&out).trim_end().to_owned())
    }
}
