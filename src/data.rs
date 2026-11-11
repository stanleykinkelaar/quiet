//! The bridge between the store and GPUI: one global that owns the
//! database, and a `change` helper that tells every open view to reload.

use std::path::PathBuf;

use chrono::{Local, NaiveDate};
use gpui_kit::{App, Global};
use quiet_store::Store;

pub struct Data {
    store: Store,
}

impl Global for Data {}

pub fn init(store: Store, cx: &mut App) {
    cx.set_global(Data { store });
}

pub fn store(cx: &App) -> &Store {
    &cx.global::<Data>().store
}

/// Applies a change to the store, then notifies every view observing
/// `Data` so it reloads. Errors are logged and swallowed: a failed write
/// should never crash a to-do app sitting in the tray.
pub fn change<T>(cx: &mut App, f: impl FnOnce(&Store) -> anyhow::Result<T>) -> Option<T> {
    let result = f(store(cx));
    cx.global_mut::<Data>();
    match result {
        Ok(value) => Some(value),
        Err(err) => {
            log_error(&err);
            None
        }
    }
}

/// Reads from the store, logging errors and falling back to `T::default()`.
pub fn read<T: Default>(cx: &App, f: impl FnOnce(&Store) -> anyhow::Result<T>) -> T {
    f(store(cx)).unwrap_or_else(|err| {
        log_error(&err);
        T::default()
    })
}

/// Errors go to the console in debug builds, and to `quiet.log` next to
/// the database in release builds, which have no console.
pub fn log_error(err: &anyhow::Error) {
    eprintln!("quiet: {err:#}");
    if !cfg!(debug_assertions) {
        use std::io::Write as _;
        let path = database_path().with_file_name("quiet.log");
        if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            writeln!(file, "{} {err:#}", Local::now().format("%Y-%m-%d %H:%M:%S")).ok();
        }
    }
}

pub fn today() -> NaiveDate {
    Local::now().date_naive()
}

/// `%APPDATA%\Quiet\quiet.db` on Windows. `QUIET_DB` overrides it, which is
/// handy for trying things out without touching the real list.
pub fn database_path() -> PathBuf {
    if let Some(path) = std::env::var_os("QUIET_DB") {
        return PathBuf::from(path);
    }
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("Quiet").join("quiet.db")
}
