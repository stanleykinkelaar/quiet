//! The edit dialog: everything about one task, on one calm card.

use chrono::Datelike as _;
use gpui_kit::component::IndexPath;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui_kit::component::select::{Select, SelectState};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::*;
use quiet_store::{LEAD_DAYS, ProjectId, Repeat, Task, TaskEdit, TaskId, parse_day, parse_time};

use crate::data;

pub fn open(task: &Task, window: &mut Window, cx: &mut App) {
    let editor = cx.new(|cx| TaskEditor::new(task, window, cx));
    let title = editor.read(cx).title.clone();
    window.open_dialog(cx, move |dialog, window, _| {
        let save = editor.clone();
        let delete = editor.clone();
        dialog
            .title("Edit task")
            .w(super::style::dialog_width(540., window))
            .child(editor.clone())
            .on_ok({
                let editor = editor.clone();
                move |_, _, cx| editor.update(cx, |editor, cx| editor.save(cx))
            })
            .footer(
                DialogFooter::new()
                    .justify_between()
                    .child(Button::new("delete-task").ghost().label("Delete").on_click(
                        move |_, window, cx| {
                            let id = delete.read(cx).id;
                            window.close_dialog(cx);
                            super::delete_task(id, window, cx);
                        },
                    ))
                    .child(Button::new("save-task").primary().label("Save").on_click(
                        move |_, window, cx| {
                            if save.update(cx, |editor, cx| editor.save(cx)) {
                                window.close_dialog(cx);
                            }
                        },
                    )),
            )
    });
    // The dialog takes focus when it opens; hand it to the title field.
    title.update(cx, |input, cx| input.focus(window, cx));
}

pub struct TaskEditor {
    id: TaskId,
    task: Task,
    title: Entity<InputState>,
    note: Entity<TextareaState>,
    /// "Inbox" first, then every active project.
    project_ids: Vec<Option<ProjectId>>,
    project: Entity<SelectState<Vec<SharedString>>>,
    repeat_options: Vec<Repeat>,
    repeat: Entity<SelectState<Vec<SharedString>>>,
    reminder: Entity<InputState>,
    reminder_error: bool,
    day: Entity<InputState>,
    day_error: bool,
    lead: Entity<SelectState<Vec<SharedString>>>,
    _subscriptions: Vec<Subscription>,
}

impl TaskEditor {
    fn new(task: &Task, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let title = cx.new(|cx| InputState::new(window, cx).default_value(task.title.clone()));
        let note = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(3, 10)
                .placeholder("Note (optional)")
                .default_value(task.note.clone().unwrap_or_default())
        });

        let mut project_ids = vec![None];
        let mut names: Vec<SharedString> = vec!["Inbox".into()];
        for p in data::read(cx, |s| s.projects(false)) {
            project_ids.push(Some(p.id));
            names.push(p.name.into());
        }
        let current = project_ids.iter().position(|&p| p == task.project_id).unwrap_or(0);
        let project = cx.new(|cx| SelectState::new(names, Some(IndexPath::new(current)), window, cx));

        // "Weekly" means the weekday it already repeats on, or today's.
        let weekday = match task.repeat {
            Repeat::Weekly(day) => day,
            _ => data::today().weekday(),
        };
        let repeat_options = vec![Repeat::Never, Repeat::Daily, Repeat::Weekdays, Repeat::Weekly(weekday)];
        let labels: Vec<SharedString> = repeat_options.iter().map(|r| r.label().into()).collect();
        let current = repeat_options.iter().position(|&r| r == task.repeat).unwrap_or(0);
        let repeat = cx.new(|cx| SelectState::new(labels, Some(IndexPath::new(current)), window, cx));

        let reminder = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("No reminder")
                .default_value(task.reminder.map(|t| t.format("%H:%M").to_string()).unwrap_or_default())
        });

        let day = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("No day")
                .default_value(task.day.map(|d| d.format("%-d %b %Y").to_string()).unwrap_or_default())
        });
        let lead_labels: Vec<SharedString> = LEAD_DAYS.iter().map(|&d| lead_label(d).into()).collect();
        let current = LEAD_DAYS.iter().position(|&d| d == task.lead_days).unwrap_or(0);
        let lead = cx.new(|cx| SelectState::new(lead_labels, Some(IndexPath::new(current)), window, cx));

        let subscriptions = vec![
            cx.subscribe_in(&day, window, |this, _, event, window, cx| match event {
                InputEvent::PressEnter { .. } => {
                    if this.save(cx) {
                        window.close_dialog(cx);
                    }
                }
                InputEvent::Change => {
                    this.day_error = false;
                    cx.notify();
                }
                _ => {}
            }),
            cx.subscribe_in(&title, window, |this, _, event, window, cx| {
                if let InputEvent::PressEnter { .. } = event
                    && this.save(cx)
                {
                    window.close_dialog(cx);
                }
            }),
            cx.subscribe_in(&reminder, window, |this, _, event, window, cx| match event {
                InputEvent::PressEnter { .. } => {
                    if this.save(cx) {
                        window.close_dialog(cx);
                    }
                }
                InputEvent::Change => {
                    this.reminder_error = false;
                    cx.notify();
                }
                _ => {}
            }),
        ];
        Self {
            id: task.id,
            task: task.clone(),
            title,
            note,
            project_ids,
            project,
            repeat_options,
            repeat,
            reminder,
            reminder_error: false,
            day,
            day_error: false,
            lead,
            _subscriptions: subscriptions,
        }
    }

    fn selected<T: Copy>(state: &Entity<SelectState<Vec<SharedString>>>, options: &[T], cx: &App) -> Option<T> {
        state.read(cx).selected_index(cx).and_then(|ix| options.get(ix.row).copied())
    }

    /// Saves, or returns false (and says why) when the reminder time
    /// cannot be read.
    fn save(&mut self, cx: &mut Context<Self>) -> bool {
        let reminder_text = self.reminder.read(cx).value().trim().to_string();
        let reminder = if reminder_text.is_empty() {
            None
        } else {
            match parse_time(&reminder_text) {
                Some(time) => Some(time),
                None => {
                    self.reminder_error = true;
                    cx.notify();
                    return false;
                }
            }
        };
        let day_text = self.day.read(cx).value().trim().to_string();
        let day = if day_text.is_empty() {
            None
        } else {
            match parse_day(&day_text, data::today()) {
                Some(day) => Some(day),
                None => {
                    self.day_error = true;
                    cx.notify();
                    return false;
                }
            }
        };
        let title = self.title.read(cx).value().trim().to_string();
        let note = self.note.read(cx).value().trim().to_string();
        let edit = TaskEdit {
            title: if title.is_empty() { self.task.title.clone() } else { title },
            note: (!note.is_empty()).then_some(note),
            project_id: Self::selected(&self.project, &self.project_ids, cx).unwrap_or(self.task.project_id),
            repeat: Self::selected(&self.repeat, &self.repeat_options, cx).unwrap_or(self.task.repeat),
            reminder,
            day,
            lead_days: Self::selected(&self.lead, LEAD_DAYS, cx).unwrap_or(0),
        };
        let id = self.id;
        data::change(cx, |s| s.update_task(id, &edit, data::today()));
        true
    }
}

