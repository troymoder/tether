use std::collections::HashMap;

use leptos::prelude::*;

use super::line::{Level, device_line, emulator_line};
use super::panes::{PickedLine, Whose};
use super::style;
use crate::ui::button::Button;

#[allow(unreachable_pub)]
#[component]
pub(super) fn LineDetail(
    line: PickedLine,
    names: StoredValue<HashMap<u32, String>, LocalStorage>,
    #[prop(into)] on_close: Callback<()>,
) -> impl IntoView {
    let parsed = match line.whose {
        Whose::Device => device_line(&line.raw),
        Whose::Emulator => emulator_line(&line.raw),
    };
    let process = parsed.pid.map(|pid| {
        let name = names.with_value(|names| names.get(&pid).cloned());

        match name {
            Some(name) => format!("{name} ({pid})"),
            None => format!("process {pid}"),
        }
    });
    let about = [
        parsed.time.as_deref(),
        parsed.level.map(Level::name),
        parsed.tag,
        process.as_deref(),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" · ");
    let message = parsed.message.to_owned();

    view! {
        <div class=style::detail>
            <header>
                <code>{about}</code>
                <Button quiet=true on:click=move |_| on_close.run(())>
                    "Close"
                </Button>
            </header>
            <pre>{message}</pre>
        </div>
    }
}
