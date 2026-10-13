mod common;

#[tokio::test]
async fn drafts_are_branches() {
    let (dir, git) = common::repo().await;
    let path = dir.path().join("main.typ");
    std::fs::write(&path, "Draft one\n").unwrap();
    git.stage_all().await.unwrap();
    git.commit("Start").await.unwrap();

    git.create_branch("reviewer-2-response").await.unwrap();
    std::fs::write(&path, "Draft two\n").unwrap();
    git.stage_all().await.unwrap();
    git.commit("Address reviewer 2").await.unwrap();

    let branches = git.branches().await.unwrap();
    let names: Vec<_> = branches
        .iter()
        .map(|b| (b.name.as_str(), b.is_current))
        .collect();
    assert_eq!(names, [("main", false), ("reviewer-2-response", true)]);

    git.switch("main").await.unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "Draft one\n");
    assert_eq!(git.status().await.unwrap().branch.as_deref(), Some("main"));
}

#[tokio::test]
async fn submissions_are_annotated_tags() {
    let (dir, git) = common::repo().await;
    std::fs::write(dir.path().join("main.typ"), "v1\n").unwrap();
    git.stage_all().await.unwrap();
    let oid = git.commit("Ready").await.unwrap();

    git.tag("submitted-neurips", "Submitted to NeurIPS\n\nPaper ID 1234")
        .await
        .unwrap();

    let tags = git.tags().await.unwrap();
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].name, "submitted-neurips");
    assert_eq!(tags[0].target, oid);
    assert_eq!(tags[0].subject, "Submitted to NeurIPS");
}

#[tokio::test]
async fn dash_names_are_rejected() {
    let (_dir, git) = common::repo().await;
    assert!(git.create_branch("-D").await.is_err());
    assert!(git.switch("--orphan").await.is_err());
}
