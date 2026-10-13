mod common;

#[tokio::test]
async fn commit_leaves_tree_clean() {
    let (dir, git) = common::repo().await;
    std::fs::write(dir.path().join("main.typ"), "= Title\n").unwrap();

    git.stage(&["main.typ"]).await.unwrap();
    let oid = git.commit("Start paper").await.unwrap();

    let s = git.status().await.unwrap();
    assert!(s.is_clean());
    assert_eq!(s.head.as_deref(), Some(oid.as_str()));
    assert_eq!(oid.len(), 40);
}

#[tokio::test]
async fn stage_all_includes_deletions() {
    let (dir, git) = common::repo().await;
    std::fs::write(dir.path().join("draft.typ"), "x\n").unwrap();
    git.stage_all().await.unwrap();
    git.commit("Add draft").await.unwrap();

    std::fs::remove_file(dir.path().join("draft.typ")).unwrap();
    git.stage_all().await.unwrap();
    git.commit("Remove draft").await.unwrap();

    assert!(git.status().await.unwrap().is_clean());
}

#[tokio::test]
async fn commit_with_nothing_staged_fails() {
    let (_dir, git) = common::repo().await;
    assert!(git.commit("Empty").await.is_err());
}
