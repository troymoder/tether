//! Only a rooted device, such as an emulator, lets the page set its language.

use android::tweaks::Tweak;
use leptos::prelude::*;

use super::{style, use_tweak};
use crate::app::Global;
use crate::ui;
use crate::ui::dropdown::{Choice, Dropdown};

const LOCALES: [(&str, &str); 22] = [
    ("en-US", "English (United States)"),
    ("en-GB", "English (United Kingdom)"),
    ("ar-EG", "Arabic (Egypt), right to left"),
    ("zh-CN", "Chinese (China)"),
    ("zh-TW", "Chinese (Taiwan)"),
    ("nl-NL", "Dutch"),
    ("fr-FR", "French"),
    ("de-DE", "German"),
    ("he-IL", "Hebrew, right to left"),
    ("hi-IN", "Hindi"),
    ("id-ID", "Indonesian"),
    ("it-IT", "Italian"),
    ("ja-JP", "Japanese"),
    ("ko-KR", "Korean"),
    ("pl-PL", "Polish"),
    ("pt-BR", "Portuguese (Brazil)"),
    ("ru-RU", "Russian"),
    ("es-ES", "Spanish (Spain)"),
    ("es-MX", "Spanish (Mexico)"),
    ("sv-SE", "Swedish"),
    ("tr-TR", "Turkish"),
    ("uk-UA", "Ukrainian"),
];

/// The device's own language is added first when it is not among [`LOCALES`],
/// so the dropdown can always show it.
fn locales(now: Option<&str>) -> Vec<Choice<String>> {
    let other = now
        .filter(|now| !LOCALES.iter().any(|(tag, _)| tag == now))
        .map(|now| Choice::new(now.to_owned(), now));

    other
        .into_iter()
        .chain(
            LOCALES
                .iter()
                .map(|(tag, name)| Choice::new((*tag).to_owned(), format!("{name} · {tag}"))),
        )
        .collect()
}

#[allow(unreachable_pub)]
#[component]
pub(super) fn Language(
    locale: Signal<Option<String>>,
    rooted: Signal<bool>,
    again: Trigger,
) -> impl IntoView {
    let ui = expect_context::<Global>().ui;
    let (busy, set) = use_tweak(again);
    let elsewhere = move || match locale.get() {
        Some(locale) => format!("{locale}. Set it in the device's own settings."),
        None => "Set it in the device's own settings.".to_owned(),
    };

    view! {
        <Show
            when=move || rooted.get()
            fallback=move || view! { <span class=ui::style::hint>{elsewhere}</span> }
        >
            <div class=style::whole>
                <Dropdown
                    class=style::choice
                    label="Language of the device"
                    disabled=busy
                    choices=Signal::derive(move || locale.with(|locale| locales(locale.as_deref())))
                    chosen=locale
                    on_choose=move |chosen: String| {
                        ui.confirm_then(
                            format!("Put the device in {chosen}?"),
                            "Android's interface starts again, which takes a few seconds and closes what is open.",
                            "Change",
                            async move {
                                // The view may have closed while the user
                                // was asked, so `try_run` is used.
                                set.try_run(Tweak::Locale(chosen));
                            },
                        );
                    }
                />
            </div>
        </Show>
    }
}
