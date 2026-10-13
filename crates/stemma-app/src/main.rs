//! Smoke test until the egui UI lands: opens the repo in the current
//! directory through the git service and prints what the UI would show.

use stemma_core::Git;
use stemma_core::service::{self, Command, Event};

fn main() {
    // The runtime's worker threads run git; the main thread stays free for
    // eframe, which must own it on macOS.
    let runtime = tokio::runtime::Runtime::new().expect("failed to start tokio runtime");

    let git = match runtime.block_on(Git::discover(".")) {
        Ok(git) => git,
        Err(e) => {
            eprintln!("stemma: {e}");
            std::process::exit(1);
        }
    };
    println!("repo   {}", git.root().display());

    let (handle, mut events) = service::spawn(git, runtime.handle(), || {});
    handle.send(Command::Refresh);
    handle.send(Command::Branches);
    handle.send(Command::Log {
        path: None,
        limit: 5,
    });
    drop(handle);

    while let Some(event) = events.blocking_recv() {
        match event {
            Event::Status(s) => {
                let branch = s.branch.as_deref().unwrap_or("(detached)");
                println!("branch {branch}, {} changed file(s)", s.files.len());
                for f in &s.files {
                    println!("  {:?}/{:?} {}", f.staged, f.unstaged, f.path);
                }
            }
            Event::Branches(branches) => {
                for b in branches {
                    println!("draft  {}{}", b.name, if b.is_current { " *" } else { "" });
                }
            }
            Event::Log(log) => {
                for c in log {
                    println!("save   {} {}", &c.oid[..7], c.summary);
                }
            }
            Event::Error(e) => eprintln!("error  {e}"),
            _ => {}
        }
    }
}
