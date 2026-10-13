mod common;

use stemma_core::git::Segment;

#[tokio::test]
async fn word_diff_and_show_old_version() {
    let (dir, git) = common::repo().await;
    let path = dir.path().join("main.typ");

    std::fs::write(&path, "We show that the method converges quickly.\n").unwrap();
    git.stage_all().await.unwrap();
    let v1 = git.commit("Draft claim").await.unwrap();

    std::fs::write(&path, "We prove that the method converges quickly.\n").unwrap();

    // Work tree against the last commit.
    let diff = git.word_diff("HEAD", None, Some("main.typ")).await.unwrap();
    assert_eq!(diff.len(), 1);
    let segs = &diff[0].hunks[0].segments;
    assert!(segs.contains(&Segment::Removed("show".into())), "{segs:?}");
    assert!(segs.contains(&Segment::Added("prove".into())), "{segs:?}");

    // Between two commits.
    git.stage_all().await.unwrap();
    let v2 = git.commit("Strengthen claim").await.unwrap();
    let diff = git.word_diff(&v1, Some(&v2), None).await.unwrap();
    assert_eq!(diff[0].new_path.as_deref(), Some("main.typ"));

    assert_eq!(
        git.show(&v1, "main.typ").await.unwrap(),
        "We show that the method converges quickly.\n"
    );
    assert!(git.word_diff("HEAD", None, None).await.unwrap().is_empty());
}
