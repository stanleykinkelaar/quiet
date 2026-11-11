//! The Done view: finished tasks, grouped by day. Visible progress, with
//! no totals and nothing to live up to.

use chrono::{Datelike as _, NaiveDate};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{Sizable as _, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::{MainView, month_name, style};
use crate::data;

pub const DAYS_PER_PAGE: usize = 30;

pub fn day_label(day: NaiveDate, today: NaiveDate) -> String {
    match (today - day).num_days() {
        0 => "Today".into(),
        1 => "Yesterday".into(),
        _ => {
            let mut label = format!(
                "{} {} {}",
                quiet_store::weekday_name(day.weekday()),
                day.day(),
                month_name(day.month())
            );
            if day.year() != today.year() {
                label.push_str(&format!(" {}", day.year()));
            }
            label
        }
    }
}

impl MainView {
    pub(super) fn render_done(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let today = data::today();
        let subtle = style::subtle(cx);
        let mut children: Vec<AnyElement> = Vec::new();
        let mut index = 0;
        for (day, tasks) in &self.done_days {
            children.push(
                div()
                    .pt_5()
                    .pb_1()
                    .px_3()
                    .text_sm()
                    .text_color(subtle)
                    .child(day_label(*day, today))
                    .into_any_element(),
            );
            for task in tasks {
                children.push(self.render_row(task, index, cx));
                index += 1;
            }
        }
        let has_more = self.done_days.len() >= self.done_day_limit;

        v_flex()
            .id("done")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .gap_1()
            .children(children)
            .when(self.done_days.is_empty(), |this| {
                this.child(
                    div()
                        .py_6()
                        .px_3()
                        .text_color(subtle)
                        .child("Finished tasks show up here, day by day."),
                )
            })
            .when(has_more, |this| {
                this.child(
                    gpui_kit::component::h_flex().pt_4().px_1().child(
                        Button::new("older")
                            .ghost()
                            .small()
                            .label("Show earlier days")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.done_day_limit += DAYS_PER_PAGE;
                                this.reload(cx);
                            })),
                    ),
                )
            })
    }
}
