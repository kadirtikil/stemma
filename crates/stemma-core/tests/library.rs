use stemma_core::git::Checkout;
use stemma_core::{GitError, Library};

#[tokio::test]
async fn creates_the_library_directory() {
    let dir = tempfile::tempdir().unwrap();
    let library = Library::open(dir.path().join("Stemma")).await.unwrap();

    assert!(library.root().is_dir());
    assert!(library.papers().await.unwrap().is_empty());
}

#[tokio::test]
async fn new_papers_are_repos_with_a_main_file() {
    let dir = tempfile::tempdir().unwrap();
    let library = Library::open(dir.path()).await.unwrap();

    let git = library.create("  Thesis ").await.unwrap();
    library.create("Alpha draft").await.unwrap();

    assert_eq!(git.files().await.unwrap(), ["main.typ"]);
    assert_eq!(
        git.read("main.typ").await.unwrap().as_deref(),
        Some("= Thesis\n\n")
    );
    let names: Vec<_> = library
        .papers()
        .await
        .unwrap()
        .into_iter()
        .map(|p| p.name)
        .collect();
    assert_eq!(names, ["Alpha draft", "Thesis"]);
}

#[tokio::test]
async fn skips_plain_directories_and_linked_worktrees() {
    let dir = tempfile::tempdir().unwrap();
    let library = Library::open(dir.path()).await.unwrap();
    let git = library.create("Thesis").await.unwrap();
    git.run(&["config", "user.name", "Test"]).await.unwrap();
    git.run(&["config", "user.email", "test@example.com"])
        .await
        .unwrap();
    git.stage_all().await.unwrap();
    git.commit("Start").await.unwrap();

    std::fs::create_dir(dir.path().join("notes")).unwrap();
    std::fs::write(dir.path().join("todo.txt"), "x").unwrap();
    git.add_worktree(
        &dir.path().join("Thesis revision"),
        &Checkout::NewBranch {
            name: "revision".into(),
            from: "main".into(),
        },
    )
    .await
    .unwrap();

    let papers = library.papers().await.unwrap();
    assert_eq!(papers.len(), 1);
    assert_eq!(papers[0].name, "Thesis");
}

#[tokio::test]
async fn refuses_bad_and_duplicate_names() {
    let dir = tempfile::tempdir().unwrap();
    let library = Library::open(dir.path()).await.unwrap();
    library.create("Thesis").await.unwrap();

    for name in ["", "  ", ".hidden", "a/b", "..", "a\\b"] {
        assert!(
            matches!(
                library.create(name).await,
                Err(GitError::InvalidPaperName(_))
            ),
            "{name:?} should be refused"
        );
    }
    assert!(matches!(
        library.create("Thesis").await,
        Err(GitError::PaperExists(_))
    ));
}
