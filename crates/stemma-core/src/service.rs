//! Bridges the synchronous UI to async git.
//!
//! The UI never awaits. It sends [`Command`]s through a [`GitHandle`] and
//! drains [`Event`]s with `try_recv` each frame. A single actor task runs
//! commands one at a time, since git serializes writes on `index.lock`
//! anyway.

use std::path::PathBuf;

use tokio::runtime::Handle;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

use crate::error::{GitError, Result};
use crate::git::{Branch, Checkout, Commit, FileDiff, Git, Status, Tag, Worktree};

#[derive(Debug, Clone)]
pub enum Command {
    Refresh,
    Files,
    /// Reads a work-tree file, answered by [`Event::Opened`].
    Read(String),
    Write {
        path: String,
        contents: String,
    },
    Stage(Vec<String>),
    StageAll,
    Commit {
        message: String,
    },
    Log {
        path: Option<String>,
        limit: usize,
    },
    WordDiff {
        from: String,
        to: Option<String>,
        path: Option<String>,
    },
    Show {
        rev: String,
        path: String,
    },
    Branches,
    CreateBranch(String),
    Switch(String),
    Tag {
        name: String,
        message: String,
    },
    Tags,
    Worktrees,
    AddWorktree {
        path: PathBuf,
        checkout: Checkout,
    },
    RemoveWorktree {
        path: PathBuf,
        force: bool,
    },
    PruneWorktrees,
}

#[derive(Debug)]
pub enum Event {
    Status(Status),
    Files(Vec<String>),
    /// A work-tree file's contents, or `None` if it isn't text.
    Opened {
        path: String,
        contents: Option<String>,
    },
    Written(String),
    Committed(String),
    Log(Vec<Commit>),
    Diff(Vec<FileDiff>),
    File {
        rev: String,
        path: String,
        contents: String,
    },
    Branches(Vec<Branch>),
    Tags(Vec<Tag>),
    Worktrees(Vec<Worktree>),
    /// A new worktree is ready. Spawn another actor on it to work there.
    WorktreeAdded(Git),
    Error(GitError),
}

/// Cheap to clone; the actor stops once every handle is dropped.
#[derive(Debug, Clone)]
pub struct GitHandle {
    tx: UnboundedSender<Command>,
}

impl GitHandle {
    /// Queues a command. Returns `false` if the actor has stopped.
    pub fn send(&self, command: Command) -> bool {
        self.tx.send(command).is_ok()
    }
}

/// Starts the actor on `runtime`. `notify` runs after every event is sent,
/// so the UI can request a repaint.
pub fn spawn(
    git: Git,
    runtime: &Handle,
    notify: impl Fn() + Send + 'static,
) -> (GitHandle, UnboundedReceiver<Event>) {
    let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel();
    let (event_tx, event_rx) = mpsc::unbounded_channel();

    runtime.spawn(async move {
        while let Some(command) = cmd_rx.recv().await {
            for event in execute(&git, command).await {
                if event_tx.send(event).is_err() {
                    return;
                }
                notify();
            }
        }
    });

    (GitHandle { tx: cmd_tx }, event_rx)
}

/// Runs one command, turning a failure into [`Event::Error`].
async fn execute(git: &Git, command: Command) -> Vec<Event> {
    run(git, command)
        .await
        .unwrap_or_else(|e| vec![Event::Error(e)])
}

/// Commands that change the work tree or HEAD also report the new status,
/// so the UI never shows stale state.
async fn run(git: &Git, command: Command) -> Result<Vec<Event>> {
    Ok(match command {
        Command::Refresh => vec![Event::Status(git.status().await?)],
        Command::Files => vec![Event::Files(git.files().await?)],
        Command::Read(path) => {
            let contents = git.read(&path).await?;
            vec![Event::Opened { path, contents }]
        }
        Command::Write { path, contents } => {
            git.write(&path, &contents).await?;
            vec![Event::Written(path), Event::Status(git.status().await?)]
        }
        Command::Stage(paths) => {
            let paths: Vec<_> = paths.iter().map(String::as_str).collect();
            git.stage(&paths).await?;
            vec![Event::Status(git.status().await?)]
        }
        Command::StageAll => {
            git.stage_all().await?;
            vec![Event::Status(git.status().await?)]
        }
        Command::Commit { message } => {
            let oid = git.commit(&message).await?;
            vec![Event::Committed(oid), Event::Status(git.status().await?)]
        }
        Command::Log { path, limit } => vec![Event::Log(git.log(path.as_deref(), limit).await?)],
        Command::WordDiff { from, to, path } => {
            let diff = git.word_diff(&from, to.as_deref(), path.as_deref()).await?;
            vec![Event::Diff(diff)]
        }
        Command::Show { rev, path } => {
            let contents = git.show(&rev, &path).await?;
            vec![Event::File {
                rev,
                path,
                contents,
            }]
        }
        Command::Branches => vec![Event::Branches(git.branches().await?)],
        Command::CreateBranch(name) => {
            git.create_branch(&name).await?;
            vec![Event::Status(git.status().await?)]
        }
        Command::Switch(name) => {
            git.switch(&name).await?;
            vec![Event::Status(git.status().await?)]
        }
        Command::Tag { name, message } => {
            git.tag(&name, &message).await?;
            vec![Event::Tags(git.tags().await?)]
        }
        Command::Tags => vec![Event::Tags(git.tags().await?)],
        Command::Worktrees => vec![Event::Worktrees(git.worktrees().await?)],
        Command::AddWorktree { path, checkout } => {
            let added = git.add_worktree(&path, &checkout).await?;
            vec![
                Event::WorktreeAdded(added),
                Event::Worktrees(git.worktrees().await?),
            ]
        }
        Command::RemoveWorktree { path, force } => {
            git.remove_worktree(&path, force).await?;
            vec![Event::Worktrees(git.worktrees().await?)]
        }
        Command::PruneWorktrees => {
            git.prune_worktrees().await?;
            vec![Event::Worktrees(git.worktrees().await?)]
        }
    })
}
