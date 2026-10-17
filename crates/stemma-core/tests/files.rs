mod common;

use stemma_core::GitError;

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

#[tokio::test]
async fn writes_and_reads_back_text() {
    let (dir, git) = common::repo().await;
    git.write("main.typ", "= Title\n").await.unwrap();

    assert_eq!(
        std::fs::read_to_string(dir.path().join("main.typ")).unwrap(),
        "= Title\n"
    );
    assert_eq!(
        git.read("main.typ").await.unwrap().as_deref(),
        Some("= Title\n")
    );
}

#[tokio::test]
async fn binary_files_read_as_none() {
    let (dir, git) = common::repo().await;
    std::fs::write(
        dir.path().join("figure.png"),
        [0x89, b'P', b'N', b'G', 0xff],
    )
    .unwrap();

    assert_eq!(git.read("figure.png").await.unwrap(), None);
}

#[tokio::test]
async fn refuses_paths_outside_the_repo() {
    let (_dir, git) = common::repo().await;

    for path in ["../escape.typ", "/etc/passwd", "a/../../b", ""] {
        assert!(
            matches!(git.read(path).await, Err(GitError::OutsideRepo(_))),
            "{path} should be refused"
        );
        assert!(matches!(
            git.write(path, "x").await,
            Err(GitError::OutsideRepo(_))
        ));
    }
}
