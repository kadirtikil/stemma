use std::collections::BTreeMap;

use eframe::egui;
use stemma_core::git::Status;
use stemma_core::service::{self, Command, Event, GitHandle};
use stemma_core::{Git, GitError};
use tokio::runtime::{Handle, Runtime};
use tokio::sync::mpsc::UnboundedReceiver;

/// The file opened when a paper opens, if it exists.
const MAIN_FILE: &str = "main.typ";

/// An open paper: one repository and the git actor serving it.
pub struct Paper {
    git: GitHandle,
    events: UnboundedReceiver<Event>,
    name: String,
    status: Option<Status>,
    files: Vec<String>,
    editor: Option<Editor>,
    /// Edits to files no longer open whose write failed, by path. Opening
    /// the file again restores them.
    unwritten: BTreeMap<String, String>,
    last_error: Option<String>,
    /// Whether the file list has arrived yet, to open [`MAIN_FILE`] once.
    listed: bool,
    /// Writes pending edits directly on close, when the actor can no longer
    /// be waited on.
    repo: Git,
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
    /// The last write failed. Stops retrying on every pause until the next
    /// edit, though switching files or focus still tries again.
    write_failed: bool,
}

enum Contents {
    Loading,
    Text(String),
    /// Not UTF-8 text, so it can't be edited here.
    Binary,
}

/// How long typing must pause before edits are written to disk.
const WRITE_DELAY: f64 = 1.0;

impl Paper {
    /// Starts a git actor for `git` on `runtime`.
    pub fn open(git: Git, runtime: &Handle, ctx: egui::Context) -> Self {
        let name = git.root().file_name().map_or_else(
            || git.root().display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        let repo = git.clone();
        let (handle, events) = service::spawn(git, runtime, move || ctx.request_repaint());
        handle.send(Command::Refresh);
        handle.send(Command::Files);

        Self {
            git: handle,
            events,
            name,
            status: None,
            files: Vec::new(),
            editor: None,
            unwritten: BTreeMap::new(),
            last_error: None,
            listed: false,
            repo,
        }
    }

    /// Draws the paper. Returns `true` when the writer asks to go back to
    /// the library.
    pub fn ui(&mut self, ui: &mut egui::Ui) -> bool {
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

        let mut back = false;
        egui::Panel::top("title").show(ui, |ui| {
            ui.horizontal(|ui| {
                back = ui.button("< Library").clicked();
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
                        self.open_file(path);
                    }
                });
            });

        egui::CentralPanel::default().show(ui, |ui| self.editor_ui(ui));

        self.write_when_idle(ui);
        back
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
                        editor.write_failed = false;
                        editor.last_edit = ui.input(|i| i.time);
                    }
                });
            }
        }
    }

    /// Writes the open file once typing has paused for [`WRITE_DELAY`].
    fn write_when_idle(&mut self, ui: &egui::Ui) {
        let Some(editor) = self.editor.as_ref().filter(|e| e.dirty && !e.write_failed) else {
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
    fn open_file(&mut self, path: String) {
        self.flush();
        let editor = match self.unwritten.remove(&path) {
            Some(text) => Editor {
                path,
                contents: Contents::Text(text),
                dirty: true,
                last_edit: 0.0,
                write_failed: false,
            },
            None => {
                self.git.send(Command::Read(path.clone()));
                Editor {
                    path,
                    contents: Contents::Loading,
                    dirty: false,
                    last_edit: 0.0,
                    write_failed: false,
                }
            }
        };
        self.editor = Some(editor);
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

    /// Writes every pending edit before the paper closes. The actor stops
    /// with the paper and couldn't report a failure, so write directly.
    pub fn close(&self, runtime: &Runtime) {
        let open = self
            .editor
            .as_ref()
            .filter(|e| e.dirty)
            .and_then(|e| match &e.contents {
                Contents::Text(text) => Some((&e.path, text)),
                _ => None,
            });
        for (path, text) in self.unwritten.iter().chain(open) {
            if let Err(e) = runtime.block_on(self.repo.write(path, text)) {
                eprintln!("stemma: couldn't write {path}: {e}");
            }
        }
    }

    /// Keeps the edits from a failed write so a later write can retry them.
    fn write_failed(&mut self, path: String, contents: String, error: &GitError) {
        self.last_error = Some(format!(
            "Couldn't write {path}, your edits are kept: {error}"
        ));
        match self.editor.as_mut().filter(|e| e.path == path) {
            Some(editor) => {
                // Newer edits already supersede the failed contents.
                if !editor.dirty {
                    editor.contents = Contents::Text(contents);
                    editor.dirty = true;
                }
                editor.write_failed = true;
            }
            None => {
                self.unwritten.insert(path, contents);
            }
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
                    if !self.listed
                        && self.editor.is_none()
                        && self.files.iter().any(|f| f == MAIN_FILE)
                    {
                        self.open_file(MAIN_FILE.to_owned());
                    }
                    self.listed = true;
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
                Event::WriteFailed {
                    path,
                    contents,
                    error,
                } => self.write_failed(path, contents, &error),
                Event::Error(e) => self.last_error = Some(e.to_string()),
                _ => {}
            }
        }
    }
}
