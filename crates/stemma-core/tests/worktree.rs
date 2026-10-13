mod common;

use std::path::Path;

use stemma_core::git::Checkout;

fn same(a: &Path, b: &Path) -> bool {
    a.canonicalize().unwrap() == b.canonicalize().unwrap()
}

/// A repo with one commit on `main` and a `submitted-v1` tag.
async fn paper() -> (tempfile::TempDir, stemma_core::Git) {
    let (dir, git) = common::repo().await;
    std::fs::write(dir.path().join("main.typ"), "Submitted text\n").unwrap();
    git.stage_all().await.unwrap();
    git.commit("Submit").await.unwrap();
    git.tag("submitted-v1", "Submitted").await.unwrap();
    (dir, git)
}

#[tokio::test]
async fn new_branch_worktree_is_independent() {
    let (dir, git) = paper().await;
    let others = tempfile::tempdir().unwrap();
    let path = others.path().join("response draft");

    let draft = git
        .add_worktree(
            &path,
            &Checkout::NewBranch {
                name: "reviewer-2".into(),
                from: "main".into(),
            },
        )
        .await
        .unwrap();
    assert!(same(draft.root(), &path));

    // Saving in the draft leaves the main work tree untouched.
    std::fs::write(path.join("main.typ"), "Revised text\n").unwrap();
    draft.stage_all().await.unwrap();
    draft.commit("Address reviewer 2").await.unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.path().join("main.typ")).unwrap(),
        "Submitted text\n"
    );
    assert!(git.status().await.unwrap().is_clean());

    let wts = git.worktrees().await.unwrap();
    assert_eq!(wts.len(), 2);
    assert!(wts[0].is_main && same(&wts[0].path, dir.path()));
    assert_eq!(wts[1].branch.as_deref(), Some("reviewer-2"));

    git.remove_worktree(&path, false).await.unwrap();
    assert_eq!(git.worktrees().await.unwrap().len(), 1);
    assert!(!path.exists());
}

#[tokio::test]
async fn detached_worktree_shows_a_submission() {
    let (dir, git) = paper().await;
    std::fs::write(dir.path().join("main.typ"), "Newer text\n").unwrap();
    git.stage_all().await.unwrap();
    git.commit("Keep writing").await.unwrap();

    let others = tempfile::tempdir().unwrap();
    let path = others.path().join("v1");
    git.add_worktree(&path, &Checkout::Detached("submitted-v1".into()))
        .await
        .unwrap();

    assert_eq!(
        std::fs::read_to_string(path.join("main.typ")).unwrap(),
        "Submitted text\n"
    );
    let wts = git.worktrees().await.unwrap();
    assert_eq!(wts[1].branch, None);
}

#[tokio::test]
async fn remove_refuses_unsaved_work_unless_forced() {
    let (_dir, git) = paper().await;
    let others = tempfile::tempdir().unwrap();
    let path = others.path().join("scratch");
    git.add_worktree(
        &path,
        &Checkout::NewBranch {
            name: "scratch".into(),
            from: "main".into(),
        },
    )
    .await
    .unwrap();
    std::fs::write(path.join("main.typ"), "Unsaved\n").unwrap();

    assert!(git.remove_worktree(&path, false).await.is_err());
    git.remove_worktree(&path, true).await.unwrap();
    assert!(!path.exists());
}

#[tokio::test]
async fn deleted_worktree_is_pruned() {
    let (_dir, git) = paper().await;
    let others = tempfile::tempdir().unwrap();
    let path = others.path().join("gone");
    git.add_worktree(&path, &Checkout::Detached("HEAD".into()))
        .await
        .unwrap();

    std::fs::remove_dir_all(&path).unwrap();
    assert!(git.worktrees().await.unwrap()[1].is_prunable);

    git.prune_worktrees().await.unwrap();
    assert_eq!(git.worktrees().await.unwrap().len(), 1);
}

#[tokio::test]
async fn existing_branch_and_bad_names() {
    let (_dir, git) = paper().await;
    git.create_branch("other").await.unwrap();
    git.switch("main").await.unwrap();

    let others = tempfile::tempdir().unwrap();
    let wt = git
        .add_worktree(
            others.path().join("other"),
            &Checkout::Branch("other".into()),
        )
        .await
        .unwrap();
    assert_eq!(wt.status().await.unwrap().branch.as_deref(), Some("other"));

    let bad = Checkout::Branch("--force".into());
    assert!(
        git.add_worktree(others.path().join("x"), &bad)
            .await
            .is_err()
    );
}
