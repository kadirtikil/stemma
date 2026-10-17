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
    files: Vec<String>,
    editor: Option<Editor>,
    last_error: Option<String>,
    /// Writes the open file directly on exit, when the actor can no longer
    /// be waited on.
    repo: Git,
    // Runs the git actor on worker threads, keeping the main thread for
    // eframe. Declared last so it is dropped after the handle.
    runtime: Runtime,
}

/// The file open in the editor.
struct Editor {
    /// Relative to the repo root.
    path: String,
    contents: Contents,
    /// Edited since the last write to disk.
    dirty: bool,
    /// When the last edit was made, in egui's input time.
    last_edit: f64,
}

enum Contents {
    Loading,
    Text(String),
    /// Not UTF-8 text, so it can't be edited here.
    Binary,
}

/// How long typing must pause before edits are written to disk.
const WRITE_DELAY: f64 = 1.0;

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

        let repo = git.clone();
        let (handle, events) = service::spawn(git, runtime.handle(), move || ctx.request_repaint());
        handle.send(Command::Refresh);
        handle.send(Command::Files);

        Self::Open(Box::new(Paper {
            git: handle,
            events,
            name,
            status: None,
            files: Vec::new(),
            editor: None,
            last_error: None,
            repo,
            runtime,
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

    fn on_exit(&mut self) {
        if let Self::Open(paper) = self {
            paper.write_on_exit();
        }
    }
}

impl Paper {
    fn ui(&mut self, ui: &mut egui::Ui) {
        self.handle_events();

        let focus = ui.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::WindowFocused(focused) => Some(*focused),
                _ => None,
            })
        });
        match focus {
            // Files may have been edited or committed outside Stemma.
            Some(true) => {
                self.git.send(Command::Refresh);
                self.git.send(Command::Files);
                if let Some(editor) = self.editor.as_ref().filter(|e| !e.dirty) {
                    self.git.send(Command::Read(editor.path.clone()));
                }
            }
            // Other tools should see the latest edits.
            Some(false) => self.flush(),
            None => {}
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

        egui::Panel::left("files")
            .resizable(true)
            .default_size(220.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    let open = self.editor.as_ref().map(|e| e.path.as_str());
                    let mut clicked = None;
                    for file in &self.files {
                        if ui.selectable_label(open == Some(file), file).clicked() {
                            clicked = Some(file.clone());
                        }
                    }
                    if let Some(path) = clicked.filter(|p| Some(p.as_str()) != open) {
                        self.open(path);
                    }
                });
            });

        egui::CentralPanel::default().show(ui, |ui| self.editor_ui(ui));

        self.write_when_idle(ui);
    }

    fn editor_ui(&mut self, ui: &mut egui::Ui) {
        let Some(editor) = &mut self.editor else {
            ui.centered_and_justified(|ui| ui.weak("Pick a file to start writing."));
            return;
        };
        match &mut editor.contents {
            Contents::Loading => {
                ui.centered_and_justified(|ui| ui.spinner());
            }
            Contents::Binary => {
                ui.centered_and_justified(|ui| {
                    ui.weak(format!("{} isn't a text file.", editor.path));
                });
            }
            Contents::Text(text) => {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    let response = ui.add_sized(
                        ui.available_size(),
                        egui::TextEdit::multiline(text)
                            .font(egui::TextStyle::Monospace)
                            .frame(egui::Frame::NONE),
                    );
                    if response.changed() {
                        editor.dirty = true;
                        editor.last_edit = ui.input(|i| i.time);
                    }
                });
            }
        }
    }

    /// Writes the open file once typing has paused for [`WRITE_DELAY`].
    fn write_when_idle(&mut self, ui: &egui::Ui) {
        let Some(editor) = self.editor.as_ref().filter(|e| e.dirty) else {
            return;
        };
        let idle = ui.input(|i| i.time) - editor.last_edit;
        if idle >= WRITE_DELAY {
            self.flush();
        } else {
            ui.ctx()
                .request_repaint_after_secs((WRITE_DELAY - idle) as f32);
        }
    }

    /// Opens `path` in the editor, writing the current file first.
    fn open(&mut self, path: String) {
        self.flush();
        self.git.send(Command::Read(path.clone()));
        self.editor = Some(Editor {
            path,
            contents: Contents::Loading,
            dirty: false,
            last_edit: 0.0,
        });
    }

    /// Writes the open file to disk if it has unwritten edits.
    fn flush(&mut self) {
        let Some(editor) = self.editor.as_mut().filter(|e| e.dirty) else {
            return;
        };
        if let Contents::Text(text) = &editor.contents {
            self.git.send(Command::Write {
                path: editor.path.clone(),
                contents: text.clone(),
            });
        }
        editor.dirty = false;
    }

    /// The actor won't get to run once eframe exits, so write directly.
    fn write_on_exit(&mut self) {
        let Some(editor) = self.editor.as_ref().filter(|e| e.dirty) else {
            return;
        };
        if let Contents::Text(text) = &editor.contents
            && let Err(e) = self.runtime.block_on(self.repo.write(&editor.path, text))
        {
            eprintln!("stemma: couldn't write {}: {e}", editor.path);
        }
    }

    fn handle_events(&mut self) {
        while let Ok(event) = self.events.try_recv() {
            match event {
                Event::Status(status) => self.status = Some(status),
                Event::Files(files) => {
                    // Close the file if it was deleted or renamed, unless
                    // there are edits that writing would bring back.
                    if self
                        .editor
                        .as_ref()
                        .is_some_and(|e| !e.dirty && !files.contains(&e.path))
                    {
                        self.editor = None;
                    }
                    self.files = files;
                }
                Event::Opened { path, contents } => {
                    // Ignore replies for a file since closed, and don't
                    // clobber edits made while a reload was in flight.
                    if let Some(editor) =
                        self.editor.as_mut().filter(|e| e.path == path && !e.dirty)
                    {
                        editor.contents = contents.map_or(Contents::Binary, Contents::Text);
                    }
                }
                Event::Error(e) => self.last_error = Some(e.to_string()),
                _ => {}
            }
        }
    }
}
