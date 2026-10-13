mod common;

#[tokio::test]
async fn empty_repo_has_no_history() {
    let (_dir, git) = common::repo().await;
    assert!(git.log(None, 10).await.unwrap().is_empty());
}

#[tokio::test]
async fn log_is_newest_first_and_filters_by_path() {
    let (dir, git) = common::repo().await;
    let write = |name: &str, text: &str| std::fs::write(dir.path().join(name), text).unwrap();

    write("main.typ", "= Title\n");
    git.stage_all().await.unwrap();
    let first = git.commit("Start paper").await.unwrap();

    write("refs.bib", "@book{x}\n");
    git.stage_all().await.unwrap();
    git.commit("Add references").await.unwrap();

    write("main.typ", "= Better title\n");
    git.stage_all().await.unwrap();
    let third = git
        .commit("Retitle\n\nThe old one was vague.")
        .await
        .unwrap();

    let log = git.log(None, 10).await.unwrap();
    let summaries: Vec<_> = log.iter().map(|c| c.summary.as_str()).collect();
    assert_eq!(summaries, ["Retitle", "Add references", "Start paper"]);
    assert_eq!(log[0].body, "The old one was vague.");
    assert_eq!(log[0].author_name, "Test");

    let main: Vec<_> = git
        .log(Some("main.typ"), 10)
        .await
        .unwrap()
        .into_iter()
        .map(|c| c.oid)
        .collect();
    assert_eq!(main, [third, first]);

    assert_eq!(git.log(None, 1).await.unwrap().len(), 1);
}
