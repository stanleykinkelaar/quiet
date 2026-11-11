//! The Projects screen (a flat list) and the project edit dialog.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use quiet_store::{Project, ProjectId};

use super::{MainView, Screen, style};
use crate::data;

impl MainView {
    pub(super) fn render_projects(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let subtle = style::subtle(cx);
        let rows: Vec<AnyElement> = self
            .project_list
            .iter()
            .enumerate()
            .map(|(index, project)| {
                let id = project.id;
                let next = self.project_next.get(&id).cloned();
                h_flex()
                    .id(("project", id as u64))
                    .gap_3()
                    .px_3()
                    .py(px(12.))
                    .rounded(cx.theme().radius)
                    .cursor_pointer()
                    .when(index == self.project_selected, |this| this.bg(style::selection(cx)))
                    .when(index != self.project_selected, |this| this.hover(|s| s.bg(cx.theme().list_hover)))
                    .when(project.archived, |this| this.opacity(0.6))
                    .child(div().flex_none().size(px(12.)).rounded_full().bg(style::hex(&project.color)))
                    .child(div().text_base().child(project.name.clone()))
                    .children(next.map(|title| {
                        div().flex_1().min_w_0().truncate().text_sm().text_color(subtle).child(format!("Next: {title}"))
                    }))
                    .when(project.archived, |this| {
                        this.child(div().text_xs().text_color(subtle).child("Archived"))
                    })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.project_selected = index;
                        this.show(Screen::Project(id), window, cx);
                    }))
                    .into_any_element()
            })
            .collect();

        v_flex()
            .id("projects")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .gap_1()
            .children(rows)
            .when(self.project_list.is_empty(), |this| {
                this.child(
                    div()
                        .py_6()
                        .px_3()
                        .text_color(subtle)
                        .child("No projects yet. Type a name above and press Enter."),
                )
            })
            .child(
                h_flex().pt_4().px_1().child(
                    Button::new("toggle-archived")
                        .ghost()
                        .xsmall()
                        .label(if self.show_archived { "Hide archived" } else { "Show archived" })
                        .text_color(subtle)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.show_archived = !this.show_archived;
                            this.reload(cx);
                        })),
                ),
            )
    }
}

pub fn open_editor(project: &Project, window: &mut Window, cx: &mut App) {
    let editor = cx.new(|cx| ProjectEditor::new(project, window, cx));
    let name = editor.read(cx).name.clone();
    let archived = project.archived;
    window.open_dialog(cx, move |dialog, _, _| {
        let save = editor.clone();
        let archive = editor.clone();
        dialog
            .title("Edit project")
            .w(px(460.))
            .child(editor.clone())
            .on_ok({
                let editor = editor.clone();
                move |_, _, cx| {
                    editor.update(cx, |editor, cx| editor.save(cx));
                    true
                }
            })
            .footer(
                DialogFooter::new()
                    .justify_between()
                    .child(
                        Button::new("archive-project")
                            .ghost()
                            .label(if archived { "Unarchive" } else { "Archive" })
                            .tooltip(if archived {
                                "Bring this project back"
                            } else {
                                "Hide this project. Nothing is deleted."
                            })
                            .on_click(move |_, window, cx| {
                                let id = archive.read(cx).id;
                                data::change(cx, |s| s.set_project_archived(id, !archived));
                                window.close_dialog(cx);
                            }),
                    )
                    .child(Button::new("save-project").primary().label("Save").on_click(
                        move |_, window, cx| {
                            save.update(cx, |editor, cx| editor.save(cx));
                            window.close_dialog(cx);
                        },
                    )),
            )
    });
    name.update(cx, |input, cx| input.focus(window, cx));
}

struct ProjectEditor {
    id: ProjectId,
    name: Entity<InputState>,
    color: String,
    _subscriptions: Vec<Subscription>,
}

impl ProjectEditor {
    fn new(project: &Project, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let name = cx.new(|cx| InputState::new(window, cx).default_value(project.name.clone()));
        let subscriptions = vec![cx.subscribe_in(&name, window, |this, _, event, window, cx| {
            if let InputEvent::PressEnter { .. } = event {
                this.save(cx);
                window.close_dialog(cx);
            }
        })];
        Self { id: project.id, name, color: project.color.clone(), _subscriptions: subscriptions }
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        let name = self.name.read(cx).value().trim().to_string();
        if name.is_empty() {
            return;
        }
        let (id, color) = (self.id, self.color.clone());
        data::change(cx, |s| s.update_project(id, &name, &color));
    }
}

impl Render for ProjectEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let swatches = style::PROJECT_COLORS.iter().map(|&hex| {
            let selected = self.color.eq_ignore_ascii_case(hex);
            div()
                .id(SharedString::from(format!("color-{hex}")))
                .size(px(28.))
                .rounded_full()
                .cursor_pointer()
                .flex()
                .items_center()
                .justify_center()
                .border_2()
                .border_color(if selected { cx.theme().foreground } else { gpui_kit::transparent_black() })
                .child(div().size(px(20.)).rounded_full().bg(style::hex(hex)))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.color = hex.to_string();
                    cx.notify();
                }))
        });
        v_flex()
            .gap_4()
            .child(Input::new(&self.name).large())
            .child(h_flex().gap_2().children(swatches))
    }
}
