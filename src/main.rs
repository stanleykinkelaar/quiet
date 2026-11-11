#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod capture;
mod data;
mod icon;
mod platform;
mod reminders;
#[cfg(windows)]
mod system;
mod ui;

use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::*;
use quiet_store::Store;

use crate::platform::{autostart, single_instance};
use crate::ui::MainView;

/// The main window, if it is open. Closing it hides the app to the tray.
struct MainWindow(Option<AnyWindowHandle>);
impl Global for MainWindow {}

pub fn open_main(cx: &mut App) {
    if let Some(handle) = cx.global::<MainWindow>().0
        && handle.update(cx, |_, window, _| window.activate_window()).is_ok()
    {
        return;
    }
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::centered(size(px(920.), px(680.)), cx)),
        titlebar: Some(TitlebarOptions { title: Some("Quiet".into()), ..Default::default() }),
        window_min_size: Some(size(px(560.), px(420.))),
        ..Default::default()
    };
    let opened = gpui_kit::open_window(options, cx, |window, cx| {
        window.on_window_should_close(cx, |_, cx| {
            cx.global_mut::<MainWindow>().0 = None;
            true
        });
        window
            .observe_window_appearance(|window, cx| Theme::sync_system_appearance(Some(window), cx))
            .detach();
        Theme::sync_system_appearance(Some(window), cx);
        cx.new(|cx| MainView::new(window, cx))
    });
    match opened {
        Ok((handle, _)) => cx.global_mut::<MainWindow>().0 = Some(handle),
        Err(err) => data::log_error(&err),
    }
}

#[cfg(windows)]
struct SystemGlobal(system::System);
#[cfg(windows)]
impl Global for SystemGlobal {}

/// Registers the quick-capture shortcut, replacing the old one.
pub fn set_hotkey(spec: &str, cx: &mut App) -> anyhow::Result<()> {
    #[cfg(windows)]
    if cx.has_global::<SystemGlobal>() {
        return cx.global_mut::<SystemGlobal>().0.set_hotkey(spec);
    }
    let _ = (spec, cx);
    Ok(())
}

#[cfg(windows)]
fn start_system(cx: &mut App, show_requests: async_channel::Receiver<()>) {
    let (tx, rx) = async_channel::unbounded();
    match system::System::start(tx.clone()) {
        Ok(system) => cx.set_global(SystemGlobal(system)),
        Err(err) => data::log_error(&err),
    }
    let hotkey = ui::saved_hotkey(cx);
    if let Err(err) = set_hotkey(&hotkey, cx) {
        data::log_error(&err.context(format!("registering {hotkey}")));
    }
    // Another copy of the app asked us to show the window.
    cx.background_spawn(async move {
        while show_requests.recv().await.is_ok() {
            tx.send(system::SystemEvent::Open).await.ok();
        }
    })
    .detach();
    cx.spawn(async move |cx| {
        while let Ok(event) = rx.recv().await {
            cx.update(|cx| match event {
                system::SystemEvent::Open => open_main(cx),
                system::SystemEvent::QuickCapture => capture::open(cx),
                system::SystemEvent::Quit => cx.quit(),
            });
        }
    })
    .detach();
}

fn main() {
    let db_path = data::database_path();
    let scope = format!("{:x}", fnv1a(&db_path.to_string_lossy()));
    let listener = match single_instance::acquire(&scope) {
        single_instance::Instance::First(listener) => listener,
        single_instance::Instance::Second => return,
    };
    let store = match Store::open(&db_path) {
        Ok(store) => store,
        Err(err) => {
            eprintln!("quiet: could not open the database: {err:#}");
            std::process::exit(1);
        }
    };
    // First launch of a new day: unfinished Today tasks go quietly back.
    if let Err(err) = store.start_day(data::today()) {
        data::log_error(&err);
    }
    let background = std::env::args().any(|a| a == autostart::BACKGROUND_ARG);

    gpui_kit::application().with_assets(gpui_kit::assets::Assets).run(move |cx| {
        gpui_kit::init(cx);
        Theme::change(ThemeMode::Light, None, cx);
        cx.set_quit_mode(QuitMode::Explicit);
        data::init(store, cx);
        cx.set_global(MainWindow(None));
        ui::bind_keys(cx);
        capture::bind_keys(cx);

        // Starting with Windows is on by default for the installed app, once.
        if !cfg!(debug_assertions) && data::read(cx, |s| s.setting("autostart")).is_none() {
            if let Err(err) = autostart::set_enabled(true) {
                data::log_error(&err);
            }
            data::change(cx, |s| s.set_setting("autostart", "1"));
        }

        let (show_tx, show_rx) = async_channel::unbounded();
        listener.listen(move || {
            show_tx.send_blocking(()).ok();
        });
        #[cfg(windows)]
        start_system(cx, show_rx);
        #[cfg(not(windows))]
        drop(show_rx);

        reminders::start(cx);

        if !background {
            open_main(cx);
        }
    });
}

/// A tiny stable hash, so the single-instance name does not leak the path.
fn fnv1a(s: &str) -> u64 {
    s.bytes().fold(0xcbf29ce484222325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3))
}
