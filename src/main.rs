#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod capture;
mod data;
mod icon;
mod platform;
mod reminders;
mod system;
mod ui;
mod update;

use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::*;
use quiet_store::Store;

use crate::platform::{Request, autostart, single_instance};
use crate::ui::MainView;

actions!(quiet_app, [Quit]);

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
        // Small enough to sit beside other windows; below 640px wide the
        // layout folds into its compact form (see ui::Layout).
        window_min_size: Some(size(px(360.), px(400.))),
        // Matches StartupWMClass in quiet.desktop, so Linux docks and
        // launchers show the right icon.
        app_id: Some("quiet".into()),
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

struct SystemGlobal(system::System);
impl Global for SystemGlobal {}

/// Registers the quick-capture shortcut, replacing the old one.
pub fn set_hotkey(spec: &str, cx: &mut App) -> anyhow::Result<()> {
    match cx.try_global::<SystemGlobal>() {
        Some(_) => cx.global_mut::<SystemGlobal>().0.set_hotkey(spec),
        None => anyhow::bail!("the tray did not start, see quiet.log"),
    }
}

/// Starts the tray and the hotkey, and routes their events, and requests
/// from other copies of the app, to the UI.
fn start_system(cx: &mut App, requests: async_channel::Receiver<Request>) {
    let (tx, rx) = async_channel::unbounded();
    match system::System::start(tx.clone()) {
        Ok(system) => cx.set_global(SystemGlobal(system)),
        Err(err) => data::log_error(&err),
    }
    // Linux has no app-owned global shortcuts; see `quiet --capture`.
    if cfg!(not(target_os = "linux")) && cx.has_global::<SystemGlobal>() {
        let hotkey = ui::saved_hotkey(cx);
        if let Err(err) = set_hotkey(&hotkey, cx) {
            data::log_error(&err.context(format!("registering {hotkey}")));
        }
    }
    cx.background_spawn(async move {
        while let Ok(request) = requests.recv().await {
            let event = match request {
                Request::Show => system::SystemEvent::Open,
                Request::Capture => system::SystemEvent::QuickCapture,
            };
            tx.send(event).await.ok();
        }
    })
    .detach();
    cx.spawn(async move |cx| {
        while let Ok(event) = rx.recv().await {
            cx.update(|cx| match event {
                system::SystemEvent::Open => open_main(cx),
                system::SystemEvent::QuickCapture => capture::toggle(cx),
                system::SystemEvent::Quit => cx.quit(),
            });
        }
    })
    .detach();
}

fn main() {
    let db_path = data::database_path();
    let scope = format!("{:x}", fnv1a(&db_path.to_string_lossy()));
    let args: Vec<String> = std::env::args().collect();
    let request = if args.iter().any(|a| a == platform::CAPTURE_ARG) {
        Request::Capture
    } else {
        Request::Show
    };
    let listener = match single_instance::acquire(&scope, request) {
        single_instance::Instance::First(listener) => listener,
        single_instance::Instance::Second => return,
    };
    let store = match Store::open(&db_path) {
        Ok(store) => store,
        Err(err) => {
            // Release builds have no console and there is no window yet, so
            // say it in a notification, and in quiet.log for the details.
            let body = if err.downcast_ref::<quiet_store::NewerDatabase>().is_some() {
                "Your list was saved by a newer version of Quiet. Run the install command again to get it back, then start Quiet. Your list is unchanged.".to_string()
            } else {
                format!("Quiet could not open your list: {err:#}. Details are in quiet.log next to it.")
            };
            data::log_error(&err.context("could not open the database"));
            reminders::show_message("Quiet did not start", &body);
            std::process::exit(1);
        }
    };
    // First launch of a new day: unfinished Today tasks go quietly back.
    if let Err(err) = store.start_day(data::today()) {
        data::log_error(&err);
    }
    let background = args.iter().any(|a| a == autostart::BACKGROUND_ARG);

    let app = gpui_kit::application().with_assets(gpui_kit::assets::Assets);
    // macOS: clicking the Dock icon brings the window back.
    app.on_reopen(open_main);
    app.run(move |cx| {
        gpui_kit::init(cx);
        Theme::change(ThemeMode::Light, None, cx);
        cx.set_quit_mode(QuitMode::Explicit);
        data::init(store, cx);
        update::init(cx);
        cx.set_global(MainWindow(None));
        ui::bind_keys(cx);
        capture::bind_keys(cx);
        // Cmd+Q on macOS, Ctrl+Q elsewhere. Closing the window only hides it.
        cx.on_action(|_: &Quit, cx| cx.quit());
        let quit_key = if cfg!(target_os = "macos") { "cmd-q" } else { "ctrl-q" };
        cx.bind_keys([KeyBinding::new(quit_key, Quit, None)]);
        #[cfg(target_os = "macos")]
        {
            cx.set_menus(vec![Menu {
                name: "Quiet".into(),
                items: vec![MenuItem::action("Quit Quiet", Quit)],
                disabled: false,
            }]);
        }

        // Starting at sign-in is on by default for the installed app, once.
        if !cfg!(debug_assertions) && data::read(cx, |s| s.setting("autostart")).is_none() {
            if let Err(err) = autostart::set_enabled(true) {
                data::log_error(&err);
            }
            data::change(cx, |s| s.set_setting("autostart", "1"));
        }

        let (request_tx, request_rx) = async_channel::unbounded();
        listener.listen(move |request| {
            request_tx.send_blocking(request).ok();
        });
        start_system(cx, request_rx);

        reminders::start(cx);
        update::start(cx);

        match request {
            Request::Capture => capture::open(cx),
            Request::Show if !background => open_main(cx),
            Request::Show => {}
        }
    });
}

/// A tiny stable hash, so the single-instance name does not leak the path.
fn fnv1a(s: &str) -> u64 {
    s.bytes().fold(0xcbf29ce484222325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3))
}
