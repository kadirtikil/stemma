use std::path::PathBuf;

use eframe::egui;
use stemma_core::Library;
use tokio::runtime::Runtime;

use crate::library::Shelf;
use crate::paper::Paper;

pub enum StemmaApp {
    Ready(Box<Stemma>),
    /// The library couldn't be opened; shown instead of the app.
    Failed(String),
}

pub struct Stemma {
    shelf: Shelf,
    /// The open paper, drawn instead of the library.
    paper: Option<Box<Paper>>,
    ctx: egui::Context,
    // Runs git and file work on worker threads, keeping the main thread for
    // eframe. Declared last so it is dropped after everything using it.
    runtime: Runtime,
}

impl StemmaApp {
    pub fn open(library: PathBuf, ctx: egui::Context) -> Self {
        let runtime = match Runtime::new() {
            Ok(rt) => rt,
            Err(e) => return Self::Failed(format!("Couldn't start the git runtime: {e}")),
        };
        let library = match runtime.block_on(Library::open(&library)) {
            Ok(library) => library,
            Err(e) => {
                return Self::Failed(format!(
                    "Couldn't open the library at {}: {e}",
                    library.display()
                ));
            }
        };
        Self::Ready(Box::new(Stemma {
            shelf: Shelf::new(library, runtime.handle().clone(), ctx.clone()),
            paper: None,
            ctx,
            runtime,
        }))
    }
}

impl eframe::App for StemmaApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        match self {
            Self::Ready(stemma) => stemma.ui(ui),
            Self::Failed(message) => {
                egui::CentralPanel::default().show(ui, |ui| {
                    ui.add_space(ui.available_height() / 3.0);
                    ui.add(
                        egui_neo::NeuEmptyState::new("Stemma couldn't start")
                            .icon(egui_neo::NeuIcon::Warning)
                            .description(message.as_str()),
                    );
                });
            }
        }
    }

    fn on_exit(&mut self) {
        if let Self::Ready(stemma) = self
            && let Some(paper) = &stemma.paper
        {
            paper.close(&stemma.runtime);
        }
    }
}

impl Stemma {
    fn ui(&mut self, ui: &mut egui::Ui) {
        if let Some(paper) = &mut self.paper {
            if paper.ui(ui) {
                paper.close(&self.runtime);
                self.paper = None;
                self.shelf.refresh();
            }
            return;
        }

        // Papers may have been added or removed outside Stemma.
        if ui.input(|i| i.events.contains(&egui::Event::WindowFocused(true))) {
            self.shelf.refresh();
        }
        if let Some(git) = self.shelf.ui(ui) {
            let paper = Paper::open(git, self.runtime.handle(), self.ctx.clone());
            self.paper = Some(Box::new(paper));
        }
    }
}
