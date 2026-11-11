//! The main window: a narrow sidebar and one screen at a time.

mod done;
mod editor;
mod projects;
mod settings;
mod sort;
mod style;

pub use settings::saved_hotkey;

use std::collections::HashMap;

use chrono::Datelike as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{ActiveTheme as _, Icon, IconName, Sizable as _, StyledExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use quiet_store::{Project, ProjectId, Task, TaskId};

use crate::data;

actions!(
    quiet,
    [
        NewTask,
        EditTask,
        CompleteTask,
        ToggleToday,
        ToggleNextAction,
        DeleteTask,
        EditProject,
        SelectNext,
        SelectPrev,
        MoveUp,
        MoveDown,
        ShowToday,
        ShowInbox,
        ShowProjects,
        ShowDone,
        ShowSettings,
        GoBack,
        FocusList,
        KeepTask,
        SortInbox,
    ]
);

const CONTEXT: &str = "Quiet";
/// Today stays calm at five. More is allowed, with a gentle note.
const TODAY_SOFT_LIMIT: usize = 5;
/// Past this many unsorted Inbox tasks, Today offers one line to sort them.
const INBOX_NUDGE: usize = 10;

pub fn bind_keys(cx: &mut App) {
    // Single keys only act when no text field has focus.
    let list = Some("Quiet && !Input");
    cx.bind_keys([
        KeyBinding::new("n", NewTask, list),
        KeyBinding::new("enter", EditTask, list),
        KeyBinding::new("space", CompleteTask, list),
        KeyBinding::new("t", ToggleToday, list),
        KeyBinding::new("a", ToggleNextAction, list),
        KeyBinding::new("e", EditProject, list),
        KeyBinding::new("k", KeepTask, list),
        KeyBinding::new("s", SortInbox, list),
        KeyBinding::new("delete", DeleteTask, list),
        KeyBinding::new("down", SelectNext, list),
        KeyBinding::new("up", SelectPrev, list),
        KeyBinding::new("alt-up", MoveUp, list),
        KeyBinding::new("alt-down", MoveDown, list),
        KeyBinding::new("1", ShowToday, list),
        KeyBinding::new("2", ShowInbox, list),
        KeyBinding::new("3", ShowProjects, list),
        KeyBinding::new("4", ShowDone, list),
        KeyBinding::new(if cfg!(target_os = "macos") { "cmd-," } else { "ctrl-," }, ShowSettings, Some("Quiet")),
        KeyBinding::new("escape", GoBack, list),
        KeyBinding::new("backspace", GoBack, list),
        KeyBinding::new("escape", FocusList, Some("Quiet > Input")),
        KeyBinding::new("down", FocusList, Some("Quiet > Input")),
    ]);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Today,
    Inbox,
    Projects,
    Project(ProjectId),
    Done,
    Settings,
}

impl Screen {
    fn sidebar_slot(self) -> Screen {
        match self {
            Screen::Project(_) => Screen::Projects,
            other => other,
        }
    }
}

/// What a dragged Today row carries.
#[derive(Clone)]
struct DraggedTask {
    id: TaskId,
    title: SharedString,
}

struct DragPreview(SharedString);

impl Render for DragPreview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_4()
            .py_2()
            .rounded(cx.theme().radius)
            .bg(cx.theme().popover)
            .border_1()
            .border_color(cx.theme().border)
            .shadow_md()
            .text_color(cx.theme().foreground)
            .child(self.0.clone())
    }
}

pub struct MainView {
    screen: Screen,
    focus: FocusHandle,
    add_input: Entity<InputState>,
    /// Open tasks of the current screen, in display order.
    open: Vec<Task>,
    /// Today only: tasks completed today, shown quietly below.
    done_today: Vec<Task>,
    /// Done screen: completed tasks grouped by day, newest first.
    done_days: Vec<(chrono::NaiveDate, Vec<Task>)>,
    done_day_limit: usize,
    done_count: usize,
    selected: Option<TaskId>,
    selected_index: usize,
    /// Every project, archived ones included, for names and colours.
    project_map: HashMap<ProjectId, Project>,
    /// Projects screen: the list as shown.
    project_list: Vec<Project>,
    project_selected: usize,
    /// Projects screen: each project's next action, if it has one.
    project_next: HashMap<ProjectId, String>,
    show_archived: bool,
    select_project: Option<ProjectId>,
    /// Inbox tasks nobody has sorted yet, for the nudge on Today.
    inbox_unsorted: usize,
    hotkey_input: Entity<InputState>,
    settings_note: Option<SharedString>,
    autostart: bool,
    scroll: ScrollHandle,
    _subscriptions: Vec<Subscription>,
}

