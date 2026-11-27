use leptos::prelude::*;

use super::super::{Item, stream};
use crate::app::Picked;
use crate::link;
use crate::ui::action::use_action;

#[derive(Clone, Copy)]
pub(super) struct Download {
    pub fetched: Signal<Option<u64>>,
    pub run: Callback<()>,
}

pub(super) fn use_download(item: &Item) -> Download {
    let device = expect_context::<Picked>().device;
    let path = item.path.clone();
    let name = item.name().to_owned();
    let total = item.size;
    let read = RwSignal::new(0u64);
    let (busy, run) = use_action(Some("media"), move |()| {
        let device = device.get_untracked();
        let path = path.clone();
        let name = name.clone();

        read.set(0);

        async move {
            let file = stream::whole(&*device, &path, total, read).await?;

            link::save(&name, &file);

            // The browser shows the saved file, so no message is needed.
            Ok(None::<String>)
        }
    });

    Download {
        fetched: Signal::derive(move || busy.get().then(|| read.get())),
        run,
    }
}
