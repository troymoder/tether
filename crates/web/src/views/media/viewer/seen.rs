use leptos::prelude::*;
use leptos::task::spawn_local;
use stylance::classes;

use super::super::stream::{self, Stream};
use super::super::tile::is_video;
use super::super::{Address, Item, style, thumbnails};
use super::progress::Progress;
use crate::app::Picked;
use crate::cache;
use crate::ui;

enum Shown {
    Fetching,
    Streaming(Stream),
    Whole(Address),
    Failed(String),
}

#[allow(unreachable_pub)]
#[component]
pub(super) fn Seen(item: Item, download: Signal<Option<u64>>) -> impl IntoView {
    let device = expect_context::<Picked>().device.get_untracked();
    let video = is_video(item.name());
    let kind = stream::kind(item.name());
    let total = item.size;
    let shown = RwSignal::new_local(Shown::Fetching);
    let early = RwSignal::new(None::<Address>);
    let served = RwSignal::new(0u64);

    // The viewer may close while this runs. `try_set` then hands the value
    // back and it is dropped, which closes the stream and revokes the
    // address.
    spawn_local(async move {
        if !video && let Some(kept) = cache::read(&thumbnails::key(&item)).await {
            early.try_set(Address::of(&kept));
        }

        let started = stream::start(&*device, item.path.clone(), item.size, kind, served).await;
        let fetched = match started {
            Ok(Some(stream)) => Shown::Streaming(stream),
            Ok(None) => match device.read(&item.path).await.map(|file| Address::of(&file)) {
                Ok(Some(address)) => Shown::Whole(address),
                Ok(None) => Shown::Failed("show failed: no address".to_owned()),
                Err(error) => Shown::Failed(error),
            },
            Err(error) => Shown::Failed(error),
        };

        shown.try_set(fetched);
    });

    let failed = Callback::new(move |()| shown.set(Shown::Failed("stream failed".to_owned())));

    move || {
        shown.with(|shown| {
            let address = match shown {
                Shown::Fetching => {
                    return view! {
                        {move || waiting(early)}
                        <Bars download total />
                    }
                    .into_any();
                }
                Shown::Failed(why) => {
                    return view! {
                        <p class=classes!(ui::style::hint, style::error)>{why.clone()}</p>
                        <Bars download total />
                    }
                    .into_any();
                }
                Shown::Streaming(stream) => stream.address.clone(),
                Shown::Whole(address) => address.as_str().to_owned(),
            };

            if video {
                view! {
                    <video src=address controls autoplay></video>
                    <Bars download total />
                }
                .into_any()
            } else {
                let streamed = matches!(shown, Shown::Streaming(_));

                view! { <Picture address streamed early served total download failed /> }.into_any()
            }
        })
    }
}

fn waiting(early: RwSignal<Option<Address>>) -> AnyView {
    let early = early.with(|early| early.as_ref().map(|early| early.as_str().to_owned()));

    match early {
        Some(early) => view! { <img class=style::early src=early alt="" /> }.into_any(),
        None => view! { <p class=ui::style::hint>"Fetching it from the device…"</p> }.into_any(),
    }
}

#[allow(unreachable_pub)]
#[component]
fn Picture(
    address: String,
    streamed: bool,
    early: RwSignal<Option<Address>>,
    served: RwSignal<u64>,
    total: u64,
    download: Signal<Option<u64>>,
    failed: Callback<()>,
) -> impl IntoView {
    let loaded = RwSignal::new(false);

    view! {
        <img
            src=address
            alt=""
            hidden=move || !loaded.get()
            on:load=move |_| loaded.set(true)
            on:error=move |_| failed.run(())
        />
        {move || (!loaded.get()).then(|| waiting(early))}
        <Bars
            streamed=Signal::derive(move || (streamed && !loaded.get()).then(|| served.get()))
            download
            total
        />
    }
}

#[allow(unreachable_pub)]
#[component]
fn Bars(
    #[prop(optional)] streamed: Signal<Option<u64>>,
    download: Signal<Option<u64>>,
    total: u64,
) -> impl IntoView {
    view! {
        <div class=style::progress>
            {move || streamed.get().map(|done| view! { <Progress done total /> })}
            {move || {
                download.get().map(|done| view! { <Progress what="Downloading" done total /> })
            }}
        </div>
    }
}