impl MainView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let add_input = cx.new(|cx| InputState::new(window, cx).placeholder("Add a task"));
        let hotkey = saved_hotkey(cx);
        let hotkey_input = cx.new(|cx| InputState::new(window, cx).default_value(hotkey));
        let subscriptions = vec![
            cx.subscribe_in(&add_input, window, |this, input, event, window, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    let text = input.read(cx).value().trim().to_string();
                    if !text.is_empty() {
                        this.submit(&text, cx);
                        input.update(cx, |input, cx| input.set_value("", window, cx));
                    }
                }
            }),
            cx.observe_global::<data::Data>(|this, cx| this.reload(cx)),
            cx.observe_global::<crate::update::Updater>(|_, cx| cx.notify()),
            cx.subscribe_in(&hotkey_input, window, |this, _, event, _, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    this.apply_hotkey(cx);
                }
            }),
        ];
        let mut view = Self {
            screen: Screen::Today,
            focus: cx.focus_handle(),
            add_input,
            open: Vec::new(),
            done_today: Vec::new(),
            done_days: Vec::new(),
            done_day_limit: done::DAYS_PER_PAGE,
            done_count: 0,
            selected: None,
            selected_index: 0,
            project_map: HashMap::new(),
            project_list: Vec::new(),
            project_selected: 0,
            project_next: HashMap::new(),
            show_archived: false,
            select_project: None,
            inbox_unsorted: 0,
            hotkey_input,
            settings_note: None,
            autostart: crate::platform::autostart::is_enabled(),
            scroll: ScrollHandle::new(),
            _subscriptions: subscriptions,
        };
        view.reload(cx);
        window.focus(&view.focus, cx);
        view
    }

    pub fn show(&mut self, screen: Screen, window: &mut Window, cx: &mut Context<Self>) {
        if self.screen != screen {
            let from_project = match self.screen {
                Screen::Project(id) => Some(id),
                _ => None,
            };
            self.screen = screen;
            self.selected = None;
            self.selected_index = 0;
            self.reload(cx);
            // Coming back from a project keeps that project selected.
            if let (Screen::Projects, Some(id)) = (screen, from_project) {
                self.project_selected =
                    self.project_list.iter().position(|p| p.id == id).unwrap_or(0);
            }
            let placeholder: SharedString = match screen {
                Screen::Today | Screen::Inbox => "Add a task".into(),
                Screen::Projects => "New project".into(),
                Screen::Project(id) => format!("Add to {}", self.project_name(id)).into(),
                Screen::Settings | Screen::Done => "".into(),
            };
            self.add_input.update(cx, |input, cx| input.set_placeholder(placeholder, window, cx));
            self.scroll.scroll_to_item(0);
        }
        window.focus(&self.focus, cx);
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        let today = data::today();
        self.project_map =
            data::read(cx, |s| s.projects(true)).into_iter().map(|p| (p.id, p)).collect();
        self.open = match self.screen {
            Screen::Today => data::read(cx, |s| s.today()),
            Screen::Inbox => data::read(cx, |s| s.inbox()),
            Screen::Project(id) => data::read(cx, |s| s.project_tasks(id)),
            Screen::Done => {
                let limit = self.done_day_limit;
                self.done_days = data::read(cx, |s| s.done_by_day(limit));
                // The rows, flattened, so keyboard selection works as usual.
                self.done_days.iter().flat_map(|(_, tasks)| tasks.iter().cloned()).collect()
            }
            Screen::Projects | Screen::Settings => Vec::new(),
        };
        self.inbox_unsorted = data::read(cx, |s| s.inbox_unsorted_count());
        self.done_today = data::read(cx, |s| s.done_on(today));
        self.done_count = self.done_today.len();
        if self.screen != Screen::Today {
            self.done_today.clear();
        }
        if self.screen == Screen::Projects {
            let show_archived = self.show_archived;
            self.project_list = data::read(cx, |s| s.projects(show_archived));
            self.project_next = self
                .project_list
                .iter()
                .filter_map(|p| {
                    let next = data::read(cx, |s| s.project_tasks(p.id)).into_iter().find(|t| t.is_next_action)?;
                    Some((p.id, next.title))
                })
                .collect();
            if let Some(id) = self.select_project.take() {
                self.project_selected = self.project_list.iter().position(|p| p.id == id).unwrap_or(0);
            }
            self.project_selected =
                self.project_selected.min(self.project_list.len().saturating_sub(1));
        }
        if let Screen::Project(id) = self.screen
            && !self.project_map.contains_key(&id)
        {
            self.screen = Screen::Projects;
            return self.reload(cx);
        }
        // Keep the selection on the same task, or on the same spot if the
        // task left this list.
        let ids = self.visible_ids();
        match self.selected.and_then(|id| ids.iter().position(|&t| t == id)) {
            Some(index) => self.selected_index = index,
            None if self.selected.is_some() => {
                self.selected_index = self.selected_index.min(ids.len().saturating_sub(1));
                self.selected = ids.get(self.selected_index).copied();
            }
            None => {}
        }
        cx.notify();
    }

    fn project_name(&self, id: ProjectId) -> SharedString {
        self.project_map.get(&id).map(|p| p.name.clone().into()).unwrap_or_default()
    }

    fn visible_ids(&self) -> Vec<TaskId> {
        self.open.iter().chain(&self.done_today).map(|t| t.id).collect()
    }

    fn selected_task(&self) -> Option<&Task> {
        let id = self.selected?;
        self.open.iter().chain(&self.done_today).find(|t| t.id == id)
    }

    fn select_index(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.screen == Screen::Projects {
            if index < self.project_list.len() {
                self.project_selected = index;
                self.scroll.scroll_to_item(index);
                cx.notify();
            }
            return;
        }
        let ids = self.visible_ids();
        if let Some(&id) = ids.get(index) {
            self.selected = Some(id);
            self.selected_index = index;
            self.scroll.scroll_to_item(index);
            cx.notify();
        }
    }

    /// Enter in the add field.
    fn submit(&mut self, text: &str, cx: &mut Context<Self>) {
        match self.screen {
            Screen::Projects => {
                let color = style::next_project_color(self.project_map.len());
                // Select the new project once the list reloads.
                self.select_project = data::change(cx, |s| s.add_project(text, color));
            }
            screen => {
                let in_today = screen == Screen::Today;
                let project = match screen {
                    Screen::Project(id) => Some(id),
                    _ => None,
                };
                if let Some(id) = data::change(cx, |s| s.add_task(text, project, in_today)) {
                    self.selected = Some(id);
                }
            }
        }
    }

    // ---- actions --------------------------------------------------------

    fn on_new_task(&mut self, _: &NewTask, window: &mut Window, cx: &mut Context<Self>) {
        self.add_input.update(cx, |input, cx| input.focus(window, cx));
    }

    fn on_focus_list(&mut self, _: &FocusList, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus, cx);
        if self.selected.is_none() {
            self.select_index(0, cx);
        }
    }

    fn on_select_next(&mut self, _: &SelectNext, _: &mut Window, cx: &mut Context<Self>) {
        let (current, len, has_selection) = if self.screen == Screen::Projects {
            (self.project_selected, self.project_list.len(), true)
        } else {
            (self.selected_index, self.visible_ids().len(), self.selected.is_some())
        };
        let next = if has_selection { current + 1 } else { 0 };
        self.select_index(next.min(len.saturating_sub(1)), cx);
    }

    fn on_select_prev(&mut self, _: &SelectPrev, _: &mut Window, cx: &mut Context<Self>) {
        let current =
            if self.screen == Screen::Projects { self.project_selected } else { self.selected_index };
        self.select_index(current.saturating_sub(1), cx);
    }

    fn on_complete(&mut self, _: &CompleteTask, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(task) = self.selected_task().cloned() {
            toggle_done(&task, cx);
        }
    }

    fn on_toggle_today(&mut self, _: &ToggleToday, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(task) = self.selected_task().cloned()
            && !task.is_done()
        {
            data::change(cx, |s| s.set_in_today(task.id, !task.in_today));
        }
    }

    fn on_toggle_next(&mut self, _: &ToggleNextAction, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(task) = self.selected_task().cloned()
            && task.project_id.is_some()
            && !task.is_done()
        {
            data::change(cx, |s| s.set_next_action(task.id, !task.is_next_action));
        }
    }

    fn on_keep(&mut self, _: &KeepTask, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(task) = self.selected_task().cloned()
            && task.carried
        {
            data::change(cx, |s| s.keep_carried(task.id));
        }
    }

    fn on_sort_inbox(&mut self, _: &SortInbox, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.screen, Screen::Inbox | Screen::Today) {
            sort::open(window, cx);
        }
    }

    fn on_delete(&mut self, _: &DeleteTask, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.selected {
            delete_task(id, window, cx);
        }
    }

    fn on_edit(&mut self, _: &EditTask, window: &mut Window, cx: &mut Context<Self>) {
        if self.screen == Screen::Projects {
            if let Some(project) = self.project_list.get(self.project_selected) {
                self.show(Screen::Project(project.id), window, cx);
            }
        } else if let Some(task) = self.selected_task().cloned() {
            editor::open(&task, window, cx);
        }
    }

    fn on_edit_project(&mut self, _: &EditProject, window: &mut Window, cx: &mut Context<Self>) {
        let project = match self.screen {
            Screen::Projects => self.project_list.get(self.project_selected).cloned(),
            Screen::Project(id) => self.project_map.get(&id).cloned(),
            _ => None,
        };
        if let Some(project) = project {
            projects::open_editor(&project, window, cx);
        }
    }

    fn on_back(&mut self, _: &GoBack, window: &mut Window, cx: &mut Context<Self>) {
        if let Screen::Project(_) = self.screen {
            self.show(Screen::Projects, window, cx);
        }
    }

    fn on_move_up(&mut self, _: &MoveUp, _: &mut Window, cx: &mut Context<Self>) {
        self.move_selected(-1, cx);
    }

    fn on_move_down(&mut self, _: &MoveDown, _: &mut Window, cx: &mut Context<Self>) {
        self.move_selected(1, cx);
    }

    fn move_selected(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.screen != Screen::Today {
            return;
        }
        let Some(id) = self.selected else { return };
        let Some(index) = self.open.iter().position(|t| t.id == id) else { return };
        let target = index.saturating_add_signed(delta).min(self.open.len().saturating_sub(1));
        if target != index {
            data::change(cx, |s| s.move_in_today(id, target));
            self.scroll.scroll_to_item(target);
        }
    }

    // ---- rendering ------------------------------------------------------

    fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let current = self.screen.sidebar_slot();
        let item = |id: &'static str, icon: IconName, label: &'static str, screen: Screen, cx: &mut Context<Self>| {
            let active = current == screen;
            h_flex()
                .id(id)
                .gap_3()
                .px_3()
                .py_2()
                .rounded(cx.theme().radius)
                .cursor_pointer()
                .text_color(if active { cx.theme().foreground } else { style::subtle(cx) })
                .when(active, |this| this.bg(cx.theme().sidebar_accent))
                .hover(|this| this.bg(cx.theme().sidebar_accent))
                .child(Icon::new(icon).small())
                .child(label)
                .on_click(cx.listener(move |this, _, window, cx| this.show(screen, window, cx)))
        };
        v_flex()
            .w(px(168.))
            .h_full()
            .flex_none()
            .gap_1()
            .px_3()
            .pt_8()
            .bg(cx.theme().sidebar)
            .border_r_1()
            .border_color(cx.theme().sidebar_border)
            .child(item("nav-today", IconName::Sun, "Today", Screen::Today, cx))
            .child(item("nav-inbox", IconName::Inbox, "Inbox", Screen::Inbox, cx))
            .child(item("nav-projects", IconName::Folder, "Projects", Screen::Projects, cx))
            .child(item("nav-done", IconName::CircleCheck, "Done", Screen::Done, cx))
            .child(div().flex_1())
            .child(div().pb_4().child(item("nav-settings", IconName::Settings, "Settings", Screen::Settings, cx)))
    }

    fn render_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let subtle = style::subtle(cx);
        let heading = |title: SharedString| div().text_2xl().font_semibold().child(title);
        let caption = move |text: SharedString| div().text_sm().text_color(subtle).child(text);

        let left = match self.screen {
            Screen::Today => {
                let d = data::today();
                v_flex().gap_1().child(heading("Today".into())).child(caption(
                    format!("{} {} {}", quiet_store::weekday_name(d.weekday()), d.day(), month_name(d.month()))
                        .into(),
                ))
            }
            Screen::Inbox => v_flex()
                .gap_1()
                .child(heading("Inbox".into()))
                .child(caption("Everything you captured. Press T to pick it for today.".into())),
            Screen::Projects => v_flex()
                .gap_1()
                .child(heading("Projects".into()))
                .child(caption("Each one keeps its own list.".into())),
            Screen::Settings => v_flex().gap_1().child(heading("Settings".into())),
            Screen::Done => v_flex()
                .gap_1()
                .child(heading("Done".into()))
                .child(caption("What you finished, day by day.".into())),
            Screen::Project(id) => {
                let color = self.project_map.get(&id).map(|p| style::hex(&p.color)).unwrap_or(subtle);
                v_flex()
                    .gap_1()
                    .child(
                        h_flex()
                            .id("back")
                            .gap_1()
                            .text_sm()
                            .text_color(subtle)
                            .cursor_pointer()
                            .hover(|s| s.text_color(cx.theme().foreground))
                            .child(Icon::new(IconName::ChevronLeft).small())
                            .child("Projects")
                            .on_click(cx.listener(|this, _, window, cx| this.show(Screen::Projects, window, cx))),
                    )
                    .child(
                        h_flex()
                            .gap_3()
                            .child(div().size(px(12.)).rounded_full().bg(color))
                            .child(heading(self.project_name(id))),
                    )
            }
        };

        let right = match self.screen {
            Screen::Today if self.done_count > 0 => Some(
                h_flex()
                    .gap_2()
                    .text_sm()
                    .text_color(subtle)
                    .child(Icon::new(IconName::CircleCheck).small())
                    .child(format!("{} done today", self.done_count))
                    .into_any_element(),
            ),
            Screen::Inbox if !self.open.iter().all(|t| t.in_today) => Some(
                Button::new("sort-inbox")
                    .ghost()
                    .small()
                    .icon(IconName::Inbox)
                    .label("Sort")
                    .tooltip("Decide on each task, one at a time (S)")
                    .on_click(cx.listener(|_, _, window, cx| sort::open(window, cx)))
                    .into_any_element(),
            ),
            Screen::Project(id) => Some(
                Button::new("edit-project")
                    .ghost()
                    .small()
                    .icon(IconName::Settings)
                    .label("Edit")
                    .tooltip("Name, colour, archive (E)")
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if let Some(project) = this.project_map.get(&id).cloned() {
                            projects::open_editor(&project, window, cx);
                        }
                    }))
                    .into_any_element(),
            ),
            _ => None,
        };

        h_flex().items_end().justify_between().child(left).children(right)
    }

    pub(super) fn render_row(&self, task: &Task, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let selected = self.selected == Some(task.id);
        let done = task.is_done();
        let id = task.id;
        let theme = cx.theme();
        let check_color = if done { theme.muted_foreground } else { theme.muted_foreground.opacity(0.5) };
        let reorderable = self.screen == Screen::Today && !done;
        let in_project = matches!(self.screen, Screen::Project(_));

        let check = div()
            .id(("check", id as u64))
            .flex_none()
            .size(px(20.))
            .rounded_full()
            .border_2()
            .border_color(check_color)
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .hover(|this| this.border_color(cx.theme().foreground))
            .when(done, |this| {
                this.bg(cx.theme().muted_foreground)
                    .child(Icon::new(IconName::Check).xsmall().text_color(cx.theme().background))
            })
            .on_click({
                let task = task.clone();
                cx.listener(move |_, _, _, cx| {
                    cx.stop_propagation();
                    toggle_done(&task, cx);
                })
            });

        // Quiet hints that a task repeats, has a day, a reminder or a note.
        let has_details = task.repeat != quiet_store::Repeat::Never
            || task.reminder.is_some()
            || task.note.is_some()
            || task.day.is_some();
        let details = (!done && has_details).then(|| {
            let today = data::today();
            h_flex()
                .flex_none()
                .gap_2()
                .text_xs()
                .text_color(style::subtle(cx))
                .when(task.note.is_some(), |this| this.child(Icon::new(IconName::FileText).xsmall()))
                .when(task.repeat != quiet_store::Repeat::Never, |this| {
                    this.child(Icon::new(IconName::RefreshCw).xsmall())
                })
                .children(task.day.filter(|d| *d != today || self.screen != Screen::Today).map(|d| {
                    h_flex()
                        .gap_1()
                        .child(Icon::new(IconName::Calendar).xsmall())
                        .child(day_chip(d, today))
                }))
                .children(task.reminder.map(|time| {
                    // A reminder that rang (or was missed) and got no reaction
                    // stays visible until you click it or finish the task.
                    let open = task.reminder_open;
                    h_flex()
                        .id(("bell", id as u64))
                        .gap_1()
                        .when(open, |this| {
                            this.px_1p5()
                                .rounded_full()
                                .bg(cx.theme().secondary)
                                .text_color(cx.theme().foreground)
                                .cursor_pointer()
                                .tooltip(|window, cx| {
                                    gpui_kit::component::tooltip::Tooltip::new("Reminder rang. Click to clear").build(window, cx)
                                })
                                .on_click(cx.listener(move |_, _, _, cx| {
                                    cx.stop_propagation();
                                    data::change(cx, |s| s.acknowledge_reminder(id));
                                }))
                        })
                        .child(Icon::new(IconName::Bell).xsmall())
                        .child(time.format("%H:%M").to_string())
                }))
        });

        // A small coloured tag naming the project, on Today only.
        let tag = matches!(self.screen, Screen::Today | Screen::Done)
            .then(|| task.project_id.and_then(|p| self.project_map.get(&p)))
            .flatten()
            .map(|project| {
                let color = style::hex(&project.color);
                h_flex()
                    .flex_none()
                    .gap_1p5()
                    .px_2()
                    .py_0p5()
                    .rounded_full()
                    .bg(color.opacity(0.14))
                    .text_xs()
                    .text_color(cx.theme().foreground.opacity(0.8))
                    .child(div().size(px(7.)).rounded_full().bg(color))
                    .child(project.name.clone())
            });

        let next_marker = (in_project && task.is_next_action).then(|| {
            div()
                .flex_none()
                .text_xs()
                .px_2()
                .py_0p5()
                .rounded_full()
                .bg(cx.theme().secondary)
                .text_color(cx.theme().secondary_foreground)
                .child("Next")
        });

        let today_toggle = (!done).then(|| {
            let on_today_screen = self.screen == Screen::Today;
            let tooltip = match (on_today_screen, task.in_today) {
                (true, _) => "Take out of Today (T)",
                (false, true) => "In Today. Click or T to take out",
                (false, false) => "Add to Today (T)",
            };
            Button::new(("today", id as u64))
                .ghost()
                .small()
                .icon(Icon::new(IconName::Sun).when(task.in_today && !on_today_screen, |i| {
                    i.text_color(cx.theme().foreground)
                }))
                .tooltip(tooltip)
                .when(!task.in_today || on_today_screen, |b| b.opacity(0.).group_hover("row", |s| s.opacity(1.)))
                .when(task.in_today && !on_today_screen, |b| b.opacity(0.9))
                .on_click({
                    let in_today = task.in_today;
                    cx.listener(move |_, _, _, cx| {
                        cx.stop_propagation();
                        data::change(cx, |s| s.set_in_today(id, !in_today));
                    })
                })
        });

        let keep = (self.screen == Screen::Today && task.carried && !done).then(|| {
            Button::new(("keep", id as u64))
                .ghost()
                .small()
                .label("Keep")
                .tooltip("Keep it in Today (K)")
                .on_click(cx.listener(move |_, _, _, cx| {
                    cx.stop_propagation();
                    data::change(cx, |s| s.keep_carried(id));
                }))
        });

        let next_toggle = (in_project && !done).then(|| {
            Button::new(("next", id as u64))
                .ghost()
                .small()
                .icon(if task.is_next_action { IconName::StarFill } else { IconName::Star })
                .tooltip(if task.is_next_action { "Next action. Click or A to clear" } else { "Make this the next action (A)" })
                .when(!task.is_next_action, |b| b.opacity(0.).group_hover("row", |s| s.opacity(1.)))
                .on_click({
                    let next = task.is_next_action;
                    cx.listener(move |_, _, _, cx| {
                        cx.stop_propagation();
                        data::change(cx, |s| s.set_next_action(id, !next));
                    })
                })
        });

        let delete = Button::new(("delete", id as u64))
            .ghost()
            .small()
            .icon(IconName::Delete)
            .tooltip("Delete")
            .opacity(0.)
            .group_hover("row", |s| s.opacity(1.))
            .on_click(cx.listener(move |_, _, window, cx| {
                cx.stop_propagation();
                delete_task(id, window, cx);
            }));

        h_flex()
            .id(("task", id as u64))
            .group("row")
            .gap_3()
            .px_3()
            .py(px(10.))
            .rounded(cx.theme().radius)
            .text_base()
            .when(selected, |this| this.bg(style::selection(cx)))
            .when(!selected, |this| this.hover(|s| s.bg(cx.theme().list_hover)))
            .child(check)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .when(done && self.screen != Screen::Done, |this| {
                        this.line_through().text_color(style::subtle(cx))
                    })
                    .child(task.title.clone()),
            )
            .children(details)
            .children(next_marker)
            .children(tag)
            .children(keep)
            .children(next_toggle)
            .children(today_toggle)
            .child(delete)
            .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                this.selected = Some(id);
                this.selected_index = index;
                window.focus(&this.focus, cx);
                cx.notify();
                if event.click_count() == 2
                    && let Some(task) = this.selected_task().cloned()
                {
                    editor::open(&task, window, cx);
                }
            }))
            .when(reorderable, |this| {
                this.on_drag(
                    DraggedTask { id, title: task.title.clone().into() },
                    |dragged, _, _, cx| cx.new(|_| DragPreview(dragged.title.clone())),
                )
                .drag_over::<DraggedTask>(|style, _, _, cx| {
                    style.border_t_2().border_color(cx.theme().drag_border)
                })
                .on_drop(cx.listener(move |_, dragged: &DraggedTask, _, cx| {
                    data::change(cx, |s| s.move_in_today(dragged.id, index));
                }))
            })
            .into_any_element()
    }

    fn render_list(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut rows: Vec<AnyElement> = Vec::new();
        let on_today = self.screen == Screen::Today;
        for (index, task) in self.open.iter().enumerate() {
            let first_carried = on_today && task.carried && !self.open[..index].iter().any(|t| t.carried);
            if first_carried {
                rows.push(self.render_carried_header(cx));
            }
            rows.push(self.render_row(task, index, cx));
        }
        let empty = self.open.is_empty();
        let over_limit = self.screen == Screen::Today && self.open.len() > TODAY_SOFT_LIMIT;

        v_flex()
            .id("list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .gap_1()
            .children(rows)
            .when(empty, |this| {
                let text = match self.screen {
                    Screen::Today => "Nothing picked for today yet. Press N to add something, or pick from the Inbox or a project with T.",
                    Screen::Inbox => "Inbox is empty. Capture anything with Ctrl+Alt+Space.",
                    _ => "No open tasks here. Press N to add one.",
                };
                this.child(div().py_6().px_3().text_color(style::subtle(cx)).child(text))
            })
            .when(over_limit, |this| {
                this.child(
                    div()
                        .px_3()
                        .pt_3()
                        .text_sm()
                        .text_color(style::subtle(cx))
                        .child("That is more than five. It is fine, and you can always move some back with T."),
                )
            })
            .when(on_today && self.inbox_unsorted >= INBOX_NUDGE, |this| {
                this.child(
                    div()
                        .id("inbox-nudge")
                        .px_3()
                        .pt_3()
                        .text_sm()
                        .text_color(style::subtle(cx))
                        .cursor_pointer()
                        .hover(|s| s.text_color(cx.theme().foreground))
                        .child(format!("{} things in your Inbox. Sort them? (S)", self.inbox_unsorted))
                        .on_click(cx.listener(|_, _, window, cx| sort::open(window, cx))),
                )
            })
            .when(!self.done_today.is_empty(), |this| {
                let offset = self.open.len();
                let done_rows: Vec<AnyElement> = self
                    .done_today
                    .iter()
                    .enumerate()
                    .map(|(i, task)| self.render_row(task, offset + i, cx))
                    .collect();
                this.child(div().pt_6().pb_1().px_3().text_sm().text_color(style::subtle(cx)).child("Done today"))
                    .children(done_rows)
            })
    }
}

