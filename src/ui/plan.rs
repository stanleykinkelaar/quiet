//! Planning the day: one guided pass, one question at a time. First the
//! tasks left from earlier days, then what could be done today (tasks with
//! a day coming up, one task per project, the Inbox), until the day
//! is full enough. Last, what to start with.

use std::collections::HashMap;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, StyledExt as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use quiet_store::{Project, ProjectId, Task};

use super::{TODAY_SOFT_LIMIT, day_chip, style};
use crate::data;

pub fn open(window: &mut Window, cx: &mut App) {
    let today = data::today();
    // Opening it counts: the nudge on Today goes away for the day, even if
    // you close this halfway.
    data::change(cx, |s| s.mark_planned(today));
    let planner = cx.new(Planner::new);
    let focus = planner.read(cx).focus.clone();
    window.open_dialog(cx, move |dialog, window, _| {
        dialog.title("Plan today").w(style::dialog_width(560., window)).child(planner.clone())
    });
    window.focus(&focus, cx);
}

#[derive(Clone, Copy, PartialEq)]
enum Step {
    /// Tasks left in Today from an earlier day.
    Earlier,
    /// Tasks that could go into Today.
    Pick,
    /// Which task comes first.
    Start,
    Finished,
}

#[derive(Clone, Copy)]
enum Decision {
    Today,
    NotToday,
    Done,
}

struct Planner {
    step: Step,
    earlier: Vec<Task>,
    candidates: Vec<Task>,
    /// Position within the current step's list.
    index: usize,
    /// Open Today tasks, refreshed after every decision.
    today: Vec<Task>,
    /// Why Pick ended early: the day already holds enough.
    full: bool,
    projects: HashMap<ProjectId, Project>,
    focus: FocusHandle,
}

impl Planner {
    fn new(cx: &mut Context<Self>) -> Self {
        let today = data::read(cx, |s| s.today());
        let earlier = today.iter().filter(|t| t.carried).cloned().collect();
        let candidates = data::read(cx, |s| s.plan_candidates(data::today()));
        let projects = data::read(cx, |s| s.projects(true)).into_iter().map(|p| (p.id, p)).collect();
        let mut planner = Self {
            step: Step::Earlier,
            earlier,
            candidates,
            index: 0,
            today,
            full: false,
            projects,
            focus: cx.focus_handle(),
        };
        planner.settle(cx);
        planner
    }

    fn refresh_today(&mut self, cx: &mut Context<Self>) {
        self.today = data::read(cx, |s| s.today());
    }

    /// Moves on to the next step while the current one has nothing left.
    fn settle(&mut self, cx: &mut Context<Self>) {
        loop {
            let next = match self.step {
                Step::Earlier if self.index >= self.earlier.len() => Step::Pick,
                Step::Pick if self.today.len() >= TODAY_SOFT_LIMIT => {
                    self.full = true;
                    Step::Start
                }
                Step::Pick if self.index >= self.candidates.len() => Step::Start,
                // Choosing a first task only makes sense with two or more.
                Step::Start if self.today.len() < 2 => Step::Finished,
                _ => break,
            };
            self.step = next;
            self.index = 0;
            self.refresh_today(cx);
        }
        cx.notify();
    }

    fn current(&self) -> Option<&Task> {
        match self.step {
            Step::Earlier => self.earlier.get(self.index),
            Step::Pick => self.candidates.get(self.index),
            Step::Start | Step::Finished => None,
        }
    }

    fn decide(&mut self, decision: Decision, cx: &mut Context<Self>) {
        let Some(task) = self.current() else { return };
        let id = task.id;
        let earlier = self.step == Step::Earlier;
        match decision {
            Decision::Today if earlier => {
                data::change(cx, |s| s.keep_carried(id));
            }
            Decision::Today => {
                data::change(cx, |s| s.set_in_today(id, true));
            }
            Decision::NotToday if earlier => {
                data::change(cx, |s| s.set_in_today(id, false));
            }
            Decision::NotToday => {}
            Decision::Done => {
                data::change(cx, |s| s.complete(id, quiet_store::now()));
            }
        }
        self.index += 1;
        self.refresh_today(cx);
        self.settle(cx);
    }

    /// Ends picking, whatever is left.
    fn enough(&mut self, cx: &mut Context<Self>) {
        if self.step == Step::Pick {
            self.index = self.candidates.len();
            self.settle(cx);
        }
    }

    /// Start: puts the chosen task first and finishes.
    fn start_with(&mut self, position: Option<usize>, cx: &mut Context<Self>) {
        if let Some(task) = position.and_then(|n| self.today.get(n)) {
            let id = task.id;
            data::change(cx, |s| s.move_in_today(id, 0));
        }
        self.step = Step::Finished;
        self.refresh_today(cx);
        cx.notify();
    }

