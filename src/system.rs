//! Tray icon and global hotkey.
//!
//! Both crates create hidden Win32 windows and need a message loop on the
//! thread that created them. GPUI runs a standard `GetMessageW` /
//! `DispatchMessageW` loop on the main thread, so creating them inside
//! `Application::run` is enough. Their callbacks run inside that loop; they
//! only forward a `SystemEvent` into a channel that a GPUI foreground task
//! awaits, so no polling timer is needed.

use std::str::FromStr;

use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState, hotkey::HotKey};
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemEvent {
    Open,
    Quit,
    QuickCapture,
}

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
            .with_menu_on_left_click(false)
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

    /// Registers `spec` (for example "Ctrl+Alt+Space") as the quick-capture
    /// hotkey, replacing the previous one.
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
