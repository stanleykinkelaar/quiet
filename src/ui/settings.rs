//! Settings: the capture shortcut, starting at sign-in and updates.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::h_flex;
#[cfg(not(target_os = "linux"))]
use gpui_kit::component::input::Input;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme as _, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::{MainView, style};
use crate::update::{self, State};
use crate::{data, platform};

pub const DEFAULT_HOTKEY: &str = "Ctrl+Alt+Space";

pub fn saved_hotkey(cx: &App) -> String {
    data::read(cx, |s| s.setting("hotkey")).unwrap_or_else(|| DEFAULT_HOTKEY.to_string())
}

impl MainView {
    pub(super) fn apply_hotkey(&mut self, cx: &mut Context<Self>) {
        let spec = self.hotkey_input.read(cx).value().trim().to_string();
        self.settings_note = Some(match crate::set_hotkey(&spec, cx) {
            Ok(()) => {
                data::change(cx, |s| s.set_setting("hotkey", &spec));
                format!("Saved. {spec} opens quick capture.").into()
            }
            Err(err) => format!("That shortcut did not work ({err}). Try something like Ctrl+Alt+Space.").into(),
        });
        cx.notify();
    }

    /// Windows and macOS: Quiet registers the shortcut itself.
    #[cfg(not(target_os = "linux"))]
    fn render_hotkey(&self, cx: &mut Context<Self>) -> AnyElement {
        let subtle = style::subtle(cx);
        v_flex()
            .gap_2()
            .child(
                h_flex()
                    .gap_2()
                    .child(div().flex_1().min_w_0().max_w(px(260.)).child(Input::new(&self.hotkey_input)))
                    .child(
                        Button::new("apply-hotkey")
                            .label("Save")
                            .on_click(cx.listener(|this, _, _, cx| this.apply_hotkey(cx))),
                    ),
            )
            .children(
                self.settings_note
                    .clone()
                    .map(|note| div().text_sm().text_color(subtle).child(note)),
            )
            .into_any_element()
    }

    /// Linux: the desktop owns global shortcuts, so bind one there.
    #[cfg(target_os = "linux")]
    fn render_hotkey(&self, cx: &mut Context<Self>) -> AnyElement {
        let subtle = style::subtle(cx);
        v_flex()
            .gap_2()
            .text_sm()
            .text_color(subtle)
            .child("Add a shortcut in your desktop's keyboard settings that runs:")
            .child(div().font_family("monospace").text_color(cx.theme().foreground).child("quiet --capture"))
            .child("On Hyprland (Omarchy), add this line to ~/.config/hypr/bindings.conf:")
            .child(
                div()
                    .font_family("monospace")
                    .text_color(cx.theme().foreground)
                    .child("bindd = CTRL ALT, SPACE, Quiet capture, exec, quiet --capture"),
            )
            .into_any_element()
    }

    /// The automatic updates switch, and a button that checks right now.
    fn render_update(&self, cx: &mut Context<Self>) -> AnyElement {
        let subtle = style::subtle(cx);
        if let Some(reason) = update::unsupported() {
            return div().text_sm().text_color(subtle).child(reason).into_any_element();
        }
        let state = update::state(cx);
        let note: Option<SharedString> = match &state {
            State::Idle | State::Checking => None,
            State::UpToDate => Some("You have the newest version.".into()),
            State::Available(version) => Some(format!("Quiet {version} is available.").into()),
            State::Installing(version) => {
                Some(format!("Downloading Quiet {version}. Quiet restarts by itself when it is done.").into())
            }
            State::Failed(message) => Some(message.clone()),
        };
        let button = match state {
            State::Available(version) => Button::new("install-update")
                .primary()
                .label(format!("Install {version}"))
                .on_click(move |_, _, cx| update::install(version.clone(), cx)),
            State::Installing(_) => Button::new("install-update").label("Installing").loading(true),
            State::Checking => Button::new("check-update").label("Checking").loading(true),
            _ => Button::new("check-update").label("Check for updates").on_click(|_, _, cx| update::check(cx)),
        };
        let auto = update::auto_enabled(cx);
        v_flex()
            .gap_3()
            .child(
                Switch::new("auto-update")
                    .checked(auto)
                    .label("Update automatically")
                    .on_change(cx.listener(|_, checked: &bool, _, cx| {
                        update::set_auto(*checked, cx);
                        cx.notify();
                    })),
            )
            .child(h_flex().child(button))
            .children(note.map(|note| div().text_sm().text_color(subtle).child(note)))
            .into_any_element()
    }

    pub(super) fn render_settings(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let subtle = style::subtle(cx);
        let section = |title: &'static str, caption: SharedString, control: AnyElement| {
            v_flex()
                .gap_2()
                .child(div().text_base().child(title))
                .child(div().text_sm().text_color(subtle).child(caption))
                .child(div().pt_1().child(control))
        };
        v_flex()
            .gap_8()
            .pt_2()
            .child(section(
                "Quick capture shortcut",
                "Opens a small window from anywhere. Type, press Enter, and it lands in your Inbox.".into(),
                self.render_hotkey(cx),
            ))
            .child(section(
                platform::AUTOSTART_TITLE,
                platform::AUTOSTART_CAPTION.into(),
                Switch::new("autostart")
                    .checked(self.autostart)
                    .label(if self.autostart { "On" } else { "Off" })
                    .on_change(cx.listener(|this, checked: &bool, _, cx| {
                        match platform::autostart::set_enabled(*checked) {
                            Ok(()) => this.autostart = *checked,
                            Err(err) => data::log_error(&err),
                        }
                        data::change(cx, |s| s.set_setting("autostart", if *checked { "1" } else { "0" }));
                        cx.notify();
                    }))
                    .into_any_element(),
            ))
            .child(section(
                "Updates",
                if update::auto_enabled(cx) {
                    format!(
                        "You have Quiet {}. A few times a day Quiet looks for a new version and installs it while its window is closed.",
                        update::CURRENT
                    )
                } else {
                    format!("You have Quiet {}. Quiet only goes online when you check.", update::CURRENT)
                }
                .into(),
                self.render_update(cx),
            ))
            .when(!platform::notifications_enabled(), |this| {
                this.child(section(
                    "Reminders",
                    "Windows notifications are switched off, so reminders cannot ring. You can turn them on in Windows Settings, System, Notifications.".into(),
                    div().into_any_element(),
                ))
            })
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!("Your list is stored in {}", data::database_path().display())),
            )
    }
}