impl MainView {
    /// Above the tasks that came along from earlier days: what they are,
    /// and two ways to decide about all of them at once.
    fn render_carried_header(&self, cx: &mut Context<Self>) -> AnyElement {
        h_flex()
            .pt_6()
            .pb_1()
            .px_3()
            .justify_between()
            .child(
                div()
                    .text_sm()
                    .text_color(style::subtle(cx))
                    .child("From earlier. Keep them for today, or send them back with T."),
            )
            .child(
                h_flex()
                    .gap_1()
                    .child(
                        Button::new("keep-all")
                            .ghost()
                            .xsmall()
                            .label("Keep all")
                            .on_click(cx.listener(|_, _, _, cx| {
                                data::change(cx, |s| s.settle_carried(true));
                            })),
                    )
                    .child(
                        Button::new("send-back-all")
                            .ghost()
                            .xsmall()
                            .label("Send all back")
                            .on_click(cx.listener(|_, _, _, cx| {
                                data::change(cx, |s| s.settle_carried(false));
                            })),
                    ),
            )
            .into_any_element()
    }
}

impl Render for MainView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body = match self.screen {
            Screen::Projects => self.render_projects(cx).into_any_element(),
            Screen::Settings => self.render_settings(cx).into_any_element(),
            Screen::Done => self.render_done(cx).into_any_element(),
            _ => self.render_list(cx).into_any_element(),
        };
        let show_add = !matches!(self.screen, Screen::Settings | Screen::Done);
        h_flex()
            .size_full()
            .key_context(CONTEXT)
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::on_new_task))
            .on_action(cx.listener(Self::on_focus_list))
            .on_action(cx.listener(Self::on_select_next))
            .on_action(cx.listener(Self::on_select_prev))
            .on_action(cx.listener(Self::on_complete))
            .on_action(cx.listener(Self::on_toggle_today))
            .on_action(cx.listener(Self::on_toggle_next))
            .on_action(cx.listener(Self::on_delete))
            .on_action(cx.listener(Self::on_edit))
            .on_action(cx.listener(Self::on_edit_project))
            .on_action(cx.listener(Self::on_back))
            .on_action(cx.listener(Self::on_move_up))
            .on_action(cx.listener(Self::on_move_down))
            .on_action(cx.listener(Self::on_keep))
            .on_action(cx.listener(Self::on_sort_inbox))
            .on_action(cx.listener(|this, _: &ShowToday, window, cx| this.show(Screen::Today, window, cx)))
            .on_action(cx.listener(|this, _: &ShowInbox, window, cx| this.show(Screen::Inbox, window, cx)))
            .on_action(cx.listener(|this, _: &ShowProjects, window, cx| this.show(Screen::Projects, window, cx)))
            .on_action(cx.listener(|this, _: &ShowSettings, window, cx| this.show(Screen::Settings, window, cx)))
            .on_action(cx.listener(|this, _: &ShowDone, window, cx| this.show(Screen::Done, window, cx)))
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(self.render_sidebar(cx))
            .child(
                h_flex().flex_1().h_full().justify_center().child(
                    v_flex()
                        .w_full()
                        .max_w(px(680.))
                        .h_full()
                        .px_8()
                        .pt_8()
                        .pb_4()
                        .gap_5()
                        .child(self.render_header(cx))
                        .when(show_add, |this| this.child(Input::new(&self.add_input).large()))
                        .child(body)
                        .child(self.render_hints(cx)),
                ),
            )
    }
}

