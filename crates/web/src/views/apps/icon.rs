use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use leptos::prelude::*;
use protocol::{App, Request};
use serde::Serialize;

use super::style;
use crate::app::Global;
use crate::cache;
use crate::ui::load::use_load;
use crate::ui::loadable::Loadable;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Icon {
    None,
    /// A CSS `background` value.
    Drawn(String),
    Failed(String),
}

/// Another build may carry another icon, so each count of builds has its
/// own cache entry.
#[derive(Clone, PartialEq, Serialize)]
struct RegistryIcon {
    id: String,
    builds: u32,
    file: String,
}

impl cache::Key for RegistryIcon {
    const KIND: &'static str = "registry-icon";
}

pub(crate) fn use_registry_icon(
    app: impl Fn() -> Option<App> + Send + Sync + 'static,
) -> Signal<Loadable<Icon>> {
    let link = expect_context::<Global>().link;

    use_load(
        move || {
            let app = app()?;

            Some(RegistryIcon {
                id: app.id,
                builds: app.builds,
                file: app.icon?,
            })
        },
        move |key: Option<RegistryIcon>| async move {
            let Some(key) = key else {
                return Ok(Icon::None);
            };

            let kept = cache::read(&key)
                .await
                .and_then(|svg| String::from_utf8(svg).ok());
            let svg = match kept {
                Some(svg) => svg,
                None => {
                    let svg = link.call(Request::AppIcon { id: key.id.clone() }).await?;

                    cache::write(&key, svg.as_bytes()).await;

                    svg
                }
            };

            Ok(Icon::Drawn(svg_background(&svg)))
        },
    )
}

fn svg_background(svg: &str) -> String {
    format!(
        "url(\"data:image/svg+xml,{}\") center / contain no-repeat",
        js_sys::encode_uri_component(svg)
    )
}

pub(super) fn png_background(png: &[u8]) -> String {
    format!(
        "url(\"data:image/png;base64,{}\") center / contain no-repeat",
        STANDARD.encode(png)
    )
}

fn tint(letter: &str) -> &'static str {
    match letter {
        "D" => "#2f6fd6",
        "L" => "#1f8a5a",
        "V" => "#c2410c",
        "C" => "#d97706",
        "S" => "#7a4fd6",
        "M" => "#0f8b8d",
        "P" => "#b4235a",
        "Y" => "#c0262d",
        _ => "#5b6472",
    }
}

#[allow(unreachable_pub)]
#[component]
pub(crate) fn AppIcon(
    #[prop(into)] id: String,
    #[prop(into)] drawn: Signal<Loadable<Icon>>,
    #[prop(optional, into)] running: Signal<bool>,
) -> impl IntoView {
    let initial: String = id
        .rsplit('.')
        .next()
        .and_then(|part| part.chars().next())
        .map(|letter| letter.to_uppercase().collect())
        .unwrap_or_default();
    let tint = tint(&initial);
    let picture = Memo::new(move |_| match drawn.get() {
        Loadable::Ready(Icon::Drawn(picture)) => Some(picture),
        _ => None,
    });

    view! {
        <span class=style::app_icon aria-hidden="true">
            <span style:background=move || {
                picture.get().unwrap_or_else(|| tint.to_owned())
            }>{move || picture.with(Option::is_none).then(|| initial.clone())}</span>
            {move || {
                running.get().then(|| view! { <span class=style::running title="Running"></span> })
            }}
        </span>
    }
}
