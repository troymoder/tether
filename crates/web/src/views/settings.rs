use leptos::prelude::*;
use protocol::{EmulatorOptions, InstanceId};

use crate::app::Global;
use crate::link::{ServerStoreFields, capture};
use crate::ui::dropdown::Choice;
use crate::ui::load::use_load;
use crate::ui::loadable::Loadable;

mod camera_field;
mod form;
mod hardware;
mod new;

pub(crate) use form::EmulatorForm;
pub(crate) use new::NewEmulator;

stylance::import_style!(
    #[allow(unreachable_pub)]
    style,
    "settings.module.scss"
);

/// Fails for an instance without an Android SDK.
pub(crate) fn options(
    instance: impl Fn() -> InstanceId + Send + Sync + 'static,
) -> Signal<Loadable<EmulatorOptions>> {
    let link = expect_context::<Global>().link;

    use_load(instance, move |instance| {
        capture::emulator_options(link, instance)
    })
}

/// `Loading` until tetherd has sent who is trusted.
pub(crate) fn here() -> Signal<Loadable<Vec<Choice<InstanceId>>>> {
    let link = expect_context::<Global>().link;

    // A memo, so a dropdown of these is only rebuilt when they change.
    Memo::new(move |_| {
        let instances = link.server.access().with(|access| match access {
            Some(access) => Loadable::Ready(
                access
                    .instances
                    .iter()
                    .map(|instance| instance.id.clone())
                    .collect::<Vec<_>>(),
            ),
            None => Loadable::Loading,
        });

        instances.map(|instances| {
            instances
                .into_iter()
                .map(|id| {
                    let name = format!("tetherd {}", link.instance_name(&id));

                    Choice::new(id, name)
                })
                .collect::<Vec<_>>()
        })
    })
    .into()
}

/// A system image's package path without the leading "system-images", such
/// as "android-36 · google_apis · x86_64".
pub(crate) fn image_name(image: &str) -> String {
    image.split(';').skip(1).collect::<Vec<_>>().join(" · ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_image_is_named_without_its_first_part() {
        assert_eq!(
            image_name("system-images;android-36;google_apis;x86_64"),
            "android-36 · google_apis · x86_64"
        );
    }
}