impl Render for TaskEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let subtle = cx.theme().muted_foreground;
        let reminder_hint = if self.reminder_error {
            "Use a time like 9:30 or 14:00."
        } else {
            "Rings once on days the task is in Today. Setting one puts it in Today."
        };
        let day_hint = if self.day_error {
            "Try fri, tomorrow, +3, 14 nov or 2026-11-14."
        } else {
            "It comes into Today on its own. Nothing turns red after."
        };
        v_flex()
            .gap_3()
            .child(Input::new(&self.title).large())
            .child(Textarea::new(&self.note))
            .child(field("Project", div().max_w(px(260.)).child(Select::new(&self.project).w_full()).into_any_element(), cx))
            .child(field("Repeat", div().max_w(px(260.)).child(Select::new(&self.repeat).w_full()).into_any_element(), cx))
            .child(field(
                "Day",
                v_flex()
                    .gap_1()
                    .child(
                        h_flex()
                            .flex_wrap()
                            .gap_2()
                            .child(div().w(px(140.)).child(Input::new(&self.day)))
                            .child(div().w(px(160.)).child(Select::new(&self.lead).w_full())),
                    )
                    .child(div().text_xs().text_color(subtle).child(day_hint))
                    .into_any_element(),
                cx,
            ))
            .child(field(
                "Reminder",
                v_flex()
                    .gap_1()
                    .child(div().w(px(140.)).child(Input::new(&self.reminder)))
                    .child(div().text_xs().text_color(subtle).child(reminder_hint))
                    .into_any_element(),
                cx,
            ))
            .child(div().pt_1().text_xs().text_color(subtle).child("Enter saves. Esc closes."))
    }
}

fn lead_label(days: i64) -> String {
    match days {
        0 => "On the day".into(),
        1 => "1 day before".into(),
        7 => "1 week before".into(),
        n => format!("{n} days before"),
    }
}

/// A quiet label on the left, the control on the right. The control takes
/// the room it is given, so it shrinks in a narrow window.
fn field(label: &'static str, control: AnyElement, cx: &App) -> impl IntoElement {
    h_flex()
        .items_start()
        .gap_4()
        .child(
            div()
                .flex_none()
                .w(px(80.))
                .pt_1p5()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(label),
        )
        .child(div().flex_1().min_w_0().max_w(px(340.)).child(control))
}
