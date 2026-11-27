use leptos::prelude::*;
use protocol::Webcam;

use super::browser::Browsing;
use super::fed::Cam;
use super::source::Source;
use super::source_settings::SourceSettings;
use super::style;
use crate::app::{Global, Icon, Picked, icons};
use crate::link::capture::{self, Capture, Remote, Wanted};
use crate::ui::file_pick::{FilePick, FilePicker};
use crate::ui::load::use_load;
use crate::ui::loadable::Loadable;
use crate::ui::popbox::PopBox;

fn webcams_note(webcams: &Loadable<Vec<Webcam>>) -> String {
    match webcams {
        Loadable::Loading => "looking".to_owned(),
        Loadable::Failed(_) => "failed".to_owned(),
        Loadable::Ready(found) if found.is_empty() => "none".to_owned(),
        Loadable::Ready(found) => format!("{} found", found.len()),
    }
}

#[allow(unreachable_pub)]
#[component]
pub(super) fn SourceBox(
    cam: Cam,
    shown: Memo<Source>,
    #[prop(into)] tetherd_feeds: Signal<bool>,
) -> impl IntoView {
    let link = expect_context::<Global>().link;
    let feed = expect_context::<Picked>().feed;
    // The source that was picked last. It is the open one while nothing
    // shows, since it may still wait for its file or for the browser's
    // permission.
    let chosen = RwSignal::new(None::<Source>);
    let open = RwSignal::new(false);
    let picker = FilePicker::new();
    let browsing = Browsing::new();
    let again = Trigger::new();
    let webcams = use_load(
        move || again.track(),
        move |()| async move { capture::webcams(link, cam.instance.get_value()).await },
    );
    let host = StoredValue::new(untrack(|| link.instance_name(&cam.instance.get_value())));
    let tab = Memo::new(move |_| match shown.get() {
        Source::Off => chosen.get().unwrap_or(Source::Off),
        shown => shown,
    });
    let capture_from = move |what: Capture| {
        let wanted = Wanted {
            lens: cam.lens,
            what,
        };

        feed.capture(wanted);
    };
    let pick = move |source: Source| {
        if tab.get_untracked() == source {
            return;
        }

        feed.stop();

        if tetherd_feeds.get_untracked() {
            feed.stop_remote();
        }

        chosen.set(Some(source));

        match source {
            // The browser asks which one, and for permission.
            Source::Camera => capture_from(Capture::Camera {
                id: None,
                size: capture::SIZES[1],
            }),
            Source::Screen => capture_from(Capture::Screen),
            Source::File => picker.open(),
            // A webcam of tetherd's is not turned on until one is chosen,
            // and a file of tetherd's is picked in the settings.
            Source::Off | Source::Webcam | Source::RemoteFile => {}
        }
    };
    let in_use = move |source: Source| -> Option<String> {
        if shown.get() != source {
            return None;
        }

        match source {
            Source::Off => None,
            Source::File => feed.played().map(|played| played.name),
            Source::Camera | Source::Screen => feed.label(),
            Source::Webcam => match feed.remoting()?.what {
                Remote::Webcam { path, .. } => Some(webcams.with(|webcams| {
                    match webcams {
                        Loadable::Ready(found) => found
                            .iter()
                            .find(|webcam| webcam.path == path)
                            .map_or_else(|| path.clone(), |webcam| webcam.name.clone()),
                        Loadable::Loading | Loadable::Failed(_) => path.clone(),
                    }
                })),
                Remote::File { .. } => None,
            },
            Source::RemoteFile => match feed.remoting()?.what {
                Remote::File { path } => path.rsplit('/').next().map(str::to_owned),
                Remote::Webcam { .. } => None,
            },
        }
    };
    let note = move |source: Source| {
        in_use(source).or_else(|| (source == Source::Webcam).then(|| webcams.with(webcams_note)))
    };
    let what = move || {
        let source = tab.get();

        match (source, in_use(source)) {
            (Source::Off, _) => "Off".to_owned(),
            (_, Some(named)) => named,
            (Source::Webcam, None) => "Choose a webcam".to_owned(),
            (Source::File | Source::RemoteFile, None) => "Choose a file".to_owned(),
            (Source::Camera, None) => "Webcam".to_owned(),
            (Source::Screen, None) => "Screen".to_owned(),
        }
    };
    let long = move || {
        let source = tab.get();
        let what = what();

        match source.kind() {
            None => what,
            Some(kind) if source.remote() => format!("{kind} {}: {what}", host.get_value()),
            Some(kind) => format!("{kind}: {what}"),
        }
    };
    let option = move |source: Source, name: &'static str| {
        view! {
            <button
                type="button"
                role="option"
                aria-selected=move || (tab.get() == source).to_string()
                on:click=move |_| pick(source)
            >
                <span class=style::dot aria-hidden="true"></span>
                <svg aria-hidden="true" viewBox="0 0 24 24">
                    <path d=source.icon() />
                </svg>
                <span>{name}</span>
                <span class=style::note>{move || note(source)}</span>
            </button>
        }
    };

    view! {
        <FilePick
            picker
            accept="image/*,video/*"
            label="An image or a video for the camera"
            on_file=move |file: web_sys::File| {
                if let Err(error) = feed.show(cam.lens, &file) {
                    link.did("camera", feed.device().as_ref(), Err(error));
                }
            }
        />
        <div class=style::bar>
            <button
                type="button"
                class=style::source
                aria-haspopup="dialog"
                aria-expanded=move || open.get().to_string()
                aria-label=move || format!("What the device's camera is fed: {}", long())
                on:click=move |_| open.update(|open| *open = !*open)
            >
                <span class=style::muted>"Source"</span>
                <svg aria-hidden="true" viewBox="0 0 24 24">
                    <path d=move || tab.get().icon() />
                </svg>
                <span class=style::what>{what}</span>
                <span class=style::muted>
                    {move || tab.get().remote().then(|| host.get_value())}
                </span>
                <Icon paths=icons::CHEVRON />
            </button>
            <PopBox open label="Source" class=style::sources>
                <div role="listbox" aria-label="Source" class=style::kinds>
                    {option(Source::Off, "Off")}
                    <span class=style::group>"This computer"</span>
                    {option(Source::File, "File")}
                    {option(Source::Camera, "Webcam")}
                    {option(Source::Screen, "Screen")}
                    <span class=style::group>{format!("tetherd {}", host.get_value())}</span>
                    {option(Source::Webcam, "Webcam")}
                    {option(Source::RemoteFile, "File")}
                </div>
                <div class=style::settings>
                    <SourceSettings
                        cam
                        tab
                        webcams
                        again
                        picker
                        browsing
                        open
                        in_use=Callback::new(in_use)
                        host
                    />
                    <Show when=move || tab.get().local()>
                        <span class=style::rule aria-hidden="true"></span>
                        <button
                            type="button"
                            role="checkbox"
                            class=style::lossless
                            aria-checked=move || feed.lossless().to_string()
                            title="Send every frame exactly, at more bandwidth"
                            on:click=move |_| feed.set_lossless(!untrack(|| feed.lossless()))
                        >
                            <span class=style::check aria-hidden="true">
                                <svg viewBox="0 0 24 24">
                                    <path d="M5 12l4 4 10-10" />
                                </svg>
                            </span>
                            <span>"Lossless"</span>
                            <span class=style::note>"every frame exact, more bandwidth"</span>
                        </button>
                    </Show>
                </div>
            </PopBox>
        </div>
    }
}
