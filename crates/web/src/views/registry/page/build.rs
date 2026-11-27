use leptos::prelude::*;
use protocol::Variant;
use stylance::classes;

use super::super::style;
use crate::app::{Global, icons};
use crate::link;
use crate::ui::action::use_action;
use crate::ui::button::{Button, IconButton};
use crate::ui::menu::MenuOf;
use crate::ui::tag::Tag;
use crate::util::size;
use crate::views::access::ago;

#[allow(unreachable_pub)]
#[component]
pub(super) fn BuildRow(
    app: String,
    variant: Variant,
    /// None for a build without native code, which runs on any ABI.
    abi: Option<String>,
    latest: Signal<bool>,
    menu: MenuOf<u32>,
) -> impl IntoView {
    let link = expect_context::<Global>().link;
    let build = variant.build;
    let file = variant.file.clone();
    let (downloading, download) = use_action(None, move |()| {
        let app = app.clone();
        let file = file.clone();

        async move {
            let apk = link
                .package(&app, build)
                .await
                .map_err(|error| format!("download failed: {error}"))?;

            link::save(&file, &apk);

            Ok(None::<String>)
        }
    });
    let version = variant.version.clone().unwrap_or_else(|| "—".to_owned());

    view! {
        <div class=style::build>
            <code class=style::number>{build}</code>
            <span class=style::names>
                <span class=style::version>
                    {version}
                    {move || {
                        latest.get().then(|| view! { <Tag class=style::latest>"Latest"</Tag> })
                    }}
                </span>
                <span class=classes!(style::muted, style::cut)>{ago(variant.handed_over)}</span>
            </span>
            <code>{abi.unwrap_or_else(|| "any".to_owned())}</code>
            <span class=style::muted>{size(variant.size)}</span>
            <span class=style::actions>
                <Button
                    small=true
                    attr:aria-haspopup="menu"
                    on:click=move |event| menu.under(&event, build)
                >
                    "Install on…"
                </Button>
                <IconButton
                    paths=icons::DOWNLOAD
                    label="Download"
                    small=true
                    class=style::small_icon
                    busy=downloading
                    on:click=move |_| download.run(())
                />
                <IconButton
                    paths=icons::BIN
                    label="Delete this build"
                    small=true
                    class=style::small_icon
                    on:click=move |_| link.unimplemented("delete build")
                />
            </span>
        </div>
    }
}
