//! The quiet clock: once a day it rolls Today over, and every few seconds
//! it checks for reminders that are due and shows a Windows notification.

use std::time::Duration;

use chrono::Local;
use gpui_kit::{App, AppContext as _};

use crate::data;

/// How often the clock looks around. Reminders can be this late, at most.
const TICK: Duration = Duration::from_secs(15);
/// A reminder found later than this (the PC was asleep or off) is skipped
/// rather than shown late.
const GRACE_MINUTES: i64 = 30;

pub const APP_ID: &str = "Quiet.Tasks";

pub fn start(cx: &mut App) {
    #[cfg(windows)]
    register_app_id(cx);
    cx.spawn(async move |cx| {
        loop {
            cx.update(tick);
            cx.background_executor().timer(TICK).await;
        }
    })
    .detach();
}

fn tick(cx: &mut App) {
    let today = data::today();
    // The first check of a new day: unfinished Today tasks go back where
    // they came from, and repeating tasks for today come in.
    if data::read(cx, |s| s.setting("last_day")) != Some(today.format("%Y-%m-%d").to_string()) {
        data::change(cx, |s| s.start_day(today));
    }

    let now = Local::now().time();
    let grace = chrono::Duration::minutes(GRACE_MINUTES);
    let Some(due) = data::change(cx, |s| s.take_due_reminders(today, now, grace)) else { return };
    for task in due {
        let project = task
            .project_id
            .and_then(|p| data::read(cx, |s| s.project(p)))
            .map(|p| p.name);
        let body = match project {
            Some(name) => format!("{name}, on your Today list"),
            None => "On your Today list".to_string(),
        };
        show(task.title, body, cx);
    }
}

#[cfg(windows)]
fn show(title: String, body: String, cx: &mut App) {
    cx.background_spawn(async move {
        let result = notify_rust::Notification::new()
            .app_id(APP_ID)
            .summary(&title)
            .body(&body)
            .show();
        if let Err(err) = result {
            data::log_error(&anyhow::anyhow!("notification: {err}"));
        }
    })
    .detach();
}

#[cfg(not(windows))]
fn show(title: String, body: String, _: &mut App) {
    eprintln!("reminder: {title} ({body})");
}

/// An unpackaged app needs its AppUserModelID registered for toasts to
/// carry its own name and icon instead of "Windows PowerShell".
#[cfg(windows)]
fn register_app_id(cx: &mut App) {
    cx.set_app_identity(APP_ID, "Quiet");
    let icon = data::database_path().with_file_name("quiet.png");
    if !icon.exists() {
        let rgba = crate::icon::rgba(128);
        if let Some(image) = image::RgbaImage::from_raw(128, 128, rgba) {
            image.save(&icon).ok();
        }
    }
    let result = windows_registry::CURRENT_USER
        .create(format!(r"Software\Classes\AppUserModelId\{APP_ID}"))
        .and_then(|key| {
            key.set_string("DisplayName", "Quiet")?;
            key.set_string("IconUri", icon.to_string_lossy().as_ref())
        });
    if let Err(err) = result {
        data::log_error(&anyhow::anyhow!("registering notifications: {err}"));
    }
}
