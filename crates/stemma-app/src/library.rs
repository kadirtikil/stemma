use eframe::egui;
use egui_neo::{
    NeuAlert, NeuButton, NeuEmptyState, NeuIcon, NeuLoader, NeuNavLink, NeuStatus, NeuText,
    NeuTextInput, NeuTitle,
};
use stemma_core::Git;
use stemma_core::error::Result;
use stemma_core::library::{Library, PaperEntry};
use tokio::runtime::Handle;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

/// The library screen: every paper, and a way to start a new one.
pub struct Shelf {
    library: Library,
    /// `None` until the first listing arrives.
    papers: Option<Vec<PaperEntry>>,
    new_name: String,
    /// Why the last paper couldn't be created, shown on the name field.
    create_error: Option<String>,
    /// Why listing or opening papers failed.
    error: Option<String>,
    tx: UnboundedSender<Reply>,
    replies: UnboundedReceiver<Reply>,
    runtime: Handle,
    ctx: egui::Context,
}

enum Reply {
    Papers(Result<Vec<PaperEntry>>),
    Opened(Result<Git>),
    Created(Result<Git>),
}

/// The library is a single column this wide, centered in the window.
const COLUMN_WIDTH: f32 = 560.0;

impl Shelf {
    pub fn new(library: Library, runtime: Handle, ctx: egui::Context) -> Self {
        let (tx, replies) = mpsc::unbounded_channel();
        let shelf = Self {
            library,
            papers: None,
            new_name: String::new(),
            create_error: None,
            error: None,
            tx,
            replies,
            runtime,
            ctx,
        };
        shelf.refresh();
        shelf
    }

    /// Lists the papers again, since they may have changed on disk.
    pub fn refresh(&self) {
        let library = self.library.clone();
        self.spawn(async move { Reply::Papers(library.papers().await) });
    }

    /// Draws the library. Returns a paper once one is ready to open.
    pub fn ui(&mut self, ui: &mut egui::Ui) -> Option<Git> {
        let ready = self.handle_replies();

        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                let width = COLUMN_WIDTH.min(ui.available_width());
                ui.horizontal(|ui| {
                    ui.add_space((ui.available_width() - width) / 2.0);
                    ui.vertical(|ui| {
                        ui.set_width(width);
                        ui.add_space(32.0);
                        self.column_ui(ui);
                    });
                });
            });
        });

        ready
    }

    fn column_ui(&mut self, ui: &mut egui::Ui) {
        ui.add(NeuTitle::new("Papers"));
        ui.add(NeuText::new(self.library.root().display().to_string()).dimmed());
        ui.add_space(16.0);

        ui.horizontal(|ui| {
            let button_width = 90.0;
            let mut input = NeuTextInput::new(&mut self.new_name)
                .placeholder("New paper name")
                .left_icon(NeuIcon::Plus)
                .width(ui.available_width() - button_width - ui.spacing().item_spacing.x);
            if let Some(error) = &self.create_error {
                input = input.error(error.as_str());
            }
            let field = ui.add(input);
            if field.changed() {
                self.create_error = None;
            }
            let entered = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let clicked = ui
                .add(NeuButton::new("Create").min_size(egui::vec2(button_width, 0.0)))
                .clicked();
            if (clicked || entered) && !self.new_name.trim().is_empty() {
                let library = self.library.clone();
                let name = std::mem::take(&mut self.new_name);
                self.spawn(async move { Reply::Created(library.create(&name).await) });
            }
        });
        ui.add_space(16.0);

        if let Some(error) = &self.error {
            let mut open = true;
            ui.add(
                NeuAlert::new(error.as_str())
                    .status(NeuStatus::Danger)
                    .closable(&mut open),
            );
            if !open {
                self.error = None;
            }
            ui.add_space(16.0);
        }

        match &self.papers {
            None => {
                ui.vertical_centered(|ui| ui.add(NeuLoader::new()));
            }
            Some(papers) if papers.is_empty() => {
                ui.add(
                    NeuEmptyState::new("No papers yet")
                        .icon(NeuIcon::Star)
                        .description("Name your first paper above to start writing."),
                );
            }
            Some(papers) => {
                let mut clicked = None;
                for paper in papers {
                    let link = NeuNavLink::new(paper.name.as_str())
                        .chevron(true)
                        .id_salt(&paper.path);
                    if ui.add(link).clicked() {
                        clicked = Some(paper.path.clone());
                    }
                }
                if let Some(path) = clicked {
                    self.spawn(async move { Reply::Opened(Git::discover(path).await) });
                }
            }
        }
    }

    fn handle_replies(&mut self) -> Option<Git> {
        let mut ready = None;
        while let Ok(reply) = self.replies.try_recv() {
            match reply {
                Reply::Papers(Ok(papers)) => self.papers = Some(papers),
                Reply::Opened(Ok(git)) | Reply::Created(Ok(git)) => {
                    self.error = None;
                    ready = Some(git);
                }
                Reply::Created(Err(e)) => self.create_error = Some(e.to_string()),
                Reply::Papers(Err(e)) | Reply::Opened(Err(e)) => self.error = Some(e.to_string()),
            }
        }
        ready
    }

    /// Runs `task` off the UI thread and wakes the UI with its reply.
    fn spawn(&self, task: impl Future<Output = Reply> + Send + 'static) {
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        self.runtime.spawn(async move {
            // The shelf only goes away when the app exits.
            let _ = tx.send(task.await);
            ctx.request_repaint();
        });
    }
}
