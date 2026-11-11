//! The quiet clock: once a day it rolls Today over, and every few seconds
//! it checks for reminders that are due and shows a system notification
//! with Snooze and Done buttons.

use std::time::Duration;

use chrono::Local;
use gpui_kit::{App, Global};
use notify_rust::NotificationResponse;
use quiet_store::TaskId;

use crate::data;

/// How often the clock looks around. Reminders can be this late, at most.
const TICK: Duration = Duration::from_secs(15);
/// A reminder found later than this (the PC was asleep or off) does not
/// ring; the task is marked as missed instead.
const GRACE_MINUTES: i64 = 30;

/// What someone did with a reminder notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Response {
    Open,
    Snooze(i64),
    Done,
}

/// The buttons on a reminder: (action id, label).
const ACTIONS: &[(&str, &str)] = &[("snooze-10", "In 10 min"), ("snooze-60", "In 1 hour"), ("done", "Done")];

/// Hands notification responses (from their own threads) to the UI thread.
struct Responses(async_channel::Sender<(TaskId, Response)>);
impl Global for Responses {}

#[cfg(windows)]
pub const APP_ID: &str = "Quiet.Tasks";

pub fn start(cx: &mut App) {
    #[cfg(windows)]
    register_app_id(cx);
    // Notifications are attributed to the app bundle. Only works once.
    #[cfg(target_os = "macos")]
    notify_rust::set_application(crate::platform::BUNDLE_ID).ok();
    let (tx, rx) = async_channel::unbounded();
    cx.set_global(Responses(tx));
    cx.spawn(async move |cx| {
        while let Ok((id, response)) = rx.recv().await {
            cx.update(|cx| respond(id, response, cx));
        }
    })
    .detach();
    cx.spawn(async move |cx| {
        loop {
            cx.update(tick);
            cx.background_executor().timer(TICK).await;
        }
    })
    .detach();
}

fn respond(id: TaskId, response: Response, cx: &mut App) {
    match response {
        Response::Open => {
            data::change(cx, |s| s.acknowledge_reminder(id));
            crate::open_main(cx);
        }
        Response::Snooze(minutes) => {
            let until = Local::now().naive_local() + chrono::Duration::minutes(minutes);
            data::change(cx, |s| s.snooze(id, until));
        }
        Response::Done => {
            data::change(cx, |s| s.complete(id, quiet_store::now()));
        }
    }
}

fn tick(cx: &mut App) {
    let today = data::today();
    // The first check of a new day: unfinished Today tasks go back where
    // they came from, and repeating tasks for today come in.
    if data::read(cx, |s| s.setting("last_day")) != Some(today.format("%Y-%m-%d").to_string()) {
        data::change(cx, |s| s.start_day(today));
    }

    let now = Local::now().naive_local();
    let grace = chrono::Duration::minutes(GRACE_MINUTES);
    let Some(due) = data::change(cx, |s| s.take_due_reminders(now, grace)) else { return };
    for task in due {
        let project = task
            .project_id
            .and_then(|p| data::read(cx, |s| s.project(p)))
            .map(|p| p.name);
        let body = match project {
            Some(name) => format!("{name}, on your Today list"),
            None => "On your Today list".to_string(),
        };
        show(task.id, task.title, body, cx);
    }
}

/// Shows the notification on its own thread, which waits for a button
/// press (or the notification closing) and reports back.
fn show(id: TaskId, title: String, body: String, cx: &mut App) {
    let responses = cx.global::<Responses>().0.clone();
    std::thread::spawn(move || {
        let mut notification = notify_rust::Notification::new();
        notification.summary(&title).body(&body);
        for (action, label) in ACTIONS {
            notification.action(action, label);
        }
        #[cfg(windows)]
        notification.app_id(APP_ID);
        // "quiet" is the themed icon the installer puts in place. The
        // "default" action is what a click on the notification itself does.
        #[cfg(target_os = "linux")]
        notification.appname("Quiet").icon("quiet").action("default", "Open Quiet");
        let handle = match notification.show() {
            Ok(handle) => handle,
            Err(err) => return data::log_error(&anyhow::anyhow!("notification: {err}")),
        };
        let waited = handle.wait_for_response(|response: &NotificationResponse| {
            if let Some(response) = parse_response(response) {
                responses.send_blocking((id, response)).ok();
            }
        });
        if let Err(err) = waited {
            data::log_error(&anyhow::anyhow!("notification response: {err}"));
        }
    });
}

/// Closing or ignoring the notification is not a response: the reminder
/// then rings once more, and the task stays marked.
fn parse_response(response: &NotificationResponse) -> Option<Response> {
    match response {
        NotificationResponse::Default => Some(Response::Open),
        NotificationResponse::Action(action) => match action.as_str() {
            "default" => Some(Response::Open),
            "done" => Some(Response::Done),
            other => other.strip_prefix("snooze-").and_then(|m| m.parse().ok()).map(Response::Snooze),
        },
        _ => None,
    }
}

/// An unpackaged app needs its AppUserModelID registered for toasts to
/// carry its own name and icon instead of "Windows PowerShell".
#[cfg(windows)]
fn register_app_id(cx: &mut App) {
    cx.set_app_identity(APP_ID, "Quiet");
    // Written on every start, so a new icon replaces the old one after an update.
    let icon = data::database_path().with_file_name("quiet.png");
    if let Some(image) = image::RgbaImage::from_raw(128, 128, crate::icon::rgba(128)) {
        image.save(&icon).ok();
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
