use leptos::prelude::*;

use super::{Clip, Origin, style};
use crate::app::Picked;
use crate::ui;
use crate::ui::tag::{Dot, Tag};

#[allow(unreachable_pub)]
#[component]
pub(super) fn RecentClips(mine: RwSignal<String>) -> impl IntoView {
    let board = expect_context::<Picked>().board;
    let rows = move || {
        let now = board.text.get();

        board
            .clips
            .get()
            .into_iter()
            .map(|Clip { time, origin, text }| {
                let on = now.as_ref() == Some(&text);
                let again = text.clone();
                let (tone, from) = match origin {
                    Origin::Device => (Dot::On, "device"),
                    Origin::Computer => (Dot::Found, "computer"),
                };

                view! {
                    <button
                        type="button"
                        class=(style::on, on)
                        on:click=move |_| {
                            mine.set(again.clone());
                            board.put(origin, again.clone());
                        }
                    >
                        <code>{time}</code>
                        <Tag tone>{from}</Tag>
                        <Tag>"Text"</Tag>
                        <code>{text}</code>
                    </button>
                }
            })
            .collect_view()
    };

    view! {
        <div class=style::recent_head>
            <h2>"Recent"</h2>
            <span class=ui::style::hint>"Click to send again."</span>
        </div>
        <div class=style::recent>{rows}</div>
    }
}
