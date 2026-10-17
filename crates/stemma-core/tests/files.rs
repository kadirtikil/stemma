mod common;

#[tokio::test]
async fn lists_tracked_and_new_files_but_not_ignored() {
    let (dir, git) = common::repo().await;
    let write = |name: &str| {
        let path = dir.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "x\n").unwrap();
    };
    write("main.typ");
    write(".gitignore");
    std::fs::write(dir.path().join(".gitignore"), "*.pdf\n").unwrap();
    git.stage_all().await.unwrap();
    git.commit("Start").await.unwrap();

    write("chapters/intro.typ");
    write("main.pdf");
    // Modified tracked files are listed once.
    std::fs::write(dir.path().join("main.typ"), "changed\n").unwrap();

    assert_eq!(
        git.files().await.unwrap(),
        [".gitignore", "chapters/intro.typ", "main.typ"]
    );
}
