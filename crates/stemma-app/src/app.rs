use std::path::Path;

use eframe::egui;
use stemma_core::Git;
use stemma_core::git::Status;
use stemma_core::service::{self, Command, Event, GitHandle};
use tokio::runtime::Runtime;
use tokio::sync::mpsc::UnboundedReceiver;

pub enum StemmaApp {
    Open(Box<Paper>),
    /// The repository couldn't be opened; shown instead of the editor.
    Failed(String),
}

/// An open paper: one repository and the git actor serving it.
pub struct Paper {
    git: GitHandle,
    events: UnboundedReceiver<Event>,
    name: String,
    status: Option<Status>,
    last_error: Option<String>,
    // Runs the git actor on worker threads, keeping the main thread for
    // eframe. Declared last so it is dropped after the handle.
    _runtime: Runtime,
}

impl StemmaApp {
    pub fn open(path: &Path, ctx: egui::Context) -> Self {
        let runtime = match Runtime::new() {
            Ok(rt) => rt,
            Err(e) => return Self::Failed(format!("Couldn't start the git runtime: {e}")),
        };
        let git = match runtime.block_on(Git::discover(path)) {
            Ok(git) => git,
            Err(e) => return Self::Failed(e.to_string()),
        };
        let name = git.root().file_name().map_or_else(
            || git.root().display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        );

        let (handle, events) = service::spawn(git, runtime.handle(), move || ctx.request_repaint());
        handle.send(Command::Refresh);

        Self::Open(Box::new(Paper {
            git: handle,
            events,
            name,
            status: None,
            last_error: None,
            _runtime: runtime,
        }))
    }
}

impl eframe::App for StemmaApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        match self {
            Self::Open(paper) => paper.ui(ui),
            Self::Failed(message) => {
                egui::CentralPanel::default().show(ui, |ui| {
                    ui.centered_and_justified(|ui| {
                        ui.label(format!("Couldn't open this paper.\n\n{message}"));
                    });
                });
            }
        }
    }
}

impl Paper {
    fn ui(&mut self, ui: &mut egui::Ui) {
        self.handle_events();

        // Files may have been edited or committed outside Stemma.
        let focused = ui.input(|i| {
            i.events
                .iter()
                .any(|e| matches!(e, egui::Event::WindowFocused(true)))
        });
        if focused {
            self.git.send(Command::Refresh);
        }

        egui::Panel::top("title").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading(&self.name);
                if let Some(branch) = self.status.as_ref().and_then(|s| s.branch.as_deref()) {
                    ui.label(format!("draft: {branch}"));
                }
            });
        });

        if let Some(error) = &self.last_error {
            let mut dismissed = false;
            egui::Panel::bottom("error").show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                    dismissed = ui.button("Dismiss").clicked();
                });
            });
            if dismissed {
                self.last_error = None;
            }
        }

        egui::CentralPanel::default().show(ui, |_ui| {});
    }

    fn handle_events(&mut self) {
        while let Ok(event) = self.events.try_recv() {
            match event {
                Event::Status(status) => self.status = Some(status),
                Event::Error(e) => self.last_error = Some(e.to_string()),
                _ => {}
            }
        }
    }
}
