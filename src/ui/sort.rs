//! Sorting the Inbox: one task at a time, one key per decision. Deciding
//! about a single thing is easier than facing a whole list.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, StyledExt as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use quiet_store::{Project, Task};

use super::style;
use crate::data;

pub fn open(window: &mut Window, cx: &mut App) {
    let queue = data::read(cx, |s| s.inbox_to_sort());
    let sorter = cx.new(|cx| InboxSorter::new(queue, cx));
    let focus = sorter.read(cx).focus.clone();
    window.open_dialog(cx, move |dialog, _, _| dialog.title("Sort the Inbox").w(px(560.)).child(sorter.clone()));
    window.focus(&focus, cx);
}

/// What to do with the task on screen.
#[derive(Clone, Copy)]
enum Decision {
    Today,
    /// Looked at, and staying in the Inbox on purpose.
    Keep,
    Project(usize),
    Done,
    Delete,
    /// No decision now; it stays unsorted.
    Skip,
}

struct InboxSorter {
    queue: Vec<Task>,
    index: usize,
    decided: usize,
    projects: Vec<Project>,
    focus: FocusHandle,
}

impl InboxSorter {
    fn new(queue: Vec<Task>, cx: &mut Context<Self>) -> Self {
        let projects = data::read(cx, |s| s.projects(false));
        Self { queue, index: 0, decided: 0, projects, focus: cx.focus_handle() }
    }

    fn decide(&mut self, decision: Decision, cx: &mut Context<Self>) {
        let Some(task) = self.queue.get(self.index) else { return };
        let id = task.id;
        match decision {
            Decision::Today => {
                data::change(cx, |s| {
                    s.set_in_today(id, true)?;
                    s.mark_reviewed(id)
                });
            }
            Decision::Keep => {
                data::change(cx, |s| s.mark_reviewed(id));
            }
            Decision::Project(n) => {
                let Some(project) = self.projects.get(n) else { return };
                let project = project.id;
                data::change(cx, |s| s.move_to_project(id, project));
            }
            Decision::Done => {
                data::change(cx, |s| s.complete(id, quiet_store::now()));
            }
            Decision::Delete => {
                data::change(cx, |s| s.delete_task(id));
            }
            Decision::Skip => {}
        }
        if !matches!(decision, Decision::Skip) {
            self.decided += 1;
        }
        self.index += 1;
        cx.notify();
    }

    fn on_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        let m = &keystroke.modifiers;
        if m.control || m.alt || m.platform || m.function {
            return;
        }
        let decision = match keystroke.key.as_str() {
            "t" => Decision::Today,
            "k" => Decision::Keep,
            "space" => Decision::Done,
            "delete" | "backspace" => Decision::Delete,
            "right" | "n" => Decision::Skip,
            "enter" if self.index >= self.queue.len() => {
                window.close_dialog(cx);
                return;
            }
            key => match key.parse::<usize>() {
                Ok(n @ 1..=9) if n <= self.projects.len() => Decision::Project(n - 1),
                _ => return,
            },
        };
        cx.stop_propagation();
        self.decide(decision, cx);
    }

    fn render_finished(&self, cx: &mut Context<Self>) -> AnyElement {
        let message = match (self.queue.len(), self.decided) {
            (0, _) => "Nothing to sort. Your Inbox is clear.".to_string(),
            (_, 0) => "That was all of them. Nothing changed.".to_string(),
            (_, 1) => "Done. You decided about 1 task.".to_string(),
            (_, n) => format!("Done. You decided about {n} tasks."),
        };
        v_flex()
            .gap_4()
            .py_4()
            .child(div().text_lg().child(message))
            .child(
                h_flex().child(
                    Button::new("sort-close")
                        .primary()
                        .label("Close")
                        .on_click(|_, window, cx| window.close_dialog(cx)),
                ),
            )
            .child(div().text_xs().text_color(style::subtle(cx)).child("Enter or Esc closes."))
            .into_any_element()
    }

    fn render_task(&self, task: &Task, cx: &mut Context<Self>) -> AnyElement {
        let subtle = style::subtle(cx);
        let choice = |id: &'static str, label: &'static str, key: &'static str, decision: Decision, cx: &mut Context<Self>| {
            Button::new(id)
                .small()
                .label(format!("{label}  {key}"))
                .on_click(cx.listener(move |this, _, _, cx| this.decide(decision, cx)))
        };
        let projects: Vec<AnyElement> = self
            .projects
            .iter()
            .take(9)
            .enumerate()
            .map(|(n, project)| {
                let color = style::hex(&project.color);
                h_flex()
                    .id(("sort-project", n))
                    .gap_1p5()
                    .px_2()
                    .py_1()
                    .rounded_full()
                    .bg(color.opacity(0.14))
                    .text_sm()
                    .cursor_pointer()
                    .hover(|s| s.bg(color.opacity(0.28)))
                    .child(div().font_semibold().text_color(subtle).child(format!("{}", n + 1)))
                    .child(div().size(px(7.)).rounded_full().bg(color))
                    .child(project.name.clone())
                    .on_click(cx.listener(move |this, _, _, cx| this.decide(Decision::Project(n), cx)))
                    .into_any_element()
            })
            .collect();

        v_flex()
            .gap_5()
            .child(
                div()
                    .text_xs()
                    .text_color(subtle)
                    .child(format!("{} of {}", self.index + 1, self.queue.len())),
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(div().text_xl().child(task.title.clone()))
                    .children(task.note.clone().map(|note| div().text_sm().text_color(subtle).child(note))),
            )
            .child(
                h_flex()
                    .flex_wrap()
                    .gap_2()
                    .child(choice("sort-today", "Today", "T", Decision::Today, cx).primary())
                    .child(choice("sort-keep", "Keep in Inbox", "K", Decision::Keep, cx))
                    .child(choice("sort-done", "Already done", "Space", Decision::Done, cx))
                    .child(choice("sort-delete", "Delete", "Del", Decision::Delete, cx).ghost())
                    .child(choice("sort-skip", "Skip", "→", Decision::Skip, cx).ghost()),
            )
            .when(!projects.is_empty(), |this| {
                this.child(
                    v_flex()
                        .gap_2()
                        .child(div().text_sm().text_color(subtle).child("Or move it to a project"))
                        .child(h_flex().flex_wrap().gap_2().children(projects)),
                )
            })
            .into_any_element()
    }
}

impl Render for InboxSorter {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body = match self.queue.get(self.index).cloned() {
            Some(task) => self.render_task(&task, cx),
            None => self.render_finished(cx),
        };
        div()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::on_key))
            .text_color(cx.theme().foreground)
            .child(body)
    }
}
