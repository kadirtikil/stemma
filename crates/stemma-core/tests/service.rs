mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

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
