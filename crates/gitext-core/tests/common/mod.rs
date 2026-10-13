#![allow(dead_code)]

use gitext_core::Git;
use tempfile::TempDir;

/// A fresh repository in a temp dir, with a local committer identity.
pub async fn repo() -> (TempDir, Git) {
    let dir = tempfile::tempdir().unwrap();
    let git = Git::init(dir.path()).await.unwrap();
    git.run(&["config", "user.name", "Test"]).await.unwrap();
    git.run(&["config", "user.email", "test@example.com"])
        .await
        .unwrap();
    (dir, git)
}
