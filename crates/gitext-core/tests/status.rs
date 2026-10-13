mod common;

use gitext_core::git::Change;

#[tokio::test]
async fn new_repo_is_clean_on_main() {
    let (_dir, git) = common::repo().await;
    let s = git.status().await.unwrap();
    assert!(s.is_clean());
    assert_eq!(s.branch.as_deref(), Some("main"));
    assert_eq!(s.head, None);
}

#[tokio::test]
async fn new_file_is_untracked() {
    let (dir, git) = common::repo().await;
    std::fs::create_dir(dir.path().join("chapters")).unwrap();
    std::fs::write(dir.path().join("chapters/intro.typ"), "= Intro\n").unwrap();

    let s = git.status().await.unwrap();
    assert_eq!(s.files.len(), 1);
    assert_eq!(s.files[0].path, "chapters/intro.typ");
    assert_eq!(s.files[0].unstaged, Change::Untracked);
}
