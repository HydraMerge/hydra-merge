//! Startup and manual update checks, and the Install / Later actions.

use std::sync::mpsc::Sender;
use std::thread;

use eframe::egui::Context;

use super::WorktreeBarApp;
use crate::update::{self, Offer};

pub(super) enum UpdateMsg {
    Checked {
        force: bool,
        result: Result<Option<Offer>, String>,
    },
    Ready(Result<std::path::PathBuf, String>),
}

impl WorktreeBarApp {
    pub(super) fn schedule_update_check(&mut self, ctx: &Context, force: bool) {
        let Some(dir) = self.theme_dir.clone() else {
            self.status = "Updates need the settings folder".to_string();
            return;
        };
        if force {
            self.status = "Checking for updates…".to_string();
        }
        if !force && !update::check_is_due(&dir) {
            return;
        }
        update::mark_checked(&dir);
        let tx = self.update_tx.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let _ = tx.send(UpdateMsg::Checked {
                force,
                result: update::fetch_offer(),
            });
            ctx.request_repaint();
        });
    }

    pub(super) fn install_update(&mut self, ctx: &Context) {
        let Some(offer) = self.update_offer.clone() else {
            return;
        };
        if self.update_busy {
            return;
        }
        self.update_busy = true;
        self.status = format!("Downloading update {}…", offer.version);
        let tx = self.update_tx.clone();
        let ctx = ctx.clone();
        spawn(tx, ctx, UpdateMsg::Ready, move || update::download(&offer));
    }

    pub(super) fn dismiss_update(&mut self) {
        if let (Some(dir), Some(offer)) = (&self.theme_dir, &self.update_offer) {
            update::dismiss(dir, &offer.version);
        }
        self.update_offer = None;
        self.status = "Update postponed".to_string();
    }

    pub(super) fn poll_update(&mut self, ctx: &Context) {
        let message = match self.update_rx.try_recv() {
            Ok(message) => message,
            Err(_) => return,
        };
        match message {
            UpdateMsg::Checked { force, result } => match result {
                Ok(Some(offer)) => {
                    let skipped = self
                        .theme_dir
                        .as_ref()
                        .and_then(|dir| update::dismissed_version(dir))
                        .is_some_and(|version| version == offer.version);
                    if !skipped {
                        self.status = format!("Update {} is available", offer.version);
                        self.update_offer = Some(offer);
                    }
                }
                Ok(None) if force => {
                    self.status = format!("HydraMerge {ver} is up to date", ver = update::CURRENT);
                }
                Ok(None) => {}
                Err(error) => {
                    crate::log_warn!("update check: {error}");
                    if force {
                        self.status = error;
                    }
                }
            },
            UpdateMsg::Ready(Ok(staged)) => {
                self.update_busy = false;
                match update::handoff(&staged) {
                    Ok(()) => {
                        self.status = "Restarting to finish the update…".to_string();
                        ctx.send_viewport_cmd(eframe::egui::ViewportCommand::Close);
                    }
                    Err(error) => {
                        crate::log_warn!("update install: {error}");
                        self.status = error;
                    }
                }
            }
            UpdateMsg::Ready(Err(error)) => {
                self.update_busy = false;
                crate::log_warn!("update download: {error}");
                self.status = error;
            }
        }
    }
}

fn spawn<T, F>(tx: Sender<UpdateMsg>, ctx: Context, wrap: fn(T) -> UpdateMsg, work: F)
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    thread::spawn(move || {
        let _ = tx.send(wrap(work()));
        ctx.request_repaint();
    });
}