    fn on_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        let m = &keystroke.modifiers;
        if m.control || m.alt || m.platform || m.function {
            return;
        }
        let key = keystroke.key.as_str();
        match self.step {
            Step::Earlier | Step::Pick => {
                let decision = match key {
                    "t" | "enter" => Decision::Today,
                    "n" | "right" => Decision::NotToday,
                    "space" => Decision::Done,
                    "s" if self.step == Step::Pick => {
                        cx.stop_propagation();
                        return self.enough(cx);
                    }
                    _ => return,
                };
                cx.stop_propagation();
                self.decide(decision, cx);
            }
            Step::Start => {
                let position = match key {
                    "enter" => None,
                    key => match key.parse::<usize>() {
                        Ok(n @ 1..=9) if n <= self.today.len() => Some(n - 1),
                        _ => return,
                    },
                };
                cx.stop_propagation();
                self.start_with(position, cx);
            }
            Step::Finished => {
                if key == "enter" {
                    cx.stop_propagation();
                    window.close_dialog(cx);
                }
            }
        }
    }

    /// Why this task is offered, in a few words.
    fn reason(&self, task: &Task) -> String {
        let today = data::today();
        match (self.step, task.day, task.project_id.and_then(|p| self.projects.get(&p))) {
            (Step::Earlier, _, _) => "Left from an earlier day".into(),
            (_, Some(day), _) => {
                // "for tomorrow", but "for Friday" and "for 14 Nov".
                let chip = day_chip(day, today);
                match chip.as_str() {
                    "Today" | "Tomorrow" | "Yesterday" => format!("For {}", chip.to_lowercase()),
                    _ => format!("For {chip}"),
                }
            }
            (_, None, Some(project)) if task.is_next_action => format!("Next action in {}", project.name),
            (_, None, Some(project)) => format!("Waiting longest in {}", project.name),
            (_, None, None) => "From your Inbox".into(),
        }
    }

    fn render_task(&self, task: &Task, cx: &mut Context<Self>) -> AnyElement {
        let subtle = style::subtle(cx);
        let earlier = self.step == Step::Earlier;
        let choice = |id: &'static str, label: &'static str, key: &'static str, decision: Decision, cx: &mut Context<Self>| {
            Button::new(id)
                .small()
                .label(format!("{label}  {key}"))
                .on_click(cx.listener(move |this, _, _, cx| this.decide(decision, cx)))
        };
        let (list, heading) = if earlier {
            (&self.earlier, "From earlier days")
        } else {
            (&self.candidates, "What could you do today?")
        };
        let count = self.today.iter().filter(|t| !t.carried).count();
        let progress = if earlier {
            format!("{} of {}", self.index + 1, list.len())
        } else {
            format!("{} of {}   ·   {} for today", self.index + 1, list.len(), count)
        };

        v_flex()
            .gap_5()
            .child(
                h_flex()
                    .justify_between()
                    .text_xs()
                    .text_color(subtle)
                    .child(heading)
                    .child(progress),
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(div().text_xl().child(task.title.clone()))
                    .child(div().text_sm().text_color(subtle).child(self.reason(task)))
                    .children(task.note.clone().map(|note| div().text_sm().text_color(subtle).child(note))),
            )
            .child(
                h_flex()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        choice("plan-today", if earlier { "Keep for today" } else { "Today" }, "T", Decision::Today, cx)
                            .primary(),
                    )
                    .child(choice("plan-not-today", "Not today", "N", Decision::NotToday, cx))
                    .child(choice("plan-done", "Already done", "Space", Decision::Done, cx).ghost()),
            )
            .when(!earlier, |this| {
                this.child(
                    h_flex().child(
                        Button::new("plan-enough")
                            .ghost()
                            .xsmall()
                            .label("That is enough for today  S")
                            .text_color(subtle)
                            .on_click(cx.listener(|this, _, _, cx| this.enough(cx))),
                    ),
                )
            })
            .into_any_element()
    }

    fn render_start(&self, cx: &mut Context<Self>) -> AnyElement {
        let subtle = style::subtle(cx);
        let rows: Vec<AnyElement> = self
            .today
            .iter()
            .take(9)
            .enumerate()
            .map(|(n, task)| {
                h_flex()
                    .id(("plan-start", n))
                    .gap_3()
                    .px_3()
                    .py_2()
                    .rounded(cx.theme().radius)
                    .cursor_pointer()
                    .hover(|s| s.bg(cx.theme().list_hover))
                    .child(div().flex_none().w(px(14.)).font_semibold().text_color(subtle).child(format!("{}", n + 1)))
                    .child(div().flex_1().min_w_0().truncate().child(task.title.clone()))
                    .on_click(cx.listener(move |this, _, _, cx| this.start_with(Some(n), cx)))
                    .into_any_element()
            })
            .collect();
        let intro = if self.full {
            format!("You have {} for today. That is a full day.", self.today.len())
        } else {
            format!("You have {} for today.", self.today.len())
        };
        v_flex()
            .gap_4()
            .child(v_flex().gap_1().child(div().text_sm().text_color(subtle).child(intro)).child(
                div().text_xl().child("What do you start with?"),
            ))
            .child(v_flex().gap_0p5().children(rows))
            .child(div().text_xs().text_color(subtle).child("Press its number. Enter keeps the order as it is."))
            .into_any_element()
    }

    fn render_finished(&self, cx: &mut Context<Self>) -> AnyElement {
        let message = match self.today.first() {
            None => "Nothing planned for today. That is fine too.".to_string(),
            Some(first) if self.today.len() == 1 => format!("Your day is set: {}.", first.title),
            Some(first) => format!("Your day is set. Start with {}.", first.title),
        };
        v_flex()
            .gap_4()
            .py_2()
            .child(div().text_lg().child(message))
            .child(
                h_flex().child(
                    Button::new("plan-close")
                        .primary()
                        .label("Close")
                        .on_click(|_, window, cx| window.close_dialog(cx)),
                ),
            )
            .child(div().text_xs().text_color(style::subtle(cx)).child("Enter or Esc closes."))
            .into_any_element()
    }
}

impl Render for Planner {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body = match self.step {
            Step::Earlier | Step::Pick => match self.current().cloned() {
                Some(task) => self.render_task(&task, cx),
                None => self.render_finished(cx),
            },
            Step::Start => self.render_start(cx),
            Step::Finished => self.render_finished(cx),
        };
        div()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::on_key))
            .text_color(cx.theme().foreground)
            .child(body)
    }
}
