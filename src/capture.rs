//! Quick capture: one text field in a small centred window. Enter saves to
//! the Inbox and closes; Esc closes. Nothing else, so the thought survives.

use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, Theme, v_flex};
use gpui_kit::*;

use crate::data;

actions!(capture, [CloseCapture]);

const CONTEXT: &str = "Capture";

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("escape", CloseCapture, Some(CONTEXT)),
        KeyBinding::new("escape", CloseCapture, Some("Capture > Input")),
    ]);
}

struct CaptureWindow(Option<AnyWindowHandle>);
impl Global for CaptureWindow {}

pub fn open(cx: &mut App) {
    if let Some(Some(handle)) = cx.try_global::<CaptureWindow>().map(|w| w.0)
        && handle.update(cx, |_, window, _| window.activate_window()).is_ok()
    {
        return;
    }
    let bounds = size(px(560.), px(108.));
    let options = || WindowOptions {
        titlebar: None,
        kind: WindowKind::PopUp,
        is_resizable: false,
        is_minimizable: false,
        focus: true,
        show: true,
        app_id: Some("quiet-capture".into()),
        ..Default::default()
    };
    // On Wayland a normal window cannot place itself, and tiling desktops
    // like Hyprland would tile it. An overlay layer, as launchers use, sits
    // centred above everything and takes the keyboard. Desktops without
    // layer shell (GNOME) get the normal popup.
    #[cfg(target_os = "linux")]
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        let overlay = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds { origin: Point::default(), size: bounds })),
            kind: WindowKind::LayerShell(layer_shell::LayerShellOptions {
                namespace: "quiet-capture".into(),
                layer: layer_shell::Layer::Overlay,
                keyboard_interactivity: layer_shell::KeyboardInteractivity::Exclusive,
                ..Default::default()
            }),
            ..options()
        };
        if let Ok(handle) = open_capture(overlay, cx) {
            cx.set_global(CaptureWindow(Some(handle)));
            return;
        }
    }
    let popup = WindowOptions { window_bounds: Some(WindowBounds::centered(bounds, cx)), ..options() };
    match open_capture(popup, cx) {
        Ok(handle) => cx.set_global(CaptureWindow(Some(handle))),
        Err(err) => data::log_error(&err),
    }
}

fn open_capture(options: WindowOptions, cx: &mut App) -> anyhow::Result<AnyWindowHandle> {
    let opened = gpui_kit::open_window(options, cx, |window, cx| {
        Theme::sync_system_appearance(Some(window), cx);
        window.on_window_should_close(cx, |_, cx| {
            cx.set_global(CaptureWindow(None));
            true
        });
        cx.new(|cx| CaptureView::new(window, cx))
    });
    Ok(opened?.0)
}

fn close(window: &mut Window, cx: &mut App) {
    cx.set_global(CaptureWindow(None));
    window.remove_window();
}

struct CaptureView {
    input: Entity<InputState>,
    _subscriptions: Vec<Subscription>,
}

impl CaptureView {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("What's on your mind?"));
        let subscriptions = vec![cx.subscribe_in(&input, window, |_, input, event, window, cx| {
            if let InputEvent::PressEnter { .. } = event {
                let title = input.read(cx).value().trim().to_string();
                if !title.is_empty() {
                    data::change(cx, |s| s.add_task(&title, None, false));
                }
                close(window, cx);
            }
        })];
        input.update(cx, |input, cx| input.focus(window, cx));
        Self { input, _subscriptions: subscriptions }
    }
}

impl Render for CaptureView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .key_context(CONTEXT)
            .on_action(|_: &CloseCapture, window, cx| close(window, cx))
            .size_full()
            .justify_center()
            .gap_2()
            .px_5()
            .bg(cx.theme().background)
            .border_1()
            .border_color(cx.theme().border)
            .text_color(cx.theme().foreground)
            .child(Input::new(&self.input).large())
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Enter saves to Inbox. Esc cancels."),
            )
    }
}