impl MainView {
    /// One muted line with the keys that matter on this screen.
    fn render_hints(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let hints: &[(&str, &str)] = match self.screen {
            Screen::Today if self.open.iter().any(|t| t.carried) => {
                &[("N", "new"), ("Space", "done"), ("K", "keep"), ("T", "take out"), ("Alt ↑↓", "move")]
            }
            Screen::Today => &[("N", "new"), ("Enter", "edit"), ("Space", "done"), ("T", "take out"), ("Alt ↑↓", "move")],
            Screen::Inbox => &[("N", "new"), ("Enter", "edit"), ("Space", "done"), ("T", "today"), ("S", "sort")],
            Screen::Projects => &[("N", "new project"), ("Enter", "open"), ("E", "edit")],
            Screen::Project(_) => &[("N", "new"), ("Space", "done"), ("T", "today"), ("A", "next action"), ("Esc", "back")],
            Screen::Done => &[("Space", "not done after all")],
            Screen::Settings => &[],
        };
        let subtle = style::subtle(cx).opacity(0.8);
        h_flex()
            .flex_none()
            .gap_4()
            .pt_2()
            .text_xs()
            .text_color(subtle)
            .children(hints.iter().map(|(key, label)| {
                h_flex()
                    .gap_1()
                    .child(div().font_semibold().child(*key))
                    .child(*label)
            }))
    }
}

