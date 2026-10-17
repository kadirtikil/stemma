mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use stemma_core::git::Checkout;
use stemma_core::service::{self, Command, Event};
use tokio::runtime::Handle;

#[tokio::test]
async fn commands_run_in_order_and_report_status() {
    let (dir, git) = common::repo().await;
    std::fs::write(dir.path().join("main.typ"), "= Title\n").unwrap();

    let notified = Arc::new(AtomicUsize::new(0));
    let counter = notified.clone();
    let (handle, mut events) = service::spawn(git, &Handle::current(), move || {
        counter.fetch_add(1, Ordering::SeqCst);
    });

    assert!(handle.send(Command::StageAll));
    assert!(handle.send(Command::Commit {
        message: "Start paper".into(),
    }));
    assert!(handle.send(Command::Log {
        path: None,
        limit: 10,
    }));

    let Some(Event::Status(staged)) = events.recv().await else {
        panic!("expected status after staging");
    };
    assert_eq!(staged.files.len(), 1);

    let Some(Event::Committed(oid)) = events.recv().await else {
        panic!("expected commit");
    };
    let Some(Event::Status(after)) = events.recv().await else {
        panic!("expected status after commit");
    };
    assert!(after.is_clean());

    let Some(Event::Log(log)) = events.recv().await else {
        panic!("expected log");
    };
    assert_eq!(log[0].oid, oid);
    assert_eq!(notified.load(Ordering::SeqCst), 4);
}

#[tokio::test]
async fn failures_become_error_events() {
    let (_dir, git) = common::repo().await;
    let (handle, mut events) = service::spawn(git, &Handle::current(), || {});

    handle.send(Command::Commit {
        message: "Nothing staged".into(),
    });
    assert!(matches!(events.recv().await, Some(Event::Error(_))));
}

#[tokio::test]
async fn actor_stops_when_handles_drop() {
    let (_dir, git) = common::repo().await;
    let (handle, mut events) = service::spawn(git, &Handle::current(), || {});
    drop(handle);
    assert!(events.recv().await.is_none());
}

#[tokio::test]
async fn added_worktree_gets_its_own_actor() {
    let (dir, git) = common::repo().await;
    std::fs::write(dir.path().join("main.typ"), "Text\n").unwrap();
    git.stage_all().await.unwrap();
    git.commit("Start").await.unwrap();

    let (handle, mut events) = service::spawn(git, &Handle::current(), || {});
    let others = tempfile::tempdir().unwrap();
    let path = others.path().join("response");
    handle.send(Command::AddWorktree {
        path: path.clone(),
        checkout: Checkout::NewBranch {
            name: "response".into(),
            from: "main".into(),
        },
    });

    let Some(Event::WorktreeAdded(draft)) = events.recv().await else {
        panic!("expected new worktree");
    };
    let Some(Event::Worktrees(wts)) = events.recv().await else {
        panic!("expected worktree list");
    };
    assert_eq!(wts.len(), 2);

    let (draft_handle, mut draft_events) = service::spawn(draft, &Handle::current(), || {});
    draft_handle.send(Command::Refresh);
    let Some(Event::Status(s)) = draft_events.recv().await else {
        panic!("expected draft status");
    };
    assert_eq!(s.branch.as_deref(), Some("response"));

    handle.send(Command::RemoveWorktree { path, force: false });
    let Some(Event::Worktrees(wts)) = events.recv().await else {
        panic!("expected worktree list after removal");
    };
    assert_eq!(wts.len(), 1);
}

#[tokio::test]
async fn writes_files_and_reads_them_back() {
    let (_dir, git) = common::repo().await;
    let (handle, mut events) = service::spawn(git, &Handle::current(), || {});

    handle.send(Command::Write {
        path: "main.typ".into(),
        contents: "= Title\n".into(),
    });
    handle.send(Command::Read("main.typ".into()));

    assert!(matches!(events.recv().await, Some(Event::Written(p)) if p == "main.typ"));
    let Some(Event::Status(status)) = events.recv().await else {
        panic!("expected status after writing");
    };
    assert_eq!(status.files.len(), 1);
    let Some(Event::Opened { path, contents }) = events.recv().await else {
        panic!("expected file contents");
    };
    assert_eq!(path, "main.typ");
    assert_eq!(contents.as_deref(), Some("= Title\n"));
}
