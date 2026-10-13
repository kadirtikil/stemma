mod common;

use stemma_core::{Git, GitError};

#[tokio::test]
async fn version_reports_git() {
    assert!(Git::version().await.unwrap().starts_with("git version"));
}

#[tokio::test]
async fn init_then_discover_from_subdir() {
    let (dir, git) = common::repo().await;
    let sub = dir.path().join("chapters");
    std::fs::create_dir(&sub).unwrap();

    let found = Git::discover(&sub).await.unwrap();
    assert_eq!(
        found.root().canonicalize().unwrap(),
        git.root().canonicalize().unwrap()
    );
}

#[tokio::test]
async fn discover_outside_repo_is_not_a_repo() {
    let dir = tempfile::tempdir().unwrap();
    let err = Git::discover(dir.path()).await.unwrap_err();
    assert!(matches!(err, GitError::NotARepo(_)), "{err:?}");
}

#[tokio::test]
async fn failed_command_carries_stderr() {
    let (_dir, git) = common::repo().await;
    match git.run(&["rev-parse", "HEAD"]).await.unwrap_err() {
        GitError::Command { stderr, .. } => assert!(!stderr.is_empty()),
        e => panic!("unexpected {e:?}"),
    }
}
