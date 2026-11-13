//! Quick capture: one text field in a small centred window. Enter saves to
//! the Inbox and closes; Esc cancels. Clicking elsewhere, or the shortcut
//! again, closes it too, and keeps what you typed for next time. Nothing
//! else, so the thought survives.

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

/// Text typed but not saved. Clicking away or pressing the shortcut again
/// keeps it for the next capture; Enter and Esc clear it.
#[derive(Default)]
struct Draft(String);
impl Global for Draft {}

/// The capture shortcut: opens quick capture, or closes it when it is
/// already open. Closing this way works even when the window did not get
/// the keyboard, which is when Esc cannot reach it.
pub fn toggle(cx: &mut App) {
    if let Some(Some(handle)) = cx.try_global::<CaptureWindow>().map(|w| w.0)
        && handle.update(cx, |_, window, cx| close(window, cx)).is_ok()
    {
        return;
    }
    open(cx);
}

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
            // The overlay asks for the keyboard itself (Exclusive above).
            cx.set_global(CaptureWindow(Some(handle)));
            return;
        }
    }
    let popup = WindowOptions { window_bounds: Some(WindowBounds::centered(bounds, cx)), ..options() };
    match open_capture(popup, cx) {
        Ok(handle) => {
            cx.set_global(CaptureWindow(Some(handle)));
            take_keyboard(handle, cx);
        }
        Err(err) => data::log_error(&err),
    }
}

/// A new window does not get the keyboard by itself. Windows only lets the
/// app that had the last input come to the front, and macOS does not
/// activate a menu bar app on its own. Without this you have to click the
/// field before you can type. Activating the window works around both:
/// on Windows GPUI simulates a key press first, which is what lets it
/// come to the front.
fn take_keyboard(handle: AnyWindowHandle, cx: &mut App) {
    cx.activate(true);
    handle.update(cx, |_, window, _| window.activate_window()).ok();
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
    /// The window had the keyboard at some point. Only then does losing it
    /// mean you clicked elsewhere.
    was_active: bool,
    _subscriptions: Vec<Subscription>,
}

impl CaptureView {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let draft = cx.try_global::<Draft>().map(|d| d.0.clone()).unwrap_or_default();
        let input =
            cx.new(|cx| InputState::new(window, cx).placeholder("What's on your mind?").default_value(draft));
        let subscriptions = vec![cx.subscribe_in(&input, window, |_, input, event, window, cx| match event {
            InputEvent::Change => cx.set_global(Draft(input.read(cx).value().to_string())),
            InputEvent::PressEnter { .. } => {
                let title = input.read(cx).value().trim().to_string();
                if !title.is_empty() {
                    data::change(cx, |s| s.add_task(&title, None, false));
                }
                cx.set_global(Draft::default());
                close(window, cx);
            }
            _ => {}
        })];
        let subscriptions = subscriptions
            .into_iter()
            // Keys only reach Esc's binding while something inside this view
            // has focus, so when the window gets the keyboard, the text
            // field gets it. Losing the keyboard after that means you
            // clicked elsewhere: close, and the draft waits for next time.
            .chain([cx.observe_window_activation(window, |this: &mut Self, window, cx| {
                if window.is_window_active() {
                    this.was_active = true;
                    this.focus_input(window, cx);
                } else if this.was_active {
                    close(window, cx);
                }
            })])
            .collect();
        let view = Self { input, was_active: window.is_window_active(), _subscriptions: subscriptions };
        view.focus_input(window, cx);
        view
    }

    fn focus_input(&self, window: &mut Window, cx: &mut App) {
        self.input.update(cx, |input, cx| input.focus(window, cx));
    }
}

impl Render for CaptureView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Focus can get lost (a click on the border, the system taking it
        // back). Without it Esc does nothing, so take it back right away.
        if window.is_window_active() && window.focused(cx).is_none() {
            let input = self.input.clone();
            window.defer(cx, move |window, cx| input.update(cx, |input, cx| input.focus(window, cx)));
        }
        v_flex()
            .key_context(CONTEXT)
            .on_action(|_: &CloseCapture, window, cx| {
                cx.set_global(Draft::default());
                close(window, cx);
            })
            // A click anywhere in the window puts the cursor in the field.
            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| this.focus_input(window, cx)))
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
                    .child("Enter saves to Inbox. Esc cancels. Click away and it waits for next time."),
            )
    }
}
