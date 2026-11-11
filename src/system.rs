//! Tray icon and global hotkey.
//!
//! On Windows and macOS, `tray-icon` and `global-hotkey` hook into the
//! native event loop that GPUI already runs on the main thread, so creating
//! them inside `Application::run` is enough. Their callbacks only forward a
//! `SystemEvent` into a channel that a GPUI foreground task awaits.
//!
//! On Linux the tray is a StatusNotifierItem served over D-Bus on its own
//! thread (no GTK needed). Global shortcuts belong to the desktop there, so
//! quick capture is a key bound to `quiet --capture` instead.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemEvent {
    Open,
    Quit,
    QuickCapture,
}

pub use imp::System;

#[cfg(any(windows, target_os = "macos"))]
mod imp {
    use std::str::FromStr;

    use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState, hotkey::HotKey};
    use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
    use tray_icon::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

    use super::SystemEvent;

    pub struct System {
        _tray: TrayIcon,
        hotkeys: GlobalHotKeyManager,
        current: Option<HotKey>,
    }

    impl System {
        pub fn start(sender: async_channel::Sender<SystemEvent>) -> anyhow::Result<Self> {
            let open = MenuItem::new("Open", true, None);
            let quit = MenuItem::new("Quit", true, None);
            let menu = Menu::new();
            menu.append(&open)?;
            menu.append(&PredefinedMenuItem::separator())?;
            menu.append(&quit)?;

            let tray = TrayIconBuilder::new()
                .with_tooltip("Quiet")
                .with_icon(tray_icon::Icon::from_rgba(crate::icon::rgba(32), 32, 32)?)
                .with_menu(Box::new(menu))
                // macOS shows the menu on a left click, as menu bar items do.
                .with_menu_on_left_click(cfg!(target_os = "macos"))
                .build()?;

            let (open_id, quit_id) = (open.id().clone(), quit.id().clone());
            let tx = sender.clone();
            MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
                if event.id == open_id {
                    tx.try_send(SystemEvent::Open).ok();
                } else if event.id == quit_id {
                    tx.try_send(SystemEvent::Quit).ok();
                }
            }));
            let tx = sender.clone();
            TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
                if let TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } = event
                {
                    tx.try_send(SystemEvent::Open).ok();
                }
            }));
            let tx = sender;
            GlobalHotKeyEvent::set_event_handler(Some(move |event: GlobalHotKeyEvent| {
                if event.state() == HotKeyState::Pressed {
                    tx.try_send(SystemEvent::QuickCapture).ok();
                }
            }));

            Ok(Self { _tray: tray, hotkeys: GlobalHotKeyManager::new()?, current: None })
        }

        /// Registers `spec` (for example "Ctrl+Alt+Space") as the
        /// quick-capture hotkey, replacing the previous one.
        pub fn set_hotkey(&mut self, spec: &str) -> anyhow::Result<()> {
            let hotkey = HotKey::from_str(spec).map_err(|e| anyhow::anyhow!("{e}"))?;
            if let Some(old) = self.current.take() {
                self.hotkeys.unregister(old).ok();
            }
            self.hotkeys.register(hotkey)?;
            self.current = Some(hotkey);
            Ok(())
        }
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use ksni::blocking::{Handle, TrayMethods as _};
    use ksni::menu::{MenuItem, StandardItem};

    use super::SystemEvent;

    struct Tray(async_channel::Sender<SystemEvent>);

    impl ksni::Tray for Tray {
        fn id(&self) -> String {
            "quiet".into()
        }

        fn title(&self) -> String {
            "Quiet".into()
        }

        fn icon_name(&self) -> String {
            "quiet".into()
        }

        /// The drawn icon too, for panels that cannot find the themed one.
        fn icon_pixmap(&self) -> Vec<ksni::Icon> {
            [22, 32, 48]
                .into_iter()
                .map(|size| {
                    // RGBA to ARGB, which StatusNotifierItem expects.
                    let mut data = crate::icon::rgba(size);
                    data.chunks_exact_mut(4).for_each(|px| px.rotate_right(1));
                    ksni::Icon { width: size as i32, height: size as i32, data }
                })
                .collect()
        }

        fn activate(&mut self, _x: i32, _y: i32) {
            self.0.try_send(SystemEvent::Open).ok();
        }

        fn menu(&self) -> Vec<MenuItem<Self>> {
            let item = |label: &str, event: SystemEvent| {
                StandardItem {
                    label: label.into(),
                    activate: Box::new(move |tray: &mut Self| {
                        tray.0.try_send(event).ok();
                    }),
                    ..Default::default()
                }
                .into()
            };
            vec![
                item("Open", SystemEvent::Open),
                item("Quick capture", SystemEvent::QuickCapture),
                MenuItem::Separator,
                item("Quit", SystemEvent::Quit),
            ]
        }
    }

    pub struct System {
        _tray: Handle<Tray>,
    }

    impl System {
        pub fn start(sender: async_channel::Sender<SystemEvent>) -> anyhow::Result<Self> {
            let tray = Tray(sender)
                .spawn()
                .map_err(|err| anyhow::anyhow!("no tray available ({err})"))?;
            Ok(Self { _tray: tray })
        }

        pub fn set_hotkey(&mut self, _spec: &str) -> anyhow::Result<()> {
            anyhow::bail!("global shortcuts are set in your desktop's settings on Linux")
        }
    }
}