fn toggle_done(task: &Task, cx: &mut App) {
    let id = task.id;
    if task.is_done() {
        data::change(cx, |s| s.reopen(id));
    } else {
        data::change(cx, |s| s.complete(id, quiet_store::now()));
    }
}

fn delete_task(id: TaskId, window: &mut Window, cx: &mut App) {
    use gpui_kit::component::WindowExt as _;
    use gpui_kit::component::notification::Notification;
    let Some(Some(task)) = data::change(cx, |s| s.delete_task(id)) else { return };
    let title: SharedString = format!("Deleted “{}”", task.title).into();
    window.push_notification(
        Notification::new()
            .message(title)
            .action(move |_, _, cx| {
                let task = task.clone();
                Button::new("undo").label("Undo").small().on_click(cx.listener(move |note, _, window, cx| {
                    data::change(cx, |s| s.restore_task(&task));
                    note.dismiss(window, cx);
                }))
            })
            // An action normally keeps the note up; this one should fade away.
            .autohide(true),
        cx,
    );
}

/// A task's day, said the way people say it: Today, Tomorrow, the weekday
/// within a week, otherwise "14 Nov". Past days read the same, never red.
fn day_chip(day: chrono::NaiveDate, today: chrono::NaiveDate) -> String {
    match (day - today).num_days() {
        0 => "Today".into(),
        1 => "Tomorrow".into(),
        -1 => "Yesterday".into(),
        2..=6 => quiet_store::weekday_name(day.weekday()).into(),
        _ => {
            let month = &month_name(day.month())[..3];
            if day.year() == today.year() {
                format!("{} {month}", day.day())
            } else {
                format!("{} {month} {}", day.day(), day.year())
            }
        }
    }
}

fn month_name(month: u32) -> &'static str {
    [
        "January", "February", "March", "April", "May", "June", "July", "August", "September",
        "October", "November", "December",
    ]
    .get(month.saturating_sub(1) as usize)
    .copied()
    .unwrap_or("")
}
