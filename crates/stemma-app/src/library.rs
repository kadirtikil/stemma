use eframe::egui;
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
    error: Option<String>,
    tx: UnboundedSender<Reply>,
    replies: UnboundedReceiver<Reply>,
    runtime: Handle,
    ctx: egui::Context,
}

enum Reply {
    Papers(Result<Vec<PaperEntry>>),
    /// A paper was opened or created and is ready to edit.
    Ready(Result<Git>),
}

impl Shelf {
    pub fn new(library: Library, runtime: Handle, ctx: egui::Context) -> Self {
        let (tx, replies) = mpsc::unbounded_channel();
        let shelf = Self {
            library,
            papers: None,
            new_name: String::new(),
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
            ui.heading("Papers");
            ui.weak(self.library.root().display().to_string());
            ui.add_space(12.0);

            match &self.papers {
                None => {
                    ui.spinner();
                }
                Some(papers) if papers.is_empty() => {
                    ui.label("No papers yet. Name your first one below.");
                }
                Some(papers) => {
                    let mut clicked = None;
                    egui::ScrollArea::vertical()
                        .max_height(ui.available_height() - 80.0)
                        .show(ui, |ui| {
                            for paper in papers {
                                if ui.selectable_label(false, &paper.name).clicked() {
                                    clicked = Some(paper.path.clone());
                                }
                            }
                        });
                    if let Some(path) = clicked {
                        self.spawn(async move { Reply::Ready(Git::discover(path).await) });
                    }
                }
            }

            ui.add_space(12.0);
            ui.horizontal(|ui| {
                let field = ui.add(
                    egui::TextEdit::singleline(&mut self.new_name).hint_text("New paper name"),
                );
                let entered = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                let create = ui.button("Create").clicked() || entered;
                if create && !self.new_name.trim().is_empty() {
                    let library = self.library.clone();
                    let name = std::mem::take(&mut self.new_name);
                    self.spawn(async move { Reply::Ready(library.create(&name).await) });
                }
            });

            if let Some(error) = &self.error {
                ui.colored_label(ui.visuals().error_fg_color, error);
            }
        });

        ready
    }

    fn handle_replies(&mut self) -> Option<Git> {
        let mut ready = None;
        while let Ok(reply) = self.replies.try_recv() {
            match reply {
                Reply::Papers(Ok(papers)) => self.papers = Some(papers),
                Reply::Ready(Ok(git)) => {
                    self.error = None;
                    ready = Some(git);
                }
                Reply::Papers(Err(e)) | Reply::Ready(Err(e)) => self.error = Some(e.to_string()),
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
